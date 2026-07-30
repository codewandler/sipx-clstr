//! `affinity-token` §10, executed verbatim.
//!
//! Every fixture below is quoted from the spec's own tables — the test keys, the fixed clock, the
//! claims, the nonces, the facts fixtures, and every minted byte. Nothing here is derived from the
//! implementation: if the two disagree, the spec is right and this file goes red, which is the
//! only arrangement under which "the vectors pass" means anything.
//!
//! The test keys MUST NOT appear in any deployment configuration (§10).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "test crate — non-negotiable #3 scopes those lints to library code"
)]

use std::fmt::Write as _;

use bytes::Bytes;
use chacha20poly1305::{AeadInPlace, ChaCha20Poly1305, KeyInit};
use pretty_assertions::assert_eq;
use sipx_clstr_affinity::{
    Algorithm, Claims, DecodeError, Direction, Expect, KeyEntry, KeySet, MAX_FACTS, MintError,
    MintKey, NonceSource, Reason, TOKEN_PARAM, TOKEN_PARAM_BUDGET, Token, Verdict,
    WORST_CASE_PARAM_LEN, decode_param_value, encode_param_value, mint, mint_with, verify,
};

// ---------------------------------------------------------------------------------------------
// §10 fixtures
// ---------------------------------------------------------------------------------------------

/// Test key `0x01`, `chacha20-poly1305`.
const K1: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
/// Test key `0x02`, `hmac-sha256-96`.
const K2: &str = "404142434445464748494a4b4c4d4e4f505152535455565758595a5b5c5d5e5f";

/// Mint time `T0` — 2026-07-28T12:00:00Z.
const T0: u32 = 1_785_240_000;
/// `expiry = T0 + L` with the default `L = 86 400`.
const EXPIRY: u32 = 1_785_326_400;
/// "`verify` at `now = T0 + 60`" — the clock every §10 vector uses unless it says otherwise.
const NOW: u32 = T0 + 60;

const N1: &str = "a0a1a2a3a4a5a6a7a8a9aaab";
const N2: &str = "b0b1b2b3b4b5b6b7b8b9babb";
const N3: &str = "c0c1c2c3c4c5c6c7c8c9cacb";
const N4: &str = "e0e1e2e3e4e5e6e7e8e9eaeb";
const N5: &str = "f0f1f2f3f4f5f6f7f8f9fafb";
const N6: &str = "d0d1d2d3d4d5d6d7d8d9dadb";
const N7: &str = "909192939495969798999a9b";
const N8: &str = "808182838485868788898a8b";

const FACTS8: &str = "deadbeefcafef00d";

/// Body plaintexts, §10: claims ‖ facts len ‖ facts.
const BODY_ORIG_F0: &str = "0000000700030005010009000000296a69eb4000";
const BODY_TERM_F0: &str = "0000000700030005020009000000296a69eb4000";
const BODY_ORIG_F8: &str = "0000000700030005010009000000296a69eb4008deadbeefcafef00d";

const AT_1: &str = "0101a0a1a2a3a4a5a6a7a8a9aaab0cab78584de5c2a8a10ffa14fcfad491f4b593bf8948568afa1022a7d5269545afdeb99b";
const AT_1_PARAM: &str = "AQGgoaKjpKWmp6ipqqsMq3hYTeXCqKEP-hT8-tSR9LWTv4lIVor6ECKn1SaVRa_euZs";
const AT_2: &str = "0101b0b1b2b3b4b5b6b7b8b9babb977d21affdeb898241769f86af4afa5a472b12a638519de0fd110e7abc7276fd595692d7";
const AT_2_PARAM: &str = "AQGwsbKztLW2t7i5uruXfSGv_euJgkF2n4avSvpaRysSpjhRneD9EQ56vHJ2_VlWktc";
const AT_3: &str =
    "0102c0c1c2c3c4c5c6c7c8c9cacb0000000700030005010009000000296a69eb4000de92133ef3f0478943fa12cf";
const AT_3_PARAM: &str = "AQLAwcLDxMXGx8jJyssAAAAHAAMABQEACQAAAClqaetAAN6SEz7z8EeJQ_oSzw";
const AT_5: &str = "0101e0e1e2e3e4e5e6e7e8e9eaeb0a28d0afb13885e699c2a026683367522a0ecff48b7c8ac253a2ed4452e800ba6d9eb3dce4871f5f0066e53b";
const AT_5_PARAM: &str =
    "AQHg4eLj5OXm5-jp6usKKNCvsTiF5pnCoCZoM2dSKg7P9It8isJTou1EUugAum2es9zkhx9fAGblOw";
const AT_6: &str = "0101f0f1f2f3f4f5f6f7f8f9fafbc04c3ec187abded25efe92fd872d11bb5c60e4d20282e14939807603959516badb1fbf91908d41944ffe6d515c19445099ffbc9c9529a209ef3c2f4698fddc959a720db8bf22c727899cb40ca72de5a4eb6261d830daf9ae55904a17c7c1ad1fdab57649";
const AT_6_PARAM: &str = "AQHw8fLz9PX29_j5-vvATD7Bh6ve0l7-kv2HLRG7XGDk0gKC4Uk5gHYDlZUWutsfv5GQjUGUT_5tUVwZRFCZ_7yclSmiCe88L0aY_dyVmnINuL8ixyeJnLQMpy3lpOtiYdgw2vmuVZBKF8fBrR_atXZJ";

const AT_12: &str = "0101b0b1b2b3b4b5b6b7b8b9babb977d21affdeb898242769f86af4afa5a472b12a62f5bedcfc9ed0982905f32de15a8be94";
const AT_13: &str = "0101d0d1d2d3d4d5d6d7d8d9dadbf44534e6a85ade8308fdf459d6d171297e6b63af3f00146ddc46d2501f50053fc4d9356e";
const AT_17: &str = "0101909192939495969798999a9baf8a6d9c880c3a7fe3377248adca0ec5edf2c9437df9b9d059cb51001ccd9e00980879389d498751caf6e0db";
const AT_18: &str = "0101808182838485868788898a8b7f4417c0d508dc301bb82ca3b66faabf4e75378ce3575789d320c778c1763476842492859cc4cb86dd56041dc380171ac2d3e66633195122164b34c2092df8b65b2588b694a32ad431ccbea9fb4118518a607eafe326c52773c0e51bd236b8953176862240";

/// Header length (§3, bytes 0–13).
const HEADER_LEN: usize = 14;
/// The AEAD tag, §4.
const AEAD_TAG_LEN: usize = 16;
/// The body's fixed part, §3 — the seven claim fields plus the `facts len` byte.
const BODY_FIXED_LEN: usize = 20;

// ---------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------

fn hex(text: &str) -> Vec<u8> {
    assert!(text.len().is_multiple_of(2), "odd-length hex fixture");
    (0..text.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&text[index..index + 2], 16).expect("hex fixture"))
        .collect()
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut text, byte| {
        let _ = write!(text, "{byte:02x}");
        text
    })
}

fn secret(text: &str) -> [u8; 32] {
    hex(text).try_into().expect("a 32 byte test secret")
}

fn nonce(text: &str) -> [u8; 12] {
    hex(text).try_into().expect("a 12 byte test nonce")
}

fn key_1() -> MintKey {
    MintKey::new(0x01, Algorithm::ChaCha20Poly1305, secret(K1))
}

fn key_2() -> MintKey {
    MintKey::new(0x02, Algorithm::HmacSha256_96, secret(K2))
}

/// "the full test key set" — both §10 keys, verify-valid across the whole fixture timeline.
fn keys() -> KeySet {
    KeySet::new(vec![
        KeyEntry::new(key_1(), 0, u32::MAX, true),
        KeyEntry::new(key_2(), 0, u32::MAX, false),
    ])
    .expect("the §10 key set is well formed")
}

/// `AT-10`: "a key set holding only key 0x02 (key 0x01 retired)".
fn keys_without_key_1() -> KeySet {
    KeySet::new(vec![KeyEntry::new(key_2(), 0, u32::MAX, true)]).expect("a one key set")
}

/// §10's fixed claims, for a given direction and facts region.
fn claims(direction: Direction, facts: &[u8]) -> Claims {
    Claims {
        tenant: 7,
        home_shard: 3,
        edge: 5,
        direction,
        media_node: 9,
        policy_version: 41,
        expiry: EXPIRY,
        module_facts: Bytes::copy_from_slice(facts),
    }
}

fn valid(direction: Direction, facts: &[u8]) -> Verdict {
    Verdict::Valid(claims(direction, facts))
}

/// Seal a body under the AEAD exactly as §4 specifies, independently of `mint`.
///
/// The negative vectors `AT-17` and `AT-18` are records `mint` refuses to produce, so the suite
/// has to build them from the plaintexts §10 prints — which also gives the round-trip vectors a
/// second, independent construction to agree with.
fn seal(key_id: u8, key: &[u8; 32], token_nonce: [u8; 12], body: &[u8]) -> Vec<u8> {
    let header = [&[0x01, key_id][..], &token_nonce[..]].concat();
    let mut buffer = body.to_vec();
    let tag = ChaCha20Poly1305::new(key.into())
        .encrypt_in_place_detached((&token_nonce).into(), &header, &mut buffer)
        .expect("the AEAD seals a body this size");
    [header, buffer, tag.to_vec()].concat()
}

fn mint_or_panic(direction: Direction, facts: &[u8], key: &MintKey, token_nonce: &str) -> Token {
    mint(&claims(direction, facts), key, nonce(token_nonce)).expect("the fixture claims mint")
}

// ---------------------------------------------------------------------------------------------
// Round-trip vectors
// ---------------------------------------------------------------------------------------------

#[test]
fn at_1_mint_encrypted_mode_orig_empty_facts() {
    let token = mint_or_panic(Direction::Originating, b"", &key_1(), N1);

    assert_eq!(to_hex(token.as_bytes()), AT_1);
    assert_eq!(token.len(), 50);
    assert_eq!(token.to_param_value(), AT_1_PARAM);

    // The header is cleartext, and the body is not: §4's confidentiality claim, checked rather
    // than described. `BODY_ORIG_F0` must appear nowhere in the token.
    assert_eq!(
        &to_hex(token.as_bytes())[..HEADER_LEN * 2],
        "0101a0a1a2a3a4a5a6a7a8a9aaab"
    );
    assert!(!to_hex(token.as_bytes()).contains(BODY_ORIG_F0));

    assert_eq!(
        verify(token.as_bytes(), &keys(), NOW, &Expect::new()),
        valid(Direction::Originating, b"")
    );
}

#[test]
fn at_2_mint_encrypted_mode_term_empty_facts() {
    let token = mint_or_panic(Direction::Terminating, b"", &key_1(), N2);

    assert_eq!(to_hex(token.as_bytes()), AT_2);
    assert_eq!(token.len(), 50);
    assert_eq!(token.to_param_value(), AT_2_PARAM);
    assert_eq!(
        to_hex(&seal(0x01, &secret(K1), nonce(N2), &hex(BODY_TERM_F0))),
        AT_2
    );
    assert_eq!(
        verify(token.as_bytes(), &keys(), NOW, &Expect::new()),
        valid(Direction::Terminating, b"")
    );

    // "Pair (AT-1, AT-2) in either pop order passes S9: equal claims, complementary directions,
    // distinct nonces, identical (empty) facts."
    let first = hex(AT_1);
    let second = hex(AT_2);
    assert_eq!(
        verify(&first, &keys(), NOW, &Expect::new().with_partner(&second)),
        valid(Direction::Originating, b"")
    );
    assert_eq!(
        verify(&second, &keys(), NOW, &Expect::new().with_partner(&first)),
        valid(Direction::Terminating, b"")
    );
}

#[test]
fn at_3_mint_authenticated_only_mode_orig_empty_facts() {
    let token = mint_or_panic(Direction::Originating, b"", &key_2(), N3);

    assert_eq!(to_hex(token.as_bytes()), AT_3);
    assert_eq!(token.len(), 46);
    assert_eq!(token.to_param_value(), AT_3_PARAM);

    // "body cleartext" — the opt-out this algorithm *is*, visible in the bytes.
    assert!(to_hex(token.as_bytes()).contains(BODY_ORIG_F0));

    // "verify at T0 + 60 → Valid with AT-1's claims."
    assert_eq!(
        verify(token.as_bytes(), &keys(), NOW, &Expect::new()),
        valid(Direction::Originating, b"")
    );
}

#[test]
fn at_4_parse_round_trip() {
    // "Decode AT-1's parameter value (base64url, unpadded) → exactly the 50 token bytes above"
    let bytes = decode_param_value(AT_1_PARAM).expect("AT-1's parameter decodes");
    assert_eq!(to_hex(&bytes), AT_1);
    assert_eq!(bytes.len(), 50);

    // "split: header = bytes 0–13, tag = last 16, ciphertext between"
    let header = &bytes[..HEADER_LEN];
    let tag = &bytes[bytes.len() - AEAD_TAG_LEN..];
    let ciphertext = &bytes[HEADER_LEN..bytes.len() - AEAD_TAG_LEN];
    assert_eq!(to_hex(header), format!("0101{N1}"));
    assert_eq!(to_hex(tag), "8948568afa1022a7d5269545afdeb99b");
    assert_eq!(
        to_hex(ciphertext),
        "0cab78584de5c2a8a10ffa14fcfad491f4b593bf"
    );

    // "AEAD open with key 0x01, nonce = bytes 2–13, AAD = bytes 0–13 → plaintext body equals
    // `ORIG, F=0` byte-exact" — proved through the public surface: sealing that plaintext under
    // the same header reproduces the ciphertext and the tag.
    assert_eq!(
        to_hex(&seal(0x01, &secret(K1), nonce(N1), &hex(BODY_ORIG_F0))),
        AT_1
    );

    // "facts len 0 = body length − 20 (S5)"
    assert_eq!(ciphertext.len() - BODY_FIXED_LEN, 0);

    // "Re-encoding the 50 bytes reproduces the parameter character-exact."
    assert_eq!(encode_param_value(&bytes), AT_1_PARAM);
}

#[test]
fn at_5_mint_with_an_eight_byte_module_facts_region() {
    let facts = hex(FACTS8);
    let token = mint_or_panic(Direction::Originating, &facts, &key_1(), N4);

    assert_eq!(to_hex(token.as_bytes()), AT_5);
    assert_eq!(token.len(), 58);
    assert_eq!(token.to_param_value(), AT_5_PARAM);
    assert_eq!(
        to_hex(&seal(0x01, &secret(K1), nonce(N4), &hex(BODY_ORIG_F8))),
        AT_5
    );

    // "the region returns verbatim, uninterpreted"
    assert_eq!(
        verify(token.as_bytes(), &keys(), NOW, &Expect::new()),
        valid(Direction::Originating, &facts)
    );
}

#[test]
fn at_6_mint_at_the_facts_ceiling() {
    // FACTS64 = "the 64 incrementing bytes".
    let facts: Vec<u8> = (0..64).collect();
    let token = mint_or_panic(Direction::Originating, &facts, &key_1(), N5);

    assert_eq!(to_hex(token.as_bytes()), AT_6);
    // "the budget vector: 114 raw bytes, 152 encoded chars, parameter 157 B ≤ 200 (§5)"
    assert_eq!(token.len(), 114);
    assert_eq!(token.to_param_value().len(), 152);
    let parameter = format!(";{TOKEN_PARAM}={}", token.to_param_value());
    assert_eq!(parameter.len(), 157);
    // proxy-behavior §7 F4's budget, which §5 verifies against this very vector. Three assertions
    // rather than one literal `<= 200`, because the row states a number *and* `AF-5` gave that
    // number one owner: the row's value is pinned to the constant, the constant bounds this
    // parameter, and this vector is by construction the worst case the layout can produce — so a
    // wider token fails here rather than on somebody's wire.
    assert_eq!(TOKEN_PARAM_BUDGET, 200);
    assert!(parameter.len() <= TOKEN_PARAM_BUDGET);
    assert_eq!(parameter.len(), WORST_CASE_PARAM_LEN);
    assert_eq!(token.to_param_value(), AT_6_PARAM);

    assert_eq!(
        verify(token.as_bytes(), &keys(), NOW, &Expect::new()),
        valid(Direction::Originating, &facts)
    );
}

/// A replayable nonce source — what the deterministic harness supplies where a driver supplies the
/// operating system's CSPRNG. Not named `at_*`: it proves the seam, not a row.
struct ScriptedNonces(Vec<[u8; 12]>);

impl NonceSource for ScriptedNonces {
    fn next_nonce(&mut self) -> [u8; 12] {
        assert!(!self.0.is_empty(), "the script ran out of nonces");
        self.0.remove(0)
    }
}

#[test]
fn the_injected_nonce_source_reproduces_the_pair() {
    // AGENTS.md rule 2: randomness enters as an injected source. `mint_with` is that seam, and
    // driving it with §10's own nonces reproduces `AT-1` and `AT-2` byte for byte — which is the
    // property that makes the whole suite replayable in the first place.
    let mut source = ScriptedNonces(vec![nonce(N1), nonce(N2)]);

    let orig = mint_with(&claims(Direction::Originating, b""), &key_1(), &mut source).unwrap();
    let term = mint_with(&claims(Direction::Terminating, b""), &key_1(), &mut source).unwrap();

    assert_eq!(to_hex(orig.as_bytes()), AT_1);
    assert_eq!(to_hex(term.as_bytes()), AT_2);
    // M4: the two entries of a pair MUST NOT share a nonce, so the source is drawn twice.
    assert_eq!(&to_hex(orig.as_bytes())[4..28], N1);
    assert_eq!(&to_hex(term.as_bytes())[4..28], N2);
}

// ---------------------------------------------------------------------------------------------
// Negative vectors
// ---------------------------------------------------------------------------------------------

#[test]
fn at_7_tampered_tag_byte() {
    let mut bytes = hex(AT_1);
    let last = bytes.len() - 1;
    bytes[last] ^= 0x01;
    assert_eq!(to_hex(&bytes[last..]), "9a");

    assert_eq!(
        verify(&bytes, &keys(), NOW, &Expect::new()),
        Verdict::Invalid(Reason::Tag)
    );
}

#[test]
fn at_8_tampered_ciphertext_byte() {
    // "AT-1 with ciphertext byte 14 XOR 0x01" — the AEAD authenticates the ciphertext, so a body
    // edit is a tag failure and never a decoded-but-wrong set of claims.
    let mut bytes = hex(AT_1);
    bytes[HEADER_LEN] ^= 0x01;
    assert_eq!(to_hex(&bytes[HEADER_LEN..=HEADER_LEN]), "0d");

    assert_eq!(
        verify(&bytes, &keys(), NOW, &Expect::new()),
        Verdict::Invalid(Reason::Tag)
    );
}

#[test]
fn at_9_expired_past_the_skew_allowance() {
    let bytes = hex(AT_1);

    // `now = expiry + S + 1`.
    assert_eq!(
        verify(&bytes, &keys(), 1_785_326_431, &Expect::new()),
        Verdict::Invalid(Reason::Expired)
    );
    // "Boundary: at `now = 1785326430` (= expiry + S) it is still Valid".
    assert_eq!(
        verify(&bytes, &keys(), 1_785_326_430, &Expect::new()),
        valid(Direction::Originating, b"")
    );
}

#[test]
fn at_10_unknown_key_id() {
    // "rejected before any cryptography" — the tag is intact here, and the verdict is still
    // unknown-key rather than tag, which is what "before" means operationally.
    assert_eq!(
        verify(&hex(AT_1), &keys_without_key_1(), NOW, &Expect::new()),
        Verdict::Invalid(Reason::UnknownKey)
    );
    assert_eq!(
        verify(&hex(AT_1), &keys(), NOW, &Expect::new()),
        valid(Direction::Originating, b"")
    );
}

#[test]
fn at_11_ingress_pinned_to_another_tenant() {
    assert_eq!(
        verify(&hex(AT_1), &keys(), NOW, &Expect::new().with_tenant(8)),
        Verdict::Invalid(Reason::Scope)
    );
    // The token's own tenant still passes, so this is scope and not a broken fixture.
    assert_eq!(
        verify(&hex(AT_1), &keys(), NOW, &Expect::new().with_tenant(7)),
        valid(Direction::Originating, b"")
    );
}

#[test]
fn at_12_pair_directions_not_complementary() {
    // "partner minted with the ORIG body under nonce N2 — both individually valid".
    let partner = mint_or_panic(Direction::Originating, b"", &key_1(), N2);
    assert_eq!(to_hex(partner.as_bytes()), AT_12);
    assert_eq!(
        verify(partner.as_bytes(), &keys(), NOW, &Expect::new()),
        valid(Direction::Originating, b"")
    );

    assert_eq!(
        verify(
            &hex(AT_1),
            &keys(),
            NOW,
            &Expect::new().with_partner(partner.as_bytes())
        ),
        Verdict::Invalid(Reason::Pair)
    );
}

#[test]
fn at_13_direction_byte_out_of_range() {
    // "the tag verifies; the value is still rejected" — sealed here rather than minted, because
    // `Direction` has no third value and that is the point of the vector.
    let mut body = hex(BODY_ORIG_F0);
    body[8] = 0x03;
    let bytes = seal(0x01, &secret(K1), nonce(N6), &body);
    assert_eq!(to_hex(&bytes), AT_13);

    assert_eq!(
        verify(&bytes, &keys(), NOW, &Expect::new()),
        Verdict::Invalid(Reason::Field)
    );
}

#[test]
fn at_14_version_byte_is_not_one() {
    // "before key lookup; the broken tag is never reached".
    let mut bytes = hex(AT_1);
    bytes[0] = 0x02;

    assert_eq!(
        verify(&bytes, &keys(), NOW, &Expect::new()),
        Verdict::Invalid(Reason::Structure)
    );
}

#[test]
fn at_15_truncated_below_the_aead_minimum() {
    let bytes = hex(AT_1);
    assert_eq!(bytes.len(), 50);

    assert_eq!(
        verify(&bytes[..49], &keys(), NOW, &Expect::new()),
        Verdict::Invalid(Reason::Length)
    );
}

#[test]
fn at_16_parameter_value_with_padding_appended() {
    // "Rejected at decode (§5: unpadded, alphabet-only) — S1 is never reached."
    let padded = format!("{AT_1_PARAM}==");
    assert_eq!(
        decode_param_value(&padded),
        Err(DecodeError::NotInAlphabet {
            byte: b'=',
            offset: AT_1_PARAM.len()
        })
    );
}

#[test]
fn at_17_facts_len_byte_disagrees_with_the_body() {
    // "facts len byte = 0x05 but an 8-byte region (body 28 B)".
    let mut body = hex(BODY_ORIG_F8);
    body[19] = 0x05;
    let bytes = seal(0x01, &secret(K1), nonce(N7), &body);
    assert_eq!(to_hex(&bytes), AT_17);

    // "framing: 5 ≠ 28 − 20".
    assert_eq!(body.len(), 28);
    assert_eq!(body.len() - 20, 8);
    assert_eq!(u32::from(body[19]), 5);

    assert_eq!(
        verify(&bytes, &keys(), NOW, &Expect::new()),
        Verdict::Invalid(Reason::Framing)
    );
}

#[test]
fn at_18_facts_region_over_the_length_maximum() {
    // "facts len byte = 0x41 (65) with 65 facts bytes — total 115 B".
    let facts: Vec<u8> = (0..65).collect();
    let mut body = hex(BODY_ORIG_F0);
    body[19] = 0x41;
    body.extend_from_slice(&facts);
    let bytes = seal(0x01, &secret(K1), nonce(N8), &body);
    assert_eq!(to_hex(&bytes), AT_18);

    // "115 > 114, the AEAD maximum — the 64-byte sub-budget is a length bound before it is ever a
    // framing check". S3 owns this, not S5: the verdict below is Length, never Framing.
    assert_eq!(bytes.len(), 115);
    assert_eq!(Algorithm::ChaCha20Poly1305.max_token_len(), 114);
    assert_eq!(MAX_FACTS, 64);

    assert_eq!(
        verify(&bytes, &keys(), NOW, &Expect::new()),
        Verdict::Invalid(Reason::Length)
    );

    // M8: `mint` refuses to produce it in the first place, whatever a hook framework assembled.
    let over_budget = claims(Direction::Originating, &facts);
    assert_eq!(
        mint(&over_budget, &key_1(), nonce(N8)),
        Err(MintError::FactsTooLong { len: 65 })
    );
}
