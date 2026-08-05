//! The echo endpoint — [e2e-probe §9](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/e2e-probe.md).
//!
//! The other end of the probe call: it registers in the test tenant like any other UA, answers a
//! probe INVITE with `200`, and copies the correlation marker back. That is the whole job, and doing
//! it as an ordinary UA is the point — the probe then exercises the real path (edge → authentication
//! → location lookup → forwarding → the answering leg) rather than a shortcut built for testing.
//!
//! **Signalling only.** The echo neither sends nor receives RTP, and never will in this process: a
//! media assertion, when it comes, goes through `MediaRelay` (§9 E4). The extension point is
//! [`EchoEndpoint::media_policy`], which today has exactly one value.
//!
//! **No proxy role links this.** The echo is a UAS, and §9's hard constraint is that a process
//! running `echo` runs no proxy role. This crate depends on neither `sipx-clstr-proxy` nor
//! `sipx-clstr-registrar`, which is that separation made structural rather than promised —
//! `tests/role_separation.rs` asserts it against the manifest.

use std::time::Duration;

use bytes::Bytes;
use sipx_sip::{
    Header, HeaderName, Message, Method, Request, RequestBuilder, Response, ResponseBuilder,
    StatusCode, Uri,
};

use crate::marker::{MARKER_HEADER, Marker};

/// What the echo will do about media.
///
/// One value, deliberately: the type exists so the extension point is visible in the code rather
/// than only in a document, and so adding a relay-mediated variant is a change to this enum instead
/// of a change to the answering path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MediaPolicy {
    /// Answer the signalling; touch no media. The only policy this process will ever have.
    #[default]
    SignallingOnly,
}

/// How the echo is configured.
#[derive(Debug, Clone)]
pub struct EchoConfig {
    /// The address-of-record it registers, in the test tenant.
    pub aor: String,
    /// The contact it registers.
    pub contact: String,
    /// Where to register.
    pub registrar: String,
    /// How long a registration lasts before it is refreshed.
    pub register_expires: u32,
    /// The host this echo puts in its `Via`, so responses can find their way back (§8.1.1.7).
    pub sent_by: String,
}

impl EchoConfig {
    /// A configuration with a one-hour registration.
    #[must_use]
    pub fn new(aor: &str, contact: &str, registrar: &str) -> Self {
        Self {
            aor: aor.to_owned(),
            contact: contact.to_owned(),
            registrar: registrar.to_owned(),
            register_expires: 3_600,
            sent_by: "echo.invalid".to_owned(),
        }
    }

    /// The same configuration, with the host this echo is reachable at.
    #[must_use]
    pub fn sent_by(mut self, host: &str) -> Self {
        host.clone_into(&mut self.sent_by);
        self
    }
}

/// Why the echo refused a call.
///
/// §9 E5: the echo answers **only** marked calls. An echo that answered anything would be an open
/// relay's more embarrassing cousin — reachable from wherever the test tenant is reachable from, and
/// happy to complete a call for anyone who found it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// No correlation marker, so this is not a probe call (E5).
    NotAProbeCall,
    /// The request is missing what a response must echo, so no response can be built.
    Unanswerable,
}

impl Refusal {
    /// The status to answer with, or `None` when nothing can be sent.
    #[must_use]
    pub fn status(self) -> Option<u16> {
        match self {
            // `403` rather than `404`: the address-of-record exists and is registered, and the call
            // is refused on policy. `404` would tell a caller to look elsewhere for something that
            // is right here.
            Self::NotAProbeCall => Some(403),
            Self::Unanswerable => None,
        }
    }
}

/// Something that happened to the echo.
#[derive(Debug)]
pub enum Input<'a> {
    /// The process started; register.
    Start,
    /// A request arrived.
    Request(&'a Request),
    /// The registration refresh timer fired.
    RefreshDue,
}

/// Something the echo wants done.
#[derive(Debug)]
pub enum Effect {
    /// Send a request — only ever a REGISTER.
    Send(Box<Request>),
    /// Answer the request that arrived.
    Respond(Box<Response>),
    /// Refresh the registration after this long.
    SetRefresh(Duration),
    /// Record a refusal, so an operator can see the echo turning traffic away.
    Refused(Refusal),
}

/// The echo, as a state machine.
///
/// Holds no per-call state (§9 E3): a call is answered from the request that arrived and nothing is
/// remembered afterwards. That is what makes it safe for the probe to abandon a dialog on a failure
/// path without leaving the echo wedged.
#[derive(Debug)]
pub struct EchoEndpoint {
    config: EchoConfig,
    media_policy: MediaPolicy,
    cseq: u32,
    /// How many calls it has answered, and how many it refused — the only state it keeps, and only
    /// because `ET-5` will want to publish it.
    answered: u64,
    refused: u64,
}

impl EchoEndpoint {
    /// An echo with this configuration.
    #[must_use]
    pub fn new(config: EchoConfig) -> Self {
        Self {
            config,
            media_policy: MediaPolicy::SignallingOnly,
            cseq: 0,
            answered: 0,
            refused: 0,
        }
    }

    /// What the echo does about media. One value; see [`MediaPolicy`].
    #[must_use]
    pub fn media_policy(&self) -> MediaPolicy {
        self.media_policy
    }

    /// How many probe calls it has answered.
    #[must_use]
    pub fn answered(&self) -> u64 {
        self.answered
    }

    /// How many calls it refused.
    #[must_use]
    pub fn refused(&self) -> u64 {
        self.refused
    }

    /// Feed one input; get the effects, in order.
    pub fn on_input(&mut self, input: &Input<'_>) -> Vec<Effect> {
        match input {
            Input::Start | Input::RefreshDue => self.register(),
            Input::Request(request) => self.on_request(request),
        }
    }

    fn register(&mut self) -> Vec<Effect> {
        self.cseq += 1;
        let Some(request) = self.register_request() else {
            return Vec::new();
        };
        vec![
            Effect::Send(Box::new(request)),
            // Refresh at half the granted lifetime, so one lost REGISTER does not lapse the binding.
            Effect::SetRefresh(Duration::from_secs(
                u64::from(self.config.register_expires) / 2,
            )),
        ]
    }

    fn on_request(&mut self, request: &Request) -> Vec<Effect> {
        match request.method {
            Method::Invite => self.on_invite(request),
            // A BYE ends a call the echo was not remembering anyway (E3), so it simply agrees; an
            // OPTIONS is answered because a UAS that ignored it would look unreachable. Same answer,
            // different reasons — hence one arm rather than a shared helper that would hide both.
            Method::Bye | Method::Options => answer(request, 200, "OK", None),
            // An ACK concludes a transaction; it is not a request anything answers.
            Method::Ack => Vec::new(),
            // Anything else is answered the way an ordinary UAS would, rather than ignored: silence
            // would make the echo look like a dead listener to whoever sent it.
            _ => answer(request, 405, "Method Not Allowed", None),
        }
    }

    fn on_invite(&mut self, request: &Request) -> Vec<Effect> {
        // E5 — only marked calls, and only from the tenant this echo lives in. The tenant boundary is
        // enforced by the platform (the test tenant has no trunks and no cross-tenant lookup); what
        // the echo enforces is that the call is a *probe* call.
        let Some(marker) = Marker::of(&Message::Request(request.clone())) else {
            self.refused += 1;
            let mut effects = vec![Effect::Refused(Refusal::NotAProbeCall)];
            effects.extend(answer(request, 403, "Forbidden", None));
            return effects;
        };

        self.answered += 1;
        // E2 — the marker is copied **verbatim**. Re-minting it, or normalising it, would break the
        // one thing the probe uses to tell our answer from anyone else's.
        answer(request, 200, "OK", Some(&marker))
    }
}

/// Build the response to a request, reflecting a marker when there is one.
///
/// A free function rather than a method: it reads nothing from the echo's state, and saying so keeps
/// the state the echo *does* keep — two counters — honestly small.
fn answer(request: &Request, status: u16, reason: &str, marker: Option<&Marker>) -> Vec<Effect> {
    // Nothing to answer *to*: the request is missing what a response echoes, and inventing those
    // fields would put a fabricated `Call-ID` on the wire.
    //
    // Still checked here even though the kernel's `ResponseBuilder::to_request` now refuses such
    // a request itself (it built one happily until sipx 1.0.0-beta.4): the builder's answer is a
    // build error, and what this endpoint owes its caller is a *decision* — `Unanswerable`, with
    // the refusal counted — not an error from a builder it happens to use. Deciding whether to
    // answer stays the application's job; the kernel refusing too is a second lock on the door.
    if !is_respondable(request) {
        return vec![Effect::Refused(Refusal::Unanswerable)];
    }
    let Some(code) = StatusCode::new(status) else {
        return Vec::new();
    };
    let Ok(builder) = ResponseBuilder::to_request(request, code, reason.to_owned()) else {
        return vec![Effect::Refused(Refusal::Unanswerable)];
    };
    let mut response = builder.build();

    if let Some(marker) = marker
        && let Ok(header) = Header::build(MARKER_HEADER, marker.header_bytes())
    {
        response.headers.push(header);
    }

    // E4 — no SDP answer describing media this process would then not send. A signalling-only
    // echo that advertised RTP ports would be lying to the offerer, and the offerer would wait
    // for audio that never comes.
    vec![Effect::Respond(Box::new(response))]
}

impl EchoEndpoint {
    fn register_request(&self) -> Option<Request> {
        let target = Uri::parse(Bytes::from(self.config.registrar.clone())).ok()?;
        RequestBuilder::new(Method::Register, target)
            .header(HeaderName::Via, self.via())
            .and_then(|b| b.header(HeaderName::CallId, format!("echo-{}", self.config.aor)))
            .and_then(|b| b.cseq(self.cseq, &Method::Register))
            .and_then(|b| b.header(HeaderName::From, format!("<{}>;tag=echo", self.config.aor)))
            .and_then(|b| b.header(HeaderName::To, format!("<{}>", self.config.aor)))
            .and_then(|b| {
                b.header(
                    HeaderName::Contact,
                    format!(
                        "<{}>;expires={}",
                        self.config.contact, self.config.register_expires
                    ),
                )
            })
            .map(sipx_sip::RequestBuilder::build)
            .ok()
    }

    /// The `Via` for the REGISTER about to go out.
    ///
    /// **Every request needs one** — RFC 3261 §8.1.1.7 makes `Via` how a response finds its way
    /// back, and the registrar cannot even build a response to a request it has no `Via` to echo.
    /// The probe's engine learned this from the end-to-end scenario; this is the same lesson
    /// applied to the only request the echo originates.
    ///
    /// The branch is derived from the `CSeq` rather than drawn at random: unique per transaction,
    /// which is all §8.1.1.7 asks of it, and reproducible, which is what the harness asks. The
    /// transport token is UDP because that is the only transport anything drives this endpoint
    /// over today; a driver that puts it on another transport widens `EchoConfig` rather than
    /// letting this line silently lie.
    fn via(&self) -> String {
        format!(
            "SIP/2.0/UDP {};branch=z9hG4bK-echo-{}",
            self.config.sent_by, self.cseq
        )
    }
}

/// Whether a response can be built for this request at all.
///
/// The fields RFC 3261 §8.2.6 requires a response to copy from the request it answers. Without them
/// there is nothing to answer to, and a response carrying invented values is worse than silence.
fn is_respondable(request: &Request) -> bool {
    [
        HeaderName::Via,
        HeaderName::CallId,
        HeaderName::CSeq,
        HeaderName::From,
        HeaderName::To,
    ]
    .iter()
    .all(|name| request.headers.get(name).is_some())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn config() -> EchoConfig {
        EchoConfig::new(
            "sip:echo@test.example",
            "sip:echo@10.8.8.8",
            "sip:registrar.test.example",
        )
    }

    fn invite(marker: Option<&Marker>) -> Request {
        let mut builder = RequestBuilder::new(
            Method::Invite,
            Uri::parse(Bytes::from_static(b"sip:echo@test.example")).unwrap(),
        )
        .header(HeaderName::CallId, "call-1")
        .unwrap()
        .cseq(1, &Method::Invite)
        .unwrap()
        .header(HeaderName::From, "<sip:probe@test.example>;tag=p")
        .unwrap()
        .header(HeaderName::To, "<sip:echo@test.example>")
        .unwrap()
        .header(HeaderName::Via, "SIP/2.0/UDP probe.test;branch=z9hG4bK-1")
        .unwrap();
        if let Some(marker) = marker {
            builder = builder
                .header(MARKER_HEADER, marker.header_bytes())
                .unwrap();
        }
        builder.build()
    }

    fn responses(effects: &[Effect]) -> Vec<&Response> {
        effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::Respond(response) => Some(response.as_ref()),
                _ => None,
            })
            .collect()
    }

    /// RFC 3261 §8.1.1.7: every request carries a `Via`, because it is how the response finds its
    /// way back. The probe engine learned this from the end-to-end scenario (`engine.rs`'s `via`
    /// doc records it); the echo's REGISTER shipped without one and nothing noticed until the
    /// kernel's response builder started refusing to answer a request it cannot echo a `Via` from.
    #[test]
    fn the_registration_carries_a_via_so_the_registrar_can_answer_it() {
        let mut echo = EchoEndpoint::new(config());
        let effects = echo.on_input(&Input::Start);

        let sent = effects
            .iter()
            .find_map(|effect| match effect {
                Effect::Send(request) => Some(request.as_ref()),
                _ => None,
            })
            .expect("a REGISTER");
        let via = sent
            .headers
            .value(&HeaderName::Via)
            .map(|v| String::from_utf8_lossy(&v).into_owned())
            .expect("a Via — a REGISTER without one cannot be answered");
        assert!(
            via.contains(";branch=z9hG4bK"),
            "the branch must carry the RFC 3261 magic cookie: {via}"
        );
        assert!(
            ResponseBuilder::to_request(sent, StatusCode::new(200).expect("200"), "OK".to_owned())
                .is_ok(),
            "the registrar must be able to build a response to it"
        );
    }

    #[test]
    fn e1_it_registers_as_an_ordinary_ua() {
        let mut echo = EchoEndpoint::new(config());
        let effects = echo.on_input(&Input::Start);

        let sent = effects
            .iter()
            .find_map(|effect| match effect {
                Effect::Send(request) => Some(request.as_ref()),
                _ => None,
            })
            .expect("a REGISTER");
        assert_eq!(sent.method, Method::Register);
        assert_eq!(
            sent.headers
                .value(&HeaderName::To)
                .map(|v| String::from_utf8_lossy(&v).trim().to_owned())
                .as_deref(),
            Some("<sip:echo@test.example>")
        );
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, Effect::SetRefresh(_))),
            "it must refresh, or the binding lapses"
        );
    }

    #[test]
    // 1800 s is exactly half of the granted `register_expires`, which is a SIP `Expires` value in
    // seconds; `from_mins(30)` would hide the halving this test exists to check.
    #[allow(clippy::duration_suboptimal_units)]
    fn the_refresh_is_well_inside_the_granted_lifetime() {
        // Refreshing at the deadline means one lost REGISTER lapses the binding, and the echo goes
        // unreachable while looking healthy.
        let mut echo = EchoEndpoint::new(config());
        let effects = echo.on_input(&Input::Start);
        let refresh = effects
            .iter()
            .find_map(|effect| match effect {
                Effect::SetRefresh(after) => Some(*after),
                _ => None,
            })
            .expect("a refresh");
        assert_eq!(refresh, Duration::from_secs(1_800));
    }

    #[test]
    fn e2_a_marked_invite_is_answered_200_with_the_marker_verbatim() {
        let marker = Marker::from_token("abc-123");
        let mut echo = EchoEndpoint::new(config());
        let effects = echo.on_input(&Input::Request(&invite(Some(&marker))));

        let answered = responses(&effects);
        let response = answered.first().expect("a response");
        assert_eq!(response.status.code(), 200);
        assert_eq!(
            Marker::of(&Message::Response((*response).clone())),
            Some(marker),
            "copied verbatim — re-minting it would break the one thing the probe checks"
        );
        assert_eq!(echo.answered(), 1);
    }

    #[test]
    fn e5_an_unmarked_invite_is_refused() {
        // An echo that answered anything would be reachable from wherever the test tenant is, and
        // happy to complete a call for whoever found it.
        let mut echo = EchoEndpoint::new(config());
        let effects = echo.on_input(&Input::Request(&invite(None)));

        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, Effect::Refused(Refusal::NotAProbeCall))),
            "the refusal is recorded, not silent"
        );
        assert_eq!(
            responses(&effects).first().map(|r| r.status.code()),
            Some(403),
            "403, not 404: the AoR exists and the call is refused on policy"
        );
        assert_eq!(echo.refused(), 1);
        assert_eq!(echo.answered(), 0);
    }

    #[test]
    fn e3_it_holds_no_state_between_calls() {
        // Which is what makes it safe for a probe to abandon a dialog on a failure path.
        let marker = Marker::from_token("m");
        let mut echo = EchoEndpoint::new(config());
        for _ in 0..5 {
            let effects = echo.on_input(&Input::Request(&invite(Some(&marker))));
            assert_eq!(
                responses(&effects).first().map(|r| r.status.code()),
                Some(200)
            );
        }
        assert_eq!(echo.answered(), 5);
    }

    #[test]
    fn e3_a_bye_is_answered_even_for_a_call_it_never_tracked() {
        let mut echo = EchoEndpoint::new(config());
        let mut bye = invite(None);
        bye.method = Method::Bye;
        let effects = echo.on_input(&Input::Request(&bye));
        assert_eq!(
            responses(&effects).first().map(|r| r.status.code()),
            Some(200)
        );
    }

    #[test]
    fn e4_no_sdp_answer_is_produced() {
        // A signalling-only echo that advertised RTP ports would be lying to the offerer, which then
        // waits for audio that never comes.
        let marker = Marker::from_token("m");
        let mut echo = EchoEndpoint::new(config());
        let effects = echo.on_input(&Input::Request(&invite(Some(&marker))));
        let response = responses(&effects).first().copied().expect("a response");
        assert!(
            response.body().is_empty(),
            "no SDP, and therefore no promise"
        );
        assert_eq!(echo.media_policy(), MediaPolicy::SignallingOnly);
    }

    #[test]
    fn an_unknown_method_is_answered_rather_than_ignored() {
        // Silence would make the echo look like a dead listener to whoever sent it — which is the
        // exact condition the probe exists to detect, so the echo must not counterfeit it.
        let mut echo = EchoEndpoint::new(config());
        let mut odd = invite(None);
        odd.method = Method::Other(Bytes::from_static(b"FROBNICATE"));
        let effects = echo.on_input(&Input::Request(&odd));
        assert_eq!(
            responses(&effects).first().map(|r| r.status.code()),
            Some(405)
        );
    }

    #[test]
    fn an_ack_is_not_answered() {
        let mut echo = EchoEndpoint::new(config());
        let mut ack = invite(None);
        ack.method = Method::Ack;
        assert!(echo.on_input(&Input::Request(&ack)).is_empty());
    }

    #[test]
    fn an_unanswerable_request_produces_no_response() {
        // Nothing to answer *to*. Inventing the fields a response echoes would put a fabricated
        // `Call-ID` on the wire.
        let bare = RequestBuilder::new(
            Method::Invite,
            Uri::parse(Bytes::from_static(b"sip:echo@test.example")).unwrap(),
        )
        .header(MARKER_HEADER, Marker::from_token("m").header_bytes())
        .unwrap()
        .build();

        let mut echo = EchoEndpoint::new(config());
        let effects = echo.on_input(&Input::Request(&bare));
        assert!(responses(&effects).is_empty());
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, Effect::Refused(Refusal::Unanswerable))),
            "and it is recorded rather than silently dropped"
        );
    }
}
