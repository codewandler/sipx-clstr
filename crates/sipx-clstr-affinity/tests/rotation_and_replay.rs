//! Key rotation (`affinity-token` §6) and the replay semantics (§9).
//!
//! §10 has no vector rows for either — §6 is a procedure a deployment executes over time and §9 is
//! a statement about what the implementation must *not* contain — so this file is `AF-4`'s
//! Acceptance rather than the vector table's, and it is written against the spec's own words.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "test crate — non-negotiable #3 scopes those lints to library code"
)]

use std::thread;

use bytes::Bytes;
use pretty_assertions::assert_eq;
use sipx_clstr_affinity::{
    Algorithm, Claims, DEFAULT_LIFETIME, DEFAULT_SKEW, Direction, Expect, KeyEntry, KeySet,
    KeySetError, MintKey, Reason, Verdict, mint, retirement_deadline, verify,
};

/// §10's test key `0x01`, `chacha20-poly1305` — key **A**, the outgoing one.
const SECRET_A: [u8; 32] = [
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
    0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
];
/// §10's test key `0x02` secret, carried by key **B** — the incoming one.
const SECRET_B: [u8; 32] = [
    0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x4b, 0x4c, 0x4d, 0x4e, 0x4f,
    0x50, 0x51, 0x52, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5a, 0x5b, 0x5c, 0x5d, 0x5e, 0x5f,
];

/// The moment K3 flips `mint` from A to B.
const T_SWITCH: u32 = 1_785_240_000;
/// location-service §5.2 E5's default tenant maximum registration expiry.
const E_MAX: u32 = 86_400;

fn key_a() -> MintKey {
    MintKey::new(0x01, Algorithm::ChaCha20Poly1305, SECRET_A)
}

fn key_b() -> MintKey {
    MintKey::new(0x02, Algorithm::ChaCha20Poly1305, SECRET_B)
}

fn claims(expiry: u32) -> Claims {
    Claims {
        tenant: 7,
        home_shard: 3,
        edge: 5,
        direction: Direction::Originating,
        media_node: 9,
        policy_version: 41,
        expiry,
        module_facts: Bytes::new(),
    }
}

// ---------------------------------------------------------------------------------------------
// §6 — rotation, distribute-then-activate
// ---------------------------------------------------------------------------------------------

#[test]
fn k1_distributes_b_before_any_node_mints_under_it() {
    // K1: "Add key B with `mint: false`, `verify_from ≤ now` to the configuration; reload every
    // node." The point of the ordering is that a node reloaded early can already *verify* a
    // B-token, so a reload wave can never produce a token some healthy edge rejects.
    let after_k1 = KeySet::new(vec![
        KeyEntry::new(key_a(), 0, T_SWITCH + 200_000, true),
        KeyEntry::new(key_b(), T_SWITCH - 3_600, T_SWITCH + 400_000, false),
    ])
    .unwrap();

    assert_eq!(after_k1.mint_key().map(MintKey::id), Some(0x01));

    let minted_under_b = mint(&claims(T_SWITCH + 86_400), &key_b(), [0xb0; 12]).unwrap();
    assert_eq!(
        verify(
            minted_under_b.as_bytes(),
            &after_k1,
            T_SWITCH,
            &Expect::new()
        ),
        Verdict::Valid(claims(T_SWITCH + 86_400))
    );
}

#[test]
fn k3_flips_the_mint_key_and_both_windows_stay_open() {
    // K3: "Flip `mint` from A to B in one config change." The overlap is the whole mechanism —
    // in-flight dialogs minted under A keep routing while new ones mint under B.
    let deadline = retirement_deadline(T_SWITCH, DEFAULT_LIFETIME, E_MAX, DEFAULT_SKEW);
    let after_k3 = KeySet::new(vec![
        KeyEntry::new(key_a(), 0, deadline, false),
        KeyEntry::new(key_b(), T_SWITCH - 3_600, deadline + 400_000, true),
    ])
    .unwrap();

    assert_eq!(after_k3.mint_key().map(MintKey::id), Some(0x02));

    let old = mint(&claims(T_SWITCH + 86_400), &key_a(), [0xa0; 12]).unwrap();
    let new = mint(&claims(T_SWITCH + 86_400), &key_b(), [0xb0; 12]).unwrap();
    let now = T_SWITCH + 60;
    assert_eq!(
        verify(old.as_bytes(), &after_k3, now, &Expect::new()),
        Verdict::Valid(claims(T_SWITCH + 86_400))
    );
    assert_eq!(
        verify(new.as_bytes(), &after_k3, now, &Expect::new()),
        Verdict::Valid(claims(T_SWITCH + 86_400))
    );
}

#[test]
fn k4_old_key_verification_ends_at_the_specified_boundary() {
    // K4: "Keep A verify-valid until `t_switch + max(L, E_max) + S` … then remove A."
    let deadline = retirement_deadline(T_SWITCH, DEFAULT_LIFETIME, E_MAX, DEFAULT_SKEW);
    assert_eq!(deadline, T_SWITCH + 86_400 + 30);

    let after_k3 = KeySet::new(vec![
        KeyEntry::new(key_a(), 0, deadline, false),
        KeyEntry::new(key_b(), T_SWITCH - 3_600, deadline + 400_000, true),
    ])
    .unwrap();

    // A token whose own expiry outlasts the key, so the boundary under test is S2's window and
    // not S6's clock — the key window governs regardless of what a token claims.
    let long_lived = claims(deadline + 100_000);
    let token = mint(&long_lived, &key_a(), [0xa0; 12]).unwrap();

    // Inclusive at the boundary: `verify_from ≤ now ≤ verify_until` (§8, S2).
    assert_eq!(
        verify(token.as_bytes(), &after_k3, deadline, &Expect::new()),
        Verdict::Valid(long_lived)
    );
    assert_eq!(
        verify(token.as_bytes(), &after_k3, deadline + 1, &Expect::new()),
        Verdict::Invalid(Reason::UnknownKey)
    );

    // And once A is removed at K4, the same token is an unknown key at any clock — §6: "Tokens …
    // under a retired key are hard-rejected", `403`, no fallback and no degraded mode.
    let after_k4 = KeySet::new(vec![KeyEntry::new(
        key_b(),
        T_SWITCH - 3_600,
        deadline + 400_000,
        true,
    )])
    .unwrap();
    assert_eq!(
        verify(token.as_bytes(), &after_k4, T_SWITCH + 60, &Expect::new()),
        Verdict::Invalid(Reason::UnknownKey)
    );
}

#[test]
fn the_retirement_bound_takes_the_larger_of_the_two_record_families() {
    // §6: `max(L, E_max) + S`, one term per record family. A deployment that raises registration
    // expiry above the token lifetime lengthens every rotation by the same amount, and the term
    // that is *not* the token's is exactly the one a token-only reading would drop.
    assert_eq!(retirement_deadline(1_000, 86_400, 86_400, 30), 87_430);
    assert_eq!(retirement_deadline(1_000, 600, 86_400, 30), 87_430);
    assert_eq!(retirement_deadline(1_000, 172_800, 86_400, 30), 173_830);
    // Saturating rather than wrapping: an absurd configuration must not retire a live key now.
    assert_eq!(retirement_deadline(u32::MAX, 86_400, 86_400, 30), u32::MAX);
}

#[test]
fn a_key_set_that_violates_section_6_does_not_load() {
    let wide = |key: MintKey, mint_flag: bool| KeyEntry::new(key, 0, u32::MAX, mint_flag);

    assert_eq!(
        KeySet::new(vec![wide(key_a(), false), wide(key_b(), false)]).unwrap_err(),
        KeySetError::NoMintKey
    );
    assert_eq!(
        KeySet::new(vec![wide(key_a(), true), wide(key_b(), true)]).unwrap_err(),
        KeySetError::MultipleMintKeys
    );
    // "No two entries share an `id` while both verify windows are open."
    assert_eq!(
        KeySet::new(vec![
            KeyEntry::new(key_a(), 0, 2_000, true),
            KeyEntry::new(key_a(), 1_999, 4_000, false),
        ])
        .unwrap_err(),
        KeySetError::OverlappingId(0x01)
    );
    assert_eq!(
        KeySet::new(vec![KeyEntry::new(key_a(), 4_000, 2_000, true)]).unwrap_err(),
        KeySetError::EmptyWindow(0x01)
    );
}

#[test]
fn an_id_may_be_reused_once_the_earlier_window_has_closed() {
    // "ids may wrap over the years, never overlap" — reuse is the design, overlap is the error.
    let reused = KeySet::new(vec![
        KeyEntry::new(key_a(), 0, 2_000, false),
        KeyEntry::new(
            MintKey::new(0x01, Algorithm::HmacSha256_96, SECRET_B),
            2_001,
            4_000,
            true,
        ),
    ])
    .unwrap();

    assert_eq!(
        reused.verify_key(0x01, 1_000).map(MintKey::algorithm),
        Some(Algorithm::ChaCha20Poly1305)
    );
    assert_eq!(
        reused.verify_key(0x01, 3_000).map(MintKey::algorithm),
        Some(Algorithm::HmacSha256_96)
    );
    assert_eq!(reused.verify_key(0x01, 2_000_000).map(MintKey::id), None);
}

// ---------------------------------------------------------------------------------------------
// §9 — replay semantics
// ---------------------------------------------------------------------------------------------

#[test]
fn re_presenting_one_token_verifies_every_time() {
    // §9: "Re-presenting the same token on **every** mid-dialog request is the mechanism, not an
    // attack." A dialog can carry thousands; every one of them must verify.
    let keys = KeySet::new(vec![KeyEntry::new(key_a(), 0, u32::MAX, true)]).unwrap();
    let expected = claims(T_SWITCH + 86_400);
    let token = mint(&expected, &key_a(), [0xa0; 12]).unwrap();

    for request in 0..2_048_u32 {
        assert_eq!(
            verify(token.as_bytes(), &keys, T_SWITCH + request, &Expect::new()),
            Verdict::Valid(expected.clone())
        );
    }
}

#[test]
fn no_replay_store_exists_behind_the_key_set() {
    // The structural half of §9's "an implementation MUST NOT add one": `verify` takes `&KeySet`,
    // so a nonce ledger would need either `&mut` — which would not compile at these call sites —
    // or interior mutability, which would cost `Sync` unless it were locked, and a lock on the
    // mid-dialog hot path is precisely the global state the token exists to remove.
    //
    // Two edges verifying one token concurrently is what that buys, and it is the shape M2's
    // criterion asks for: zero cross-node dialog lookups, zero shared state.
    let keys = KeySet::new(vec![KeyEntry::new(key_a(), 0, u32::MAX, true)]).unwrap();
    let expected = claims(T_SWITCH + 86_400);
    let token = mint(&expected, &key_a(), [0xa0; 12]).unwrap();
    let bytes = token.as_bytes().to_vec();

    thread::scope(|scope| {
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let keys = &keys;
                let bytes = &bytes;
                let expected = &expected;
                scope.spawn(move || {
                    for _ in 0..256 {
                        assert_eq!(
                            verify(bytes, keys, T_SWITCH + 60, &Expect::new()),
                            Verdict::Valid(expected.clone())
                        );
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().expect("no verifier panics");
        }
    });
}

#[test]
fn a_repeated_nonce_is_not_a_replay_defence() {
    // §9: "The nonce provides mint uniqueness and unlinkability (§3, M4) — it is **not** a replay
    // defense." Two tokens sharing a nonce is an M4 violation the *minting* side must not commit;
    // it is deliberately not something `verify` polices, because policing it would require the
    // ledger §9 forbids. Reusing a nonce here is the point of the test, not an example to copy.
    let keys = KeySet::new(vec![KeyEntry::new(key_a(), 0, u32::MAX, true)]).unwrap();
    let first = claims(T_SWITCH + 86_400);
    let mut second = first.clone();
    second.direction = Direction::Terminating;

    let one = mint(&first, &key_a(), [0xa0; 12]).unwrap();
    let two = mint(&second, &key_a(), [0xa0; 12]).unwrap();

    assert_eq!(
        verify(one.as_bytes(), &keys, T_SWITCH + 60, &Expect::new()),
        Verdict::Valid(first)
    );
    assert_eq!(
        verify(two.as_bytes(), &keys, T_SWITCH + 60, &Expect::new()),
        Verdict::Valid(second)
    );
}

#[test]
fn a_pair_sharing_a_nonce_fails_s9_even_though_each_token_verifies() {
    // The other side of the same coin: M4 says the two entries of a pair MUST NOT share a nonce,
    // and S9 is where that becomes checkable without any stored state — the partner is in the
    // message, so the comparison is local.
    let keys = KeySet::new(vec![KeyEntry::new(key_a(), 0, u32::MAX, true)]).unwrap();
    let orig = claims(T_SWITCH + 86_400);
    let mut term = orig.clone();
    term.direction = Direction::Terminating;

    let one = mint(&orig, &key_a(), [0xa0; 12]).unwrap();
    let shared = mint(&term, &key_a(), [0xa0; 12]).unwrap();
    let distinct = mint(&term, &key_a(), [0xb0; 12]).unwrap();
    let now = T_SWITCH + 60;

    assert_eq!(
        verify(
            one.as_bytes(),
            &keys,
            now,
            &Expect::new().with_partner(shared.as_bytes())
        ),
        Verdict::Invalid(Reason::Pair)
    );
    assert_eq!(
        verify(
            one.as_bytes(),
            &keys,
            now,
            &Expect::new().with_partner(distinct.as_bytes())
        ),
        Verdict::Valid(orig)
    );
}

#[test]
fn a_pair_whose_module_facts_differ_fails_s9() {
    // M3: the pair's claims are identical except direction and nonce, "**and byte-identical
    // module-facts region**". A pair that disagrees about the facts is two dialogs' state stapled
    // together, and the hooks downstream would read whichever entry was popped first.
    let keys = KeySet::new(vec![KeyEntry::new(key_a(), 0, u32::MAX, true)]).unwrap();
    let mut orig = claims(T_SWITCH + 86_400);
    orig.module_facts = Bytes::from_static(b"\xde\xad\xbe\xef");
    let mut term = orig.clone();
    term.direction = Direction::Terminating;
    term.module_facts = Bytes::from_static(b"\xde\xad\xbe\xee");

    let one = mint(&orig, &key_a(), [0xa0; 12]).unwrap();
    let other = mint(&term, &key_a(), [0xb0; 12]).unwrap();

    assert_eq!(
        verify(
            one.as_bytes(),
            &keys,
            T_SWITCH + 60,
            &Expect::new().with_partner(other.as_bytes())
        ),
        Verdict::Invalid(Reason::Pair)
    );
}

#[test]
fn a_partner_that_fails_s1_to_s7_fails_the_pair_check() {
    // S9: "the partner token MUST pass S1–S7 under the same rules". A partner that does not is a
    // pair failure, and the verdict never leaks *which* of its steps failed — one reason for the
    // presented token, one `403` on the wire.
    let keys = KeySet::new(vec![KeyEntry::new(key_a(), 0, u32::MAX, true)]).unwrap();
    let orig = claims(T_SWITCH + 86_400);
    let mut term = orig.clone();
    term.direction = Direction::Terminating;

    let one = mint(&orig, &key_a(), [0xa0; 12]).unwrap();
    let mut partner = mint(&term, &key_a(), [0xb0; 12])
        .unwrap()
        .as_bytes()
        .to_vec();
    let last = partner.len() - 1;
    partner[last] ^= 0x01;

    assert_eq!(
        verify(
            one.as_bytes(),
            &keys,
            T_SWITCH + 60,
            &Expect::new().with_partner(&partner)
        ),
        Verdict::Invalid(Reason::Pair)
    );
}

#[test]
fn a_configurable_skew_moves_the_expiry_boundary_and_nothing_else() {
    // §8 S6: "reject iff `now > expiry + S`, with skew allowance `S` = 30 s default, configurable".
    let keys = KeySet::new(vec![KeyEntry::new(key_a(), 0, u32::MAX, true)]).unwrap();
    let expected = claims(T_SWITCH + 86_400);
    let expiry = expected.expiry;
    let token = mint(&expected, &key_a(), [0xa0; 12]).unwrap();

    assert_eq!(DEFAULT_SKEW, 30);
    for (skew, at_boundary) in [(0_u32, expiry), (30, expiry + 30), (120, expiry + 120)] {
        let expect = Expect::new().with_skew(skew);
        assert_eq!(
            verify(token.as_bytes(), &keys, at_boundary, &expect),
            Verdict::Valid(expected.clone())
        );
        assert_eq!(
            verify(token.as_bytes(), &keys, at_boundary + 1, &expect),
            Verdict::Invalid(Reason::Expired)
        );
    }
}
