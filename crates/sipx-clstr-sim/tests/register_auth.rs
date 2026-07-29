//! Digest through the whole pipeline — `RG-2`'s acceptance, and M1's fifth exit criterion.
//!
//! One node running the **real** registrar: a phone registers, is challenged, answers, and its
//! binding is written under the principal the decision produced. Then it retransmits the identical
//! authenticated REGISTER — same nonce, same nonce-count, same digest — and must be authenticated
//! again ([registrar-auth §7](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/registrar-auth.md)
//! RA-R-1). Everything is genuine except the driver, which is this file, and the transport, which
//! is the simulated network.
//!
//! **Why this had to be reachable from the sans-IO side at all.** A retransmission is ordinary over
//! UDP, and a registrar that mistook one for a replay would answer `401` to a client that did
//! nothing wrong — a phone that re-registers every 60 seconds would drop off the network on the
//! first lost `200`. If the crypto lived in the driver, this harness could only *observe* the
//! criterion; because it lives below, the harness asserts it.
//!
//! The client half is the kernel's own responder, not a fixture: `sipx_ua::auth::respond` is the
//! same function a `sipx` phone uses, so what this proves is that the two halves agree.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use bytes::Bytes;
use sipx_clstr_registrar::{
    Admission, CanonicalAor, InMemoryCredentials, InMemoryStore, LocationStore, TenantAuth,
    TenantPolicy, Timestamp, admit, apply,
};
use sipx_clstr_sim::node::{Effect, Input, SimNode, TimerId, send};
use sipx_clstr_sim::{LinkKind, LinkPolicy, NodeId, Sim, SimTime};
use sipx_sip::{
    HeaderName, Message, Method, Request, RequestBuilder, Response, ResponseBuilder, StatusCode,
    Uri,
};

const TENANT: &str = "t1";
const REALM: &str = "atlanta.example";
const USER: &str = "alice";
const PASSWORD: &str = "open sesame";
const CNONCE: &str = "0a4f113b";
const REGISTRAR_URI: &str = "sip:atlanta.example";
/// The nonce key, fixed rather than drawn: the harness replays byte for byte from its seed, and a
/// secret that came from anywhere else would be the one input that did not.
const SECRET: [u8; 32] = [0x5a; 32];

/// The phone's re-send timer.
const RETRANSMIT: TimerId = TimerId(0);

fn uri(text: &str) -> Uri {
    Uri::parse(Bytes::copy_from_slice(text.as_bytes())).expect("a valid URI")
}

// ---------------------------------------------------------------------------------------------
// the edge: the real registrar, with the real authenticator in front of it
// ---------------------------------------------------------------------------------------------

#[derive(Debug)]
struct Edge {
    name: String,
    store: InMemoryStore,
    policy: TenantPolicy,
    auth: TenantAuth,
    credentials: InMemoryCredentials,
    /// Every admission this edge reached, in order — the seam the scenarios assert on.
    admissions: Vec<Verdict>,
}

/// What one REGISTER was admitted as, flattened to something a scenario can compare.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Verdict {
    /// Authenticated, under this principal. `None` is an open tenant (§3 A1).
    Admitted(Option<String>),
    /// Challenged, and whether the challenge said `stale`.
    Challenged { stale: bool },
    /// Refused, with the status.
    Rejected(u16),
}

impl Edge {
    fn new(name: &str, auth: TenantAuth) -> Self {
        Self {
            name: name.to_owned(),
            store: InMemoryStore::new(),
            policy: TenantPolicy::default(),
            auth,
            credentials: InMemoryCredentials::new().with(TENANT, USER, PASSWORD),
            admissions: Vec::new(),
        }
    }

    fn aor() -> CanonicalAor {
        CanonicalAor::parse(Bytes::from_static(b"sip:alice@atlanta.example"))
            .expect("a canonical AoR")
    }

    /// How many bindings the AoR currently holds.
    fn bindings(&self, now: Timestamp) -> usize {
        self.store.lookup(TENANT, &Self::aor(), now).len()
    }

    /// The principal recorded on the stored binding, if there is one.
    ///
    /// Read from the **binding**, not from a `Target`: §7's target set deliberately drops the
    /// principal, because who proved an identity is not a routing input. The audit fact lives on
    /// what was stored, which is the only place asserting it means anything.
    fn stored_principal(&self, now: Timestamp) -> Option<String> {
        let (set, _) = self.store.read(TENANT, &Self::aor());
        set.active(now)
            .next()
            .and_then(|binding| binding.principal.clone())
            .map(|principal| String::from_utf8_lossy(&principal).into_owned())
    }

    fn on_register(&mut self, from: NodeId, request: &Request, now: SimTime) -> Vec<Effect> {
        let clock = Timestamp::from_nanos(now.as_nanos());
        let context = sipx_clstr_registrar::EdgeContext {
            tenant: TENANT.to_owned(),
            ..sipx_clstr_registrar::EdgeContext::default()
        };

        // registrar-auth §2 — the decision runs *before* anything becomes a command, which is what
        // makes "nothing was stored" true of a challenged request rather than merely likely.
        let cmd = match admit(request, &mut self.auth, &self.credentials, &context, clock) {
            Admission::Challenge(challenge) => {
                self.admissions.push(Verdict::Challenged {
                    stale: challenge.stale,
                });
                let mut response = reply_message(request, challenge.status, "Unauthorized");
                let header = sipx_sip::Header::build(challenge.header, challenge.value)
                    .expect("a challenge header");
                response.headers.push(header);
                return vec![
                    Effect::Note(format!("challenged {}", challenge.status)),
                    send(from, Message::Response(response)),
                ];
            }
            Admission::Reject(rejection) => {
                self.admissions.push(Verdict::Rejected(rejection.status()));
                return vec![
                    Effect::Note(format!("refused {}", rejection.status())),
                    reply(from, request, rejection.status(), "Forbidden"),
                ];
            }
            Admission::Command(cmd) => cmd,
        };

        let principal = cmd
            .principal
            .as_ref()
            .map(|principal| String::from_utf8_lossy(principal).into_owned());
        self.admissions.push(Verdict::Admitted(principal.clone()));

        let applied = apply(&self.store, &cmd, &self.policy, 3);
        let status = applied.outcome.status();
        vec![
            Effect::Note(format!(
                "admitted {status} as {}",
                principal.as_deref().unwrap_or("nobody")
            )),
            reply(from, request, status, "OK"),
        ]
    }
}

impl SimNode for Edge {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_input(&mut self, now: SimTime, input: Input<'_>) -> Vec<Effect> {
        match input {
            Input::Message {
                from,
                message: Message::Request(request),
            } if request.method == Method::Register => self.on_register(from, request, now),
            _ => Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// the phone: the kernel's own client-side responder
// ---------------------------------------------------------------------------------------------

#[derive(Debug)]
struct Phone {
    name: String,
    edge: NodeId,
    password: String,
    /// What to do once the authenticated REGISTER has been answered.
    then: Encore,
    /// The authenticated REGISTER, kept verbatim so the retransmission is a *retransmission* and
    /// not a second request that happens to look similar.
    sent: Option<Request>,
    /// The challenge it was answered against, so a forgery can reuse the same nonce.
    challenge: Option<String>,
    /// Statuses received, in order.
    answers: Vec<u16>,
    /// How many challenges this phone has answered. One, in a healthy run: a client that is
    /// challenged twice has been told its credentials are wrong.
    answered: u32,
    /// An `Authorization` to present unprompted on the very first REGISTER, for the phone that
    /// arrives already believing it is somewhere else.
    opening: Option<String>,
    cseq: u32,
}

/// What the phone does after its first authenticated REGISTER is answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Encore {
    /// Nothing.
    Stop,
    /// Send the identical request again — RA-R-1's retransmission.
    Retransmit,
    /// Send the same nonce and nonce-count over a *different* signed URI, so the digest differs.
    /// RA-R-2: a captured credential aimed at another request.
    Forge,
}

impl SimNode for Phone {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_input(&mut self, _now: SimTime, input: Input<'_>) -> Vec<Effect> {
        match input {
            Input::Started => {
                let opening = self.opening.clone();
                vec![send(self.edge, Message::Request(self.register(opening)))]
            }
            Input::Message {
                message: Message::Response(response),
                ..
            } => self.on_response(response),
            Input::Timer(RETRANSMIT) => self.encore(),
            _ => Vec::new(),
        }
    }
}

impl Phone {
    fn on_response(&mut self, response: &Response) -> Vec<Effect> {
        let status = response.status.code();
        self.answers.push(status);

        if status == 401 {
            // A second challenge means the first answer was refused; answering again would be a
            // client retrying a bad guess, so the phone stops exactly as a real one does.
            if self.answered > 0 {
                return vec![Effect::Note(format!("{} gave up", self.name))];
            }
            self.answered += 1;
            let Some(challenge) = response
                .headers
                .value(&HeaderName::WwwAuthenticate)
                .map(|value| String::from_utf8_lossy(&value).into_owned())
            else {
                return Vec::new();
            };
            let request = self.register(Some(self.authorization(&challenge, REGISTRAR_URI)));
            self.challenge = Some(challenge);
            self.sent = Some(request.clone());
            return vec![
                Effect::Note(format!("{} answered the challenge", self.name)),
                send(self.edge, Message::Request(request)),
            ];
        }

        // The encore is armed on a timer rather than sent inline, so the retransmission is a
        // separate scheduling event and the scenario's virtual clock has advanced between the two.
        if status == 200
            && self.then != Encore::Stop
            && self.answers.iter().filter(|s| **s == 200).count() == 1
        {
            return vec![Effect::SetTimer {
                timer: RETRANSMIT,
                after: Duration::from_millis(500),
            }];
        }
        Vec::new()
    }

    fn encore(&mut self) -> Vec<Effect> {
        let Some(sent) = self.sent.clone() else {
            return Vec::new();
        };
        match self.then {
            Encore::Stop => Vec::new(),
            // Byte for byte the request the edge already authenticated. Same nonce, same `nc`,
            // same digest — which over UDP is what a lost `200` produces every day.
            Encore::Retransmit => vec![
                Effect::Note(format!("{} retransmitted", self.name)),
                send(self.edge, Message::Request(sent)),
            ],
            Encore::Forge => {
                let Some(challenge) = self.challenge.clone() else {
                    return Vec::new();
                };
                // Same nonce and the same `nc`, over a URI the first request did not sign. The
                // digest therefore differs, which is the shape of a captured credential being
                // pointed at something else.
                let forged = self.authorization(&challenge, "sip:biloxi.example");
                let mut request = sent;
                request.headers.remove_all(&HeaderName::Authorization);
                request.headers.push(
                    sipx_sip::Header::build(HeaderName::Authorization, forged)
                        .expect("an Authorization header"),
                );
                vec![
                    Effect::Note(format!("{} forged", self.name)),
                    send(self.edge, Message::Request(request)),
                ]
            }
        }
    }

    /// Answer a challenge the way a real client does — with the kernel's responder.
    fn authorization(&self, challenge: &str, signed_uri: &str) -> String {
        let parsed =
            sipx_ua::auth::Challenge::parse(challenge.as_bytes(), false).expect("a challenge");
        sipx_ua::auth::respond(
            &parsed,
            &sipx_ua::auth::Credentials::new(USER, &self.password),
            "REGISTER",
            signed_uri,
            1,
            CNONCE,
        )
    }

    fn register(&mut self, authorization: Option<String>) -> Request {
        self.cseq += 1;
        let mut builder = RequestBuilder::new(Method::Register, uri(REGISTRAR_URI))
            .header(HeaderName::CallId, format!("reg-{}", self.name))
            .expect("Call-ID")
            .cseq(self.cseq, &Method::Register)
            .expect("CSeq")
            .header(HeaderName::From, format!("<sip:{USER}@{REALM}>;tag=t"))
            .expect("From")
            .header(HeaderName::To, format!("<sip:{USER}@{REALM}>"))
            .expect("To")
            .header(
                HeaderName::Via,
                format!(
                    "SIP/2.0/UDP {}.sim;branch=z9hG4bK-r{}",
                    self.name, self.cseq
                ),
            )
            .expect("Via")
            .header(HeaderName::Contact, "<sip:alice@10.0.0.9>;expires=3600")
            .expect("Contact");
        if let Some(authorization) = authorization {
            builder = builder
                .header(HeaderName::Authorization, authorization)
                .expect("Authorization");
        }
        builder.build()
    }
}

fn reply_message(request: &Request, status: u16, reason: &str) -> Response {
    ResponseBuilder::to_request(
        request,
        StatusCode::new(status).expect("a valid status"),
        reason.to_owned(),
    )
    .expect("a response")
    .build()
}

fn reply(to: NodeId, request: &Request, status: u16, reason: &str) -> Effect {
    send(
        to,
        Message::Response(reply_message(request, status, reason)),
    )
}

// ---------------------------------------------------------------------------------------------
// scenarios
// ---------------------------------------------------------------------------------------------

const EDGE_NODE: NodeId = NodeId::from_index(0);
const ALICE: NodeId = NodeId::from_index(1);

fn scenario(seed: u64, auth: TenantAuth, password: &str, then: Encore) -> Sim {
    scenario_with(seed, auth, password, then, LinkPolicy::CLEAN, None)
}

fn scenario_with(
    seed: u64,
    auth: TenantAuth,
    password: &str,
    then: Encore,
    policy: LinkPolicy,
    opening: Option<String>,
) -> Sim {
    let mut sim = Sim::new(seed);
    sim.link_default(LinkKind::Datagram, policy);
    sim.add_node(Box::new(Edge::new("edge", auth)));
    sim.add_node(Box::new(Phone {
        name: "alice".to_owned(),
        edge: EDGE_NODE,
        password: password.to_owned(),
        then,
        sent: None,
        challenge: None,
        answers: Vec::new(),
        answered: 0,
        opening,
        cseq: 0,
    }));
    sim
}

fn closed_tenant() -> TenantAuth {
    TenantAuth::required(TENANT, REALM, SECRET)
}

#[test]
fn ra_d_4_credentials_for_another_realm_are_refused_and_bind_nothing() {
    // §3 A3 end to end: a phone that arrives already believing it is somewhere else is answered
    // `403` and is **not** challenged, because challenging again would loop between two ends that
    // disagree about which protection space they are in. Asserted here and not only as a unit
    // vector, since the thing worth proving is that the refusal reaches the wire as a refusal and
    // that nothing was stored on the way.
    let mut sim = scenario_with(
        0x5247_0207,
        closed_tenant(),
        PASSWORD,
        Encore::Stop,
        LinkPolicy::CLEAN,
        Some(
            "Digest username=\"alice\", realm=\"biloxi.example\", nonce=\"x\", \
             uri=\"sip:atlanta.example\", response=\"deadbeef\""
                .to_owned(),
        ),
    );
    sim.run_until_idle().expect("settles");

    let edge = sim.node::<Edge>(EDGE_NODE).expect("the edge");
    assert_eq!(
        edge.admissions,
        vec![Verdict::Rejected(403)],
        "another protection space is refused, not challenged\n{}",
        sim.trace().render()
    );
    assert_eq!(edge.bindings(Timestamp::from_secs(1)), 0);

    let phone = sim.node::<Phone>(ALICE).expect("the phone");
    assert_eq!(phone.answers, vec![403]);
}

#[test]
fn a_challenged_register_authenticates_and_binds_under_its_principal() {
    let mut sim = scenario(0x5247_0201, closed_tenant(), PASSWORD, Encore::Stop);
    sim.run_until_idle().expect("settles");

    let edge = sim.node::<Edge>(EDGE_NODE).expect("the edge");
    assert_eq!(
        edge.admissions,
        vec![
            Verdict::Challenged { stale: false },
            Verdict::Admitted(Some("t1:alice".to_owned())),
        ],
        "{}",
        sim.trace().render()
    );
    // §5 through location-service's `principal` column: the identity the decision produced is the
    // one on the stored binding, which is the half of this that no unit vector can reach.
    assert_eq!(
        edge.stored_principal(Timestamp::from_secs(1)),
        Some("t1:alice".to_owned())
    );
    assert_eq!(edge.bindings(Timestamp::from_secs(1)), 1);
}

#[test]
fn ra_r_1_a_retransmitted_register_authenticates_again() {
    // M1's fifth exit criterion. The same nonce, the same nonce-count and the same digest arrive
    // twice, and both must authenticate: over UDP a lost `200` produces exactly this, and an edge
    // that answered `401` to the second would drop a phone that did nothing wrong.
    let mut sim = scenario(0x5247_0202, closed_tenant(), PASSWORD, Encore::Retransmit);
    sim.run_until_idle().expect("settles");

    let edge = sim.node::<Edge>(EDGE_NODE).expect("the edge");
    assert_eq!(
        edge.admissions,
        vec![
            Verdict::Challenged { stale: false },
            Verdict::Admitted(Some("t1:alice".to_owned())),
            Verdict::Admitted(Some("t1:alice".to_owned())),
        ],
        "the retransmission must authenticate, not be refused as a replay\n{}",
        sim.trace().render()
    );

    // One REGISTER seen twice is one binding, not two — the retransmission wrote nothing.
    assert_eq!(edge.bindings(Timestamp::from_secs(1)), 1);

    // And the phone is told so. location-service §5.3 B4 classifies the retransmission as an
    // idempotent retry — the granted *duration* is what "same granted expiry base" compares — so
    // the second `200` is the same answer again, not a `500` from B5.
    let phone = sim.node::<Phone>(ALICE).expect("the phone");
    assert_eq!(
        phone.answers,
        vec![401, 200, 200],
        "a retransmission is a retry, not a second write\n{}",
        sim.trace().render()
    );
}

#[test]
fn ra_r_2_the_same_count_over_a_different_digest_is_refused() {
    // The twin that makes the test above mean something. If the edge accepted anything that
    // carried a nonce it had minted, RA-R-1 would pass for the wrong reason.
    let mut sim = scenario(0x5247_0203, closed_tenant(), PASSWORD, Encore::Forge);
    sim.run_until_idle().expect("settles");

    let edge = sim.node::<Edge>(EDGE_NODE).expect("the edge");
    assert_eq!(
        edge.admissions,
        vec![
            Verdict::Challenged { stale: false },
            Verdict::Admitted(Some("t1:alice".to_owned())),
            Verdict::Challenged { stale: false },
        ],
        "a reused count over a different digest is a replay\n{}",
        sim.trace().render()
    );
    assert_eq!(edge.bindings(Timestamp::from_secs(1)), 1);
}

#[test]
fn a_wrong_password_is_challenged_again_and_binds_nothing() {
    let mut sim = scenario(
        0x5247_0204,
        closed_tenant(),
        "not the password",
        Encore::Stop,
    );
    sim.run_until_idle().expect("settles");

    let edge = sim.node::<Edge>(EDGE_NODE).expect("the edge");
    assert_eq!(
        edge.admissions,
        vec![
            Verdict::Challenged { stale: false },
            Verdict::Challenged { stale: false },
        ],
        "{}",
        sim.trace().render()
    );
    // §2's ordering, observable: the request never became a command, so there was nothing to store.
    assert_eq!(edge.bindings(Timestamp::from_secs(1)), 0);
}

#[test]
fn an_open_tenant_binds_with_no_principal_recorded() {
    // §3 A1 end to end. `principal: None` is the audit trail saying *unauthenticated* rather than
    // failing to say anything — which is only true if the binding was written at all.
    let mut sim = scenario(
        0x5247_0205,
        TenantAuth::open(TENANT),
        PASSWORD,
        Encore::Stop,
    );
    sim.run_until_idle().expect("settles");

    let edge = sim.node::<Edge>(EDGE_NODE).expect("the edge");
    assert_eq!(edge.admissions, vec![Verdict::Admitted(None)]);
    assert_eq!(edge.bindings(Timestamp::from_secs(1)), 1);
    assert_eq!(edge.stored_principal(Timestamp::from_secs(1)), None);
}

#[test]
fn the_exchange_replays_byte_for_byte_under_jitter() {
    let policy = LinkPolicy::jittery(1, 30).with_duplication(0.2);
    for seed in 0..8_u64 {
        let mut first = scenario_with(
            seed,
            closed_tenant(),
            PASSWORD,
            Encore::Retransmit,
            policy,
            None,
        );
        let mut second = scenario_with(
            seed,
            closed_tenant(),
            PASSWORD,
            Encore::Retransmit,
            policy,
            None,
        );
        first.advance(Duration::from_secs(5)).expect("runs");
        second.advance(Duration::from_secs(5)).expect("runs");
        assert_eq!(
            first.trace().render(),
            second.trace().render(),
            "seed {seed} diverged"
        );
    }
}
