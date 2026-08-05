//! Location-service §9's LS-A REGISTER-admission vectors.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Mutex;

use bytes::Bytes;
use sipx_clstr_registrar::{
    Admission, CanonicalAor, EdgeContext, InMemoryCredentials, InMemoryStore, LocationStore,
    OpenRegistrationPolicy, RegistrationAuthorizations, RegistrationPolicy, RequestAuthority,
    TenantAuth, TenantPolicy, Timestamp, admit, admit_audited, apply, register_command,
};
use sipx_sip::{HeaderName, Host, Method, Request, RequestBuilder, Uri};

const TENANT: &str = "t1";
const DOMAIN: &str = "atlanta.example";
const SECRET: [u8; 32] = [0x18; 32];
const NOW: Timestamp = Timestamp::from_secs(1_800_000_000);

#[derive(Debug)]
struct Policy {
    served: Vec<Host>,
    authorizations: RegistrationAuthorizations,
    seen: Mutex<Vec<(String, Option<u16>)>>,
}

impl Policy {
    fn new(served: &[&str], authorizations: RegistrationAuthorizations) -> Self {
        Self {
            served: served
                .iter()
                .map(|host| {
                    Host::parse_hostport(&Bytes::copy_from_slice(host.as_bytes()))
                        .expect("a served host")
                        .0
                })
                .collect(),
            authorizations,
            seen: Mutex::new(Vec::new()),
        }
    }

    fn any(authorizations: RegistrationAuthorizations) -> Self {
        Self {
            served: Vec::new(),
            authorizations,
            seen: Mutex::new(Vec::new()),
        }
    }

    fn seen(&self) -> Vec<(String, Option<u16>)> {
        self.seen.lock().expect("seen lock").clone()
    }
}

impl RegistrationPolicy for Policy {
    fn serves(&self, tenant: &str, authority: &RequestAuthority) -> bool {
        self.seen
            .lock()
            .expect("seen lock")
            .push((authority.host().to_string(), authority.port()));
        tenant == TENANT
            && (self.served.is_empty()
                || self
                    .served
                    .iter()
                    .any(|served| served.equivalent(authority.host())))
    }

    fn authorizes(&self, tenant: &str, principal: Option<&[u8]>, aor: &CanonicalAor) -> bool {
        tenant == TENANT && self.authorizations.authorizes(principal, aor)
    }
}

fn request(
    request_uri: &str,
    to: &str,
    contact: Option<&str>,
    authorization: Option<&str>,
) -> Request {
    let mut builder = RequestBuilder::new(
        Method::Register,
        Uri::parse(Bytes::copy_from_slice(request_uri.as_bytes())).expect("a request URI"),
    )
    .header(HeaderName::CallId, "ls-a")
    .expect("Call-ID")
    .header(HeaderName::CSeq, "1 REGISTER")
    .expect("CSeq")
    .header(HeaderName::To, format!("<{to}>"))
    .expect("To");
    if let Some(contact) = contact {
        builder = builder
            .header(HeaderName::Contact, contact.to_owned())
            .expect("Contact");
    }
    if contact == Some("*") {
        builder = builder.header(HeaderName::Expires, "0").expect("Expires");
    }
    if let Some(authorization) = authorization {
        builder = builder
            .header(HeaderName::Authorization, authorization.to_owned())
            .expect("Authorization");
    }
    builder.build()
}

fn edge() -> EdgeContext {
    EdgeContext {
        tenant: TENANT.to_owned(),
        ..EdgeContext::default()
    }
}

fn status(admission: Admission) -> u16 {
    match admission {
        Admission::Reject(rejection) => rejection.status(),
        Admission::Challenge(challenge) => challenge.status,
        Admission::Command(_) => 200,
    }
}

#[test]
fn ls_a_1_a_served_to_does_not_rescue_an_unserved_request_uri() {
    let policy = Policy::new(&[DOMAIN], RegistrationAuthorizations::open());
    let mut auth = TenantAuth::required(TENANT, DOMAIN, SECRET);
    let (admission, outcome) = admit_audited(
        &request(
            "sip:biloxi.example",
            "sip:alice@atlanta.example",
            None,
            None,
        ),
        &mut auth,
        &InMemoryCredentials::new(),
        &policy,
        &edge(),
        NOW,
    );
    assert_eq!(status(admission), 404);
    assert_eq!(outcome, None, "S1 precedes authentication");
    assert_eq!(policy.seen(), vec![("biloxi.example".to_owned(), None)]);
}

#[test]
fn ls_a_2_a_served_request_uri_does_not_rescue_an_out_of_domain_to() {
    let policy = Policy::new(&[DOMAIN], RegistrationAuthorizations::open());
    let mut auth = TenantAuth::open(TENANT);
    let admission = admit(
        &request(
            "sip:atlanta.example",
            "sip:alice@biloxi.example",
            None,
            None,
        ),
        &mut auth,
        &InMemoryCredentials::new(),
        &policy,
        &edge(),
        NOW,
    );
    assert_eq!(status(admission), 404);
    assert_eq!(
        policy.seen(),
        vec![
            ("atlanta.example".to_owned(), None),
            ("biloxi.example".to_owned(), None),
        ]
    );
}

fn bob_fixture() -> (InMemoryStore, CanonicalAor) {
    let store = InMemoryStore::new();
    let bob =
        CanonicalAor::parse(Bytes::from_static(b"sip:bob@atlanta.example")).expect("Bob's AoR");
    let command = register_command(
        &request(
            "sip:atlanta.example",
            "sip:bob@atlanta.example",
            Some("<sip:bob@10.0.0.2>;expires=3600"),
            None,
        ),
        &OpenRegistrationPolicy,
        &edge(),
        NOW,
    )
    .expect("Bob registers");
    let applied = apply(&store, &command, &TenantPolicy::default(), 3);
    assert_eq!(applied.outcome.status(), 200);
    (store, bob)
}

// covers: LS-A-3
fn alice_attacks_bob(contact: &str) {
    let (store, bob) = bob_fixture();
    let before = store.read(TENANT, &bob).expect("read Bob");
    assert_eq!(before.1.0, 1, "the fixture is revision 1");

    let alice =
        CanonicalAor::parse(Bytes::from_static(b"sip:alice@atlanta.example")).expect("Alice's AoR");
    let policy = Policy::new(
        &[DOMAIN],
        RegistrationAuthorizations::restricted().allow(Bytes::from_static(b"t1:alice"), alice),
    );
    let credentials = InMemoryCredentials::new().with(TENANT, "alice", "alice-secret");
    let mut auth = TenantAuth::required(TENANT, DOMAIN, SECRET);
    let bare = request(
        "sip:atlanta.example",
        "sip:bob@atlanta.example",
        Some(contact),
        None,
    );
    let Admission::Challenge(challenge) =
        admit(&bare, &mut auth, &credentials, &policy, &edge(), NOW)
    else {
        panic!("Alice must first be challenged")
    };
    let parsed = sipx_ua::auth::Challenge::parse(challenge.value.as_bytes(), false)
        .expect("challenge parses");
    let authorization = sipx_ua::auth::respond(
        &parsed,
        &sipx_ua::auth::Credentials::new("alice", "alice-secret"),
        "REGISTER",
        "sip:atlanta.example",
        1,
        "ls-a-3",
    );
    let answered = request(
        "sip:atlanta.example",
        "sip:bob@atlanta.example",
        Some(contact),
        Some(&authorization),
    );
    assert_eq!(
        status(admit(
            &answered,
            &mut auth,
            &credentials,
            &policy,
            &edge(),
            NOW,
        )),
        403
    );

    // Admission has no store parameter at all; the equality pins both the binding bytes and the
    // revision in case a future driver moves this check after `apply`.
    assert_eq!(store.read(TENANT, &bob).expect("read Bob again"), before);
}

#[test]
fn ls_a_3_alice_cannot_explicitly_or_wildcard_register_bob() {
    alice_attacks_bob("<sip:mallory@10.6.6.6>;expires=3600");
    alice_attacks_bob("*");
}

#[test]
fn ls_a_4_an_open_tenant_is_an_explicit_policy_decision() {
    let request = request(
        "sip:atlanta.example",
        "sip:alice@atlanta.example",
        None,
        None,
    );
    let mut open = TenantAuth::open(TENANT);
    let allowed = Policy::new(&[DOMAIN], RegistrationAuthorizations::open());
    let Admission::Command(command) = admit(
        &request,
        &mut open,
        &InMemoryCredentials::new(),
        &allowed,
        &edge(),
        NOW,
    ) else {
        panic!("the explicit open policy permits None")
    };
    assert_eq!(command.principal, None);

    let denied = Policy::new(&[DOMAIN], RegistrationAuthorizations::restricted());
    let mut open = TenantAuth::open(TENANT);
    assert_eq!(
        status(admit(
            &request,
            &mut open,
            &InMemoryCredentials::new(),
            &denied,
            &edge(),
            NOW,
        )),
        403
    );
}

#[test]
fn ls_a_5_request_authorities_are_typed_not_split() {
    for request_uri in ["sip:EXAMPLE.test:5070", "sip:registrar@EXAMPLE.test:5070"] {
        let policy = Policy::any(RegistrationAuthorizations::open());
        register_command(
            &request(request_uri, "sip:alice@atlanta.example", None, None),
            &policy,
            &edge(),
            NOW,
        )
        .expect("the authority is valid");
        assert_eq!(
            policy.seen().first(),
            Some(&("EXAMPLE.test".to_owned(), Some(5070)))
        );
    }

    let ipv6 = Policy::any(RegistrationAuthorizations::open());
    register_command(
        &request(
            "sip:[2001:DB8::1]:5070",
            "sip:alice@atlanta.example",
            None,
            None,
        ),
        &ipv6,
        &edge(),
        NOW,
    )
    .expect("IPv6 is an authority");
    assert_eq!(
        ipv6.seen().first(),
        Some(&("[2001:db8::1]".to_owned(), Some(5070)))
    );

    let opaque = Policy::any(RegistrationAuthorizations::open());
    let error = register_command(
        &request("tel:+15550101", "sip:alice@atlanta.example", None, None),
        &opaque,
        &edge(),
        NOW,
    )
    .expect_err("a tel URI has no registrar authority");
    assert_eq!(error.status(), 404);
    assert!(
        opaque.seen().is_empty(),
        "policy was not given invented facts"
    );
}

#[test]
fn ls_a_6_the_binding_keeps_exactly_the_principal_policy_approved() {
    let aor = CanonicalAor::parse(Bytes::from_static(b"sip:line-one@atlanta.example"))
        .expect("the alias AoR");
    let policy = Policy::new(
        &[DOMAIN],
        RegistrationAuthorizations::restricted()
            .allow(Bytes::from_static(b"t1:alice"), aor.clone()),
    );
    let credentials = InMemoryCredentials::new().with(TENANT, "alice", "alice-secret");
    let mut auth = TenantAuth::required(TENANT, DOMAIN, SECRET);
    let bare = request(
        "sip:atlanta.example",
        "sip:line-one@atlanta.example",
        Some("<sip:alice@10.0.0.1>;expires=3600"),
        None,
    );
    let Admission::Challenge(challenge) =
        admit(&bare, &mut auth, &credentials, &policy, &edge(), NOW)
    else {
        panic!("challenge first")
    };
    let parsed = sipx_ua::auth::Challenge::parse(challenge.value.as_bytes(), false)
        .expect("challenge parses");
    let authorization = sipx_ua::auth::respond(
        &parsed,
        &sipx_ua::auth::Credentials::new("alice", "alice-secret"),
        "REGISTER",
        "sip:atlanta.example",
        1,
        "ls-a-6",
    );
    let answered = request(
        "sip:atlanta.example",
        "sip:line-one@atlanta.example",
        Some("<sip:alice@10.0.0.1>;expires=3600"),
        Some(&authorization),
    );
    let Admission::Command(command) =
        admit(&answered, &mut auth, &credentials, &policy, &edge(), NOW)
    else {
        panic!("the grant admits")
    };
    assert_eq!(command.principal.as_deref(), Some(&b"t1:alice"[..]));

    let store = InMemoryStore::new();
    assert_eq!(
        apply(&store, &command, &TenantPolicy::default(), 3)
            .outcome
            .status(),
        200
    );
    let (set, _) = store.read(TENANT, &aor).expect("read alias");
    assert_eq!(
        set.all()
            .first()
            .and_then(|binding| binding.principal.as_deref()),
        Some(&b"t1:alice"[..])
    );
}
