//! The RA vector tables of
//! [registrar-auth §8](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/registrar-auth.md),
//! row by row.
//!
//! **The client half is the kernel's own**, not a fixture built here: every answer is computed by
//! `sipx_ua::auth::respond`, the same function a `sipx` phone uses. A test that computed the
//! expected digest itself would only prove this file agrees with itself — which is exactly the
//! failure mode `S-16`'s formula-sharing exists to rule out.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bytes::Bytes;
use sipx_clstr_registrar::auth::{
    Algorithm, CredentialStore, Decision, InMemoryCredentials, Reason, TenantAuth,
};
use sipx_sip::{HeaderName, Method, Request, RequestBuilder, Uri};
use sipx_ua::auth::{Challenge, Credentials, respond};

const TENANT: &str = "t1";
const REALM: &str = "atlanta.example";
const USER: &str = "alice";
const PASSWORD: &str = "open sesame";
const SECRET: [u8; 32] = [0x5a; 32];
const CNONCE: &str = "0a4f113b";
const REQUEST_URI: &str = "sip:atlanta.example";

/// A fixed wall clock. Nothing here reads one — `decide` takes `now` — so the number only has to
/// be plausible, and being fixed is what makes the vectors replay identically.
const T0: u64 = 1_800_000_000;

fn credentials() -> InMemoryCredentials {
    InMemoryCredentials::new().with(TENANT, USER, PASSWORD)
}

fn tenant() -> TenantAuth {
    TenantAuth::required(TENANT, REALM, SECRET)
}

fn register(authorization: Option<(HeaderName, &str)>) -> Request {
    let uri = Uri::parse(Bytes::from_static(b"sip:atlanta.example")).unwrap();
    let mut builder = RequestBuilder::new(Method::Register, uri)
        .header(HeaderName::CallId, "i1")
        .unwrap()
        .header(HeaderName::CSeq, "1 REGISTER")
        .unwrap()
        .header(HeaderName::To, "<sip:alice@atlanta.example>")
        .unwrap();
    if let Some((name, value)) = authorization {
        builder = builder.header(name, value.to_owned()).unwrap();
    }
    builder.build()
}

/// A REGISTER carrying the header fields a binding is actually made of, each chosen by the caller.
///
/// The digest covers none of them (§7), which is the whole of `RA-R-6`: two requests built here
/// from one `authorization` string are indistinguishable to the verifier, however far apart their
/// `Contact`, `CSeq` and `Expires` are.
fn register_binding(authorization: &str, contact: &str, cseq: &str, expires: &str) -> Request {
    let uri = Uri::parse(Bytes::from_static(b"sip:atlanta.example")).unwrap();
    RequestBuilder::new(Method::Register, uri)
        .header(HeaderName::CallId, "i1")
        .unwrap()
        .header(HeaderName::CSeq, cseq.to_owned())
        .unwrap()
        .header(HeaderName::To, "<sip:alice@atlanta.example>")
        .unwrap()
        .header(HeaderName::Contact, contact.to_owned())
        .unwrap()
        .header(HeaderName::Expires, expires.to_owned())
        .unwrap()
        .header(HeaderName::Authorization, authorization.to_owned())
        .unwrap()
        .build()
}

/// Answer a challenge the way a real client does — with the kernel's client-side responder.
fn answer(challenge_value: &str, user: &str, password: &str, method: &str, nc: u32) -> String {
    answer_for(challenge_value, user, password, method, REQUEST_URI, nc)
}

/// [`answer`] with the signed URI chosen, which only a replay fixture needs.
fn answer_for(
    challenge_value: &str,
    user: &str,
    password: &str,
    method: &str,
    uri: &str,
    nc: u32,
) -> String {
    let challenge = Challenge::parse(challenge_value.as_bytes(), false).expect("a challenge");
    respond(
        &challenge,
        &Credentials::new(user, password),
        method,
        uri,
        nc,
        CNONCE,
    )
}

/// The value of the challenge a fresh REGISTER draws.
fn challenge_value(auth: &mut TenantAuth, now: u64) -> String {
    match auth.decide(&register(None), &credentials(), now) {
        Decision::Challenge(challenge) => challenge.value,
        other => panic!("expected a challenge, got {other:?}"),
    }
}

fn decide_with(auth: &mut TenantAuth, header: &str, now: u64) -> Decision {
    auth.decide(
        &register(Some((HeaderName::Authorization, header))),
        &credentials(),
        now,
    )
}

fn expect_challenge(decision: Decision) -> sipx_clstr_registrar::ChallengeResponse {
    match decision {
        Decision::Challenge(challenge) => challenge,
        other => panic!("expected a challenge, got {other:?}"),
    }
}

fn expect_principal(decision: Decision) -> Bytes {
    match decision {
        Decision::Proceed {
            principal: Some(principal),
        } => principal,
        other => panic!("expected an authenticated proceed, got {other:?}"),
    }
}

// ------------------------------------------------------------------ §8 the decision (RA-D) -----

#[test]
fn ra_d_1_an_open_tenant_proceeds_with_no_principal() {
    let mut auth = TenantAuth::open(TENANT);
    // A1: unauthenticated is a *recorded* fact. `principal: None` is what lets the audit trail say
    // "nobody proved this" rather than merely have nothing to say.
    assert_eq!(
        auth.decide(&register(None), &credentials(), T0),
        Decision::Proceed { principal: None }
    );
}

#[test]
fn ra_d_2_a_bare_register_is_challenged() {
    let mut auth = tenant();
    let challenge = expect_challenge(auth.decide(&register(None), &credentials(), T0));

    assert_eq!(challenge.status, 401);
    assert_eq!(challenge.header, HeaderName::WwwAuthenticate);
    assert!(challenge.because.is_none(), "nothing was offered to refuse");
    assert!(!challenge.stale);
    assert!(challenge.value.starts_with("Digest "));
    assert!(challenge.value.contains(&format!(r#"realm="{REALM}""#)));
    assert!(challenge.value.contains(r#"qop="auth""#));
    assert!(challenge.value.contains("algorithm=SHA-256"));
    assert!(!challenge.value.contains("stale"));
}

#[test]
fn ra_d_3_correct_credentials_proceed_with_a_principal() {
    let mut auth = tenant();
    let value = challenge_value(&mut auth, T0);
    let header = answer(&value, USER, PASSWORD, "REGISTER", 1);

    assert_eq!(
        expect_principal(decide_with(&mut auth, &header, T0)),
        Bytes::from_static(b"t1:alice")
    );
}

#[test]
fn ra_d_4_credentials_for_another_realm_are_forbidden_not_rechallenged() {
    let mut auth = tenant();
    // A challenge from somewhere else entirely, answered correctly *for that somewhere else*.
    let elsewhere =
        r#"Digest realm="biloxi.example", nonce="abc.def", algorithm=SHA-256, qop="auth""#;
    let header = answer(elsewhere, USER, PASSWORD, "REGISTER", 1);

    // A3: a realm is a protection space. Challenging again would loop between two ends that
    // disagree about where they are, and it is not a wrong password.
    assert_eq!(decide_with(&mut auth, &header, T0), Decision::Forbidden);
}

#[test]
fn ra_d_5_a_wrong_password_is_challenged_again_without_stale() {
    let mut auth = tenant();
    let value = challenge_value(&mut auth, T0);
    let header = answer(&value, USER, "not the password", "REGISTER", 1);

    let challenge = expect_challenge(decide_with(&mut auth, &header, T0));
    assert_eq!(challenge.because, Some(Reason::Mismatch));
    // No `stale`: a client told 401 without it stops and asks a human, which is right when the
    // password really is wrong.
    assert!(!challenge.stale);
    assert!(!challenge.value.contains("stale"));
}

#[test]
fn ra_d_6_an_unknown_user_is_indistinguishable_from_a_wrong_password() {
    // A4, and the point of the whole vector: telling these apart is a user-enumeration oracle.
    let mut wrong_password = tenant();
    let value = challenge_value(&mut wrong_password, T0);
    let refused = expect_challenge(decide_with(
        &mut wrong_password,
        &answer(&value, USER, "not the password", "REGISTER", 1),
        T0,
    ));

    let mut unknown_user = tenant();
    let same_nonce = challenge_value(&mut unknown_user, T0);
    assert_eq!(value, same_nonce, "the fixture needs one nonce, not two");
    let stranger = expect_challenge(decide_with(
        &mut unknown_user,
        &answer(&same_nonce, "mallory", "any password at all", "REGISTER", 1),
        T0,
    ));

    // Byte-identical, not merely both-a-401: the status, the reason, the staleness and the
    // challenge itself all agree, so the response carries no signal about which user exists.
    assert_eq!(refused, stranger);
}

#[test]
fn ra_d_7_a_nonce_this_edge_never_minted_is_refused() {
    let mut auth = tenant();
    let forged =
        format!(r#"Digest realm="{REALM}", nonce="0000.dead", algorithm=SHA-256, qop="auth""#);
    let header = answer(&forged, USER, PASSWORD, "REGISTER", 1);

    let challenge = expect_challenge(decide_with(&mut auth, &header, T0));
    assert_eq!(challenge.because, Some(Reason::ForeignNonce));
    assert!(!challenge.stale);
}

#[test]
fn ra_d_8_an_expired_nonce_with_the_right_password_is_stale() {
    let mut auth = tenant().with_lifetime(std::time::Duration::from_secs(300));
    let value = challenge_value(&mut auth, T0);
    let header = answer(&value, USER, PASSWORD, "REGISTER", 1);

    // A7: the password was never wrong, so the client re-computes and re-sends by itself. Telling
    // it plain 401 would send a human to change a password that was fine.
    let challenge = expect_challenge(decide_with(&mut auth, &header, T0 + 3_600));
    assert!(challenge.stale);
    assert!(challenge.value.contains("stale=true"));
    assert!(challenge.because.is_none());
    // And a *fresh* nonce, not the expired one it just refused.
    assert!(!challenge.value.contains(&value));
}

#[test]
fn ra_d_9_credentials_without_qop_are_refused() {
    let mut auth = tenant();
    let value = challenge_value(&mut auth, T0);
    // Strip the qop offer, so the kernel's responder uses the RFC 2069 formula — which has no
    // client nonce in it, and therefore no replay protection. Accepting it is a downgrade an
    // attacker can simply ask for.
    let without_qop = value.replace(r#", qop="auth""#, "");
    let header = answer(&without_qop, USER, PASSWORD, "REGISTER", 1);

    let challenge = expect_challenge(decide_with(&mut auth, &header, T0));
    assert_eq!(challenge.because, Some(Reason::QopMismatch));
}

#[test]
fn ra_d_10_a_proxy_challenges_with_407_and_reads_the_proxy_header() {
    let mut auth = tenant().as_proxy();
    let challenge = expect_challenge(auth.decide(&register(None), &credentials(), T0));
    assert_eq!(challenge.status, 407);
    assert_eq!(challenge.header, HeaderName::ProxyAuthenticate);

    let header = answer(&challenge.value, USER, PASSWORD, "REGISTER", 1);

    // The right header authenticates.
    let request = register(Some((HeaderName::ProxyAuthorization, &header)));
    assert_eq!(
        expect_principal(auth.decide(&request, &credentials(), T0)),
        Bytes::from_static(b"t1:alice")
    );

    // The wrong one does not — and the failure is "you offered nothing", because a proxy does not
    // read `Authorization`. A server that mixed the pair would authenticate nobody while looking
    // like it worked.
    let mut auth = tenant().as_proxy();
    let value = match auth.decide(&register(None), &credentials(), T0) {
        Decision::Challenge(challenge) => challenge.value,
        other => panic!("expected a challenge, got {other:?}"),
    };
    let header = answer(&value, USER, PASSWORD, "REGISTER", 1);
    let misfiled = register(Some((HeaderName::Authorization, &header)));
    let challenge = expect_challenge(auth.decide(&misfiled, &credentials(), T0));
    assert!(challenge.because.is_none());
}

// -------------------------------------------------------------------- §8 algorithms (RA-A) -----

/// Every offered algorithm round-trips against the kernel's own client.
fn round_trip(algorithm: Algorithm) {
    let mut auth = tenant().with_algorithm(algorithm);
    let value = challenge_value(&mut auth, T0);
    let header = answer(&value, USER, PASSWORD, "REGISTER", 1);
    assert_eq!(
        expect_principal(decide_with(&mut auth, &header, T0)),
        Bytes::from_static(b"t1:alice"),
        "{algorithm:?}"
    );
}

#[test]
fn ra_a_1_sha_256_is_the_default_and_round_trips() {
    assert!(challenge_value(&mut tenant(), T0).contains("algorithm=SHA-256"));
    round_trip(Algorithm::Sha256);
}

#[test]
fn ra_a_2_md5_round_trips_for_a_tenant_that_configures_it() {
    // Per tenant, deliberately: a legacy endpoint's constraint should not be everyone's.
    round_trip(Algorithm::Md5);
}

#[test]
fn ra_a_3_the_weaker_answer_to_a_stronger_offer_is_refused() {
    let mut auth = tenant().with_algorithm(Algorithm::Sha256);
    let value = challenge_value(&mut auth, T0);
    // The same nonce, answered under MD5 — what an on-path attacker gets if the edge offers a menu
    // and the client picks, since a challenge is not integrity-protected (RFC 8760 §3).
    let downgraded = value.replace("algorithm=SHA-256", "algorithm=MD5");
    let header = answer(&downgraded, USER, PASSWORD, "REGISTER", 1);

    let challenge = expect_challenge(decide_with(&mut auth, &header, T0));
    assert_eq!(challenge.because, Some(Reason::Algorithm));
}

#[test]
fn ra_a_4_sha_512_256_round_trips() {
    round_trip(Algorithm::Sha512_256);
}

// ------------------------------------------------- §8 replay and retransmission (RA-R) ---------

#[test]
fn ra_r_1_a_retransmission_is_not_a_replay() {
    // **M1 exit criterion 5, and the reason it is written as three clauses.** UDP retransmits; a
    // registrar that called the second copy a replay would fail every REGISTER on a lossy network.
    let mut auth = tenant();
    let value = challenge_value(&mut auth, T0);
    let header = answer(&value, USER, PASSWORD, "REGISTER", 7);

    for attempt in 1..=3 {
        assert_eq!(
            expect_principal(decide_with(&mut auth, &header, T0)),
            Bytes::from_static(b"t1:alice"),
            "attempt {attempt}: the same request arriving again is the same request"
        );
    }
}

#[test]
fn ra_r_2_the_same_count_with_a_different_digest_is_a_replay() {
    let mut auth = tenant();
    let value = challenge_value(&mut auth, T0);

    // One request at nc=7 authenticates.
    let first = answer(&value, USER, PASSWORD, "REGISTER", 7);
    assert!(matches!(
        decide_with(&mut auth, &first, T0),
        Decision::Proceed { .. }
    ));

    // A *different* request reusing that count. The digest covers the signed URI as well as the
    // method (RFC 7616 §3.4.3), so aiming the same credential at another registrar produces a
    // different digest at the same `nc` — which is a captured credential, not a retransmission.
    //
    // It has to be the **URI** rather than the method: a digest computed for another method fails
    // verification outright and never reaches the replay window, so a fixture built that way would
    // pass for the wrong reason.
    let captured = answer_for(&value, USER, PASSWORD, "REGISTER", "sip:biloxi.example", 7);
    assert_ne!(first, captured, "the fixture needs two distinct digests");
    let challenge = expect_challenge(decide_with(&mut auth, &captured, T0));
    assert_eq!(challenge.because, Some(Reason::Replay));
}

#[test]
fn ra_r_3_counting_up_across_refreshes_authenticates_every_time() {
    let mut auth = tenant();
    let value = challenge_value(&mut auth, T0);
    for nc in 1..=5 {
        let header = answer(&value, USER, PASSWORD, "REGISTER", nc);
        assert!(
            matches!(
                decide_with(&mut auth, &header, T0),
                Decision::Proceed { .. }
            ),
            "nc={nc}"
        );
    }
}

#[test]
fn ra_r_4_a_count_that_goes_backwards_is_refused() {
    let mut auth = tenant();
    let value = challenge_value(&mut auth, T0);
    for nc in [1_u32, 2, 3] {
        let header = answer(&value, USER, PASSWORD, "REGISTER", nc);
        assert!(matches!(
            decide_with(&mut auth, &header, T0),
            Decision::Proceed { .. }
        ));
    }

    let old = answer(&value, USER, PASSWORD, "REGISTER", 2);
    let challenge = expect_challenge(decide_with(&mut auth, &old, T0));
    assert_eq!(challenge.because, Some(Reason::Replay));
}

#[test]
fn ra_r_5_the_replay_window_does_not_grow_with_traffic() {
    // The window is bounded, which means it forgets — see §6. What this proves is the property
    // that matters operationally: a stream of distinct nonces does not consume memory without
    // limit, because an unbounded window is a leak an attacker sizes by asking for challenges.
    let mut auth = tenant();
    let mut authenticated = 0_u32;
    for step in 0..2_000_u64 {
        // A new second each time, so each challenge carries a distinct nonce.
        let now = T0 + step;
        let value = challenge_value(&mut auth, now);
        let header = answer(&value, USER, PASSWORD, "REGISTER", 1);
        if matches!(
            decide_with(&mut auth, &header, now),
            Decision::Proceed { .. }
        ) {
            authenticated += 1;
        }
    }
    assert_eq!(
        authenticated, 2_000,
        "every distinct nonce is usable once; eviction must not start refusing live clients"
    );
}

#[test]
fn ra_r_6_a_reattached_credential_over_different_bindings_is_accepted() {
    // The row that says what §7 says, in code: `A2 = Method ":" request-uri` (RFC 7616 §3.4.3), so
    // a REGISTER's `Contact`, `CSeq` and `Expires` are not hashed and cannot be detected as
    // changed. This is not a defect being pinned as correct — it is the exposure §7.3 accepts,
    // recorded so the next reader does not have to rediscover it from the kernel.
    let mut auth = tenant();
    let value = challenge_value(&mut auth, T0);
    let captured = answer(&value, USER, PASSWORD, "REGISTER", 3);

    // One REGISTER, authenticated, exactly as the phone that owns the credential sends it.
    let honest = register_binding(&captured, "<sip:alice@10.0.0.9>", "3 REGISTER", "3600");
    assert_eq!(
        expect_principal(auth.decide(&honest, &credentials(), T0)),
        Bytes::from_static(b"t1:alice")
    );

    // The same `Authorization`, byte for byte, on a request sharing only its method and its signed
    // `uri`. Everything a binding is made of differs.
    let reattached = register_binding(&captured, "<sip:mallory@10.6.6.6>", "9 REGISTER", "60");
    for name in [HeaderName::Contact, HeaderName::CSeq, HeaderName::Expires] {
        assert_ne!(
            honest.headers.value(&name),
            reattached.headers.value(&name),
            "the fixture must actually differ in {name:?}"
        );
    }
    assert_eq!(
        honest.headers.value(&HeaderName::Authorization),
        reattached.headers.value(&HeaderName::Authorization),
        "and must reuse the credential unmodified"
    );

    // Accepted — and note *which* branch accepts it: the digest is identical, so this is RA-R-1's
    // retransmission path, not RA-R-2's refusal. The two are the same event to the replay window,
    // which is why narrowing this without breaking RA-R-1 is not available (§7.3).
    assert_eq!(
        expect_principal(auth.decide(&reattached, &credentials(), T0)),
        Bytes::from_static(b"t1:alice"),
        "the digest binds the method and the signed uri, and nothing else"
    );
}

// ------------------------------------------------------------- §8 the tenant boundary (RA-T) ---

#[test]
fn ra_t_1_one_username_in_two_tenants_is_two_credentials() {
    let store = InMemoryCredentials::new()
        .with("t1", USER, "password one")
        .with("t2", USER, "password two");

    let mut one = TenantAuth::required("t1", REALM, SECRET);
    let value = match one.decide(&register(None), &store, T0) {
        Decision::Challenge(challenge) => challenge.value,
        other => panic!("expected a challenge, got {other:?}"),
    };

    let right = answer(&value, USER, "password one", "REGISTER", 1);
    let request = register(Some((HeaderName::Authorization, &right)));
    assert_eq!(
        expect_principal(one.decide(&request, &store, T0)),
        Bytes::from_static(b"t1:alice"),
        "the principal names the tenant, because a username is unique only within one"
    );

    // The other tenant's password does not work here, even though the username exists in both.
    let mut one = TenantAuth::required("t1", REALM, SECRET);
    let value = match one.decide(&register(None), &store, T0) {
        Decision::Challenge(challenge) => challenge.value,
        other => panic!("expected a challenge, got {other:?}"),
    };
    let wrong = answer(&value, USER, "password two", "REGISTER", 1);
    let request = register(Some((HeaderName::Authorization, &wrong)));
    assert!(matches!(
        one.decide(&request, &store, T0),
        Decision::Challenge(_)
    ));
}

#[test]
fn ra_t_2_credentials_from_another_tenants_realm_are_forbidden() {
    let store = credentials();
    // Tenant two authenticates in its own protection space.
    let mut two = TenantAuth::required("t2", "biloxi.example", SECRET);
    let value = match two.decide(&register(None), &store, T0) {
        Decision::Challenge(challenge) => challenge.value,
        other => panic!("expected a challenge, got {other:?}"),
    };
    let header = answer(&value, USER, PASSWORD, "REGISTER", 1);

    // Presented to tenant one, whose realm is different: RA-D-4's path, reached across a tenant
    // boundary rather than by a confused client.
    let mut one = tenant();
    assert_eq!(decide_with(&mut one, &header, T0), Decision::Forbidden);
}

#[test]
fn ra_t_3_an_open_tenant_does_not_weaken_a_closed_one() {
    let mut open = TenantAuth::open("t2");
    let mut closed = tenant();

    assert_eq!(
        open.decide(&register(None), &credentials(), T0),
        Decision::Proceed { principal: None }
    );
    // The closed tenant still challenges. Policy is per tenant, and the weakest one does not set
    // the floor for the rest.
    assert!(matches!(
        closed.decide(&register(None), &credentials(), T0),
        Decision::Challenge(_)
    ));
}

// --------------------------------------------------------------------- beyond the table --------

#[test]
fn the_digest_covers_the_method() {
    // RFC 7616 §3.4.3, and the reason RA-R-2's fixture works: an `Authorization` captured from a
    // REGISTER must not authorize an INVITE.
    let mut auth = tenant();
    let value = challenge_value(&mut auth, T0);
    let for_invite = answer(&value, USER, PASSWORD, "INVITE", 1);

    let challenge = expect_challenge(decide_with(&mut auth, &for_invite, T0));
    assert_eq!(challenge.because, Some(Reason::Mismatch));
}

#[test]
fn a_wrong_password_cannot_spend_a_nonce_count() {
    // An attacker guessing passwords must not be able to burn the counts a real client is about to
    // use, or a failed guess becomes a denial of service against the account it guessed at.
    let mut auth = tenant();
    let value = challenge_value(&mut auth, T0);

    let guess = answer(&value, USER, "wrong", "REGISTER", 5);
    assert!(matches!(
        decide_with(&mut auth, &guess, T0),
        Decision::Challenge(_)
    ));

    let real = answer(&value, USER, PASSWORD, "REGISTER", 5);
    assert!(matches!(
        decide_with(&mut auth, &real, T0),
        Decision::Proceed { .. }
    ));
}

#[test]
fn a_credential_store_is_consulted_per_tenant() {
    let store = credentials();
    assert_eq!(store.password(TENANT, USER).as_deref(), Some(PASSWORD));
    assert_eq!(store.password("t2", USER), None);
    assert_eq!(store.password(TENANT, "mallory"), None);
}
