//! Turning a REGISTER request into a [`RegisterCommand`].
//!
//! Everything that can be judged from the message alone happens here — S5's canonicalization and
//! S6's wildcard validation — so that [`crate::process`] receives a command it can decide about
//! rather than a message it has to re-validate on every CAS retry.
//!
//! The tenant is a parameter, never read from the request: it comes from the accepting edge's
//! trust context (§4). A registrar that took its tenant from a URI would let a caller choose which
//! tenant's bindings to write.
//!
//! Authentication is the gate in front of all of it ([registrar-auth](
//! https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/registrar-auth.md) §2): [`admit`]
//! runs the decision *before* a command exists, so a request that fails it never reaches
//! [`crate::process`] at all. [`register_command`] is the open-tenant path — it yields
//! `principal: None`, because the only way to obtain a principal is to have proved one.

use bytes::Bytes;
use sipx_sip::headers::Address;
use sipx_sip::{HeaderName, Request};

use crate::aor::CanonicalAor;
use crate::auth::{AuthOutcome, ChallengeResponse, CredentialStore, Decision, TenantAuth};
use crate::binding::{Push, SourceAddr, Timestamp};
use crate::command::{ContactOp, ContactOps, RegisterCommand, Rejection};

/// What the edge knows that the message does not.
///
/// No `principal` field, deliberately. An identity is not something an edge *knows* — it is
/// something a decision *produced*, and the only producer is [`crate::auth::TenantAuth::decide`].
/// A settable field here would let a driver assert an identity nobody proved, which is the one
/// mistake in this area that no test downstream could catch.
#[derive(Debug, Clone, Default)]
pub struct EdgeContext {
    /// The tenant this listener belongs to.
    ///
    /// Read by [`register_command`] only. [`admit`] takes the tenant from the [`TenantAuth`] that
    /// authenticated instead, so the tenant and the principal can never name different ones.
    pub tenant: String,
    /// The observed source (RFC 3261 §18.2.1, RFC 3581).
    pub received: Option<SourceAddr>,
    /// Opaque flow reference, when the registration rides a client-initiated connection.
    pub flow_ref: Option<Bytes>,
}

/// What an edge does with a REGISTER once [registrar-auth](
/// https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/registrar-auth.md) §3 has spoken.
///
/// Three outcomes rather than a `Result`, because a challenge is not a failure: it is the first
/// half of a round trip the client is expected to complete, and typing it as an error would put it
/// on the same footing as a malformed request.
#[derive(Debug, Clone)]
pub enum Admission {
    /// The request became a command. Hand it to [`crate::process`].
    Command(Box<RegisterCommand>),
    /// Answer with this challenge (§3 A2, A6, A7). Nothing was processed, and nothing was stored.
    Challenge(ChallengeResponse),
    /// Refuse. Either the credentials named another protection space (§3 A3) or the message could
    /// not become a command at all.
    Reject(Rejection),
}

/// Decide whether a REGISTER may become a [`RegisterCommand`], and build it if it may.
///
/// This is the seam [registrar-auth §2](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/registrar-auth.md)
/// describes as "authentication runs before REGISTER processing, not inside it". The order is
/// load-bearing in one direction only: a request that will be challenged must not be parsed into a
/// command that could be stored, and a principal must not be attachable to a command except by
/// having come out of the decision immediately above it.
///
/// `auth` is `&mut` because it holds the replay window — one per tenant, living as long as the edge.
///
/// **The command's tenant is `auth`'s, not `edge`'s**, and `EdgeContext::tenant` is ignored here.
/// The principal of §5 is `<tenant>:<username>` with the tenant taken from the same `TenantAuth`, so
/// sourcing the command's tenant anywhere else would let a misconfigured edge write a binding under
/// tenant A carrying a principal that names tenant B — precisely the "authorization bug waiting for
/// a cross-tenant lookup" §5 gives the principal its shape to prevent. One source, no disagreement
/// possible.
pub fn admit(
    request: &Request,
    auth: &mut TenantAuth,
    credentials: &impl CredentialStore,
    edge: &EdgeContext,
    now: Timestamp,
) -> Admission {
    admit_audited(request, auth, credentials, edge, now).0
}

/// [`admit`], with the §9 audit record of what §3 decided alongside it.
///
/// **The record cannot be recovered from the [`Admission`] alone**, which is why this exists. A
/// `Decision::Proceed` that then fails to become a command arrives as `Admission::Reject`, with the
/// principal it proved already dropped — so a driver reading only the `Admission` cannot tell a
/// correctly authenticated REGISTER with a malformed `Contact` from one that was never
/// authenticated at all. `RG-15`'s first pass did exactly that and left a successful authentication
/// producing no record, which is the state [registrar-auth](
/// https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/registrar-auth.md) §9 L3 exists to
/// forbid: "an absent record and an unauthenticated one are different facts".
///
/// Any driver that keeps an audit trail wants this one; [`admit`] is for callers that do not.
pub fn admit_audited(
    request: &Request,
    auth: &mut TenantAuth,
    credentials: &impl CredentialStore,
    edge: &EdgeContext,
    now: Timestamp,
) -> (Admission, AuthOutcome) {
    let decision = auth.decide(request, credentials, now.as_secs());
    // Taken **before** the decision is consumed below. Everything after this point can discard the
    // decision freely; the record has already been made.
    let outcome = decision.outcome();

    let admission = match decision {
        Decision::Challenge(challenge) => Admission::Challenge(challenge),
        // §3 A3 — a realm this edge does not serve. `403` rather than another challenge, since two
        // ends that disagree about which protection space they are in would loop forever.
        Decision::Forbidden => Admission::Reject(Rejection::Forbidden(
            "the credentials name another protection space",
        )),
        // A1 hands us `None` and A5 hands us the principal of §5; both proceed, and the difference
        // is recorded on the binding rather than lost. `principal: None` on an open tenant is the
        // audit trail saying *unauthenticated*, not failing to say anything.
        Decision::Proceed { principal } => {
            let edge = EdgeContext {
                tenant: auth.tenant().to_owned(),
                ..edge.clone()
            };
            match command(request, &edge, principal, now) {
                Ok(cmd) => Admission::Command(Box::new(cmd)),
                Err(rejection) => Admission::Reject(rejection),
            }
        }
    };

    (admission, outcome)
}

/// Build a command from a REGISTER, or say why it cannot be built.
///
/// The **unauthenticated** path: the command it yields carries `principal: None`. An edge that
/// authenticates calls [`admit`] instead, which is the only way a principal is ever attached.
pub fn register_command(
    request: &Request,
    edge: &EdgeContext,
    now: Timestamp,
) -> Result<RegisterCommand, Rejection> {
    command(request, edge, None, now)
}

fn command(
    request: &Request,
    edge: &EdgeContext,
    principal: Option<Bytes>,
    now: Timestamp,
) -> Result<RegisterCommand, Rejection> {
    let call_id = request
        .headers
        .value(&HeaderName::CallId)
        .map(|value| Bytes::copy_from_slice(value.as_ref()))
        .ok_or(Rejection::BadRequest("a REGISTER needs a Call-ID"))?;

    let cseq = request
        .headers
        .value(&HeaderName::CSeq)
        .and_then(|value| {
            let text = String::from_utf8_lossy(&value);
            text.split_whitespace().next()?.parse::<u32>().ok()
        })
        .ok_or(Rejection::BadRequest("a REGISTER needs a numeric CSeq"))?;

    // S5 — the AoR is the `To` URI, canonicalized. Every §3 rejection lands as 400.
    let to = request
        .headers
        .value(&HeaderName::To)
        .ok_or(Rejection::BadRequest("a REGISTER needs a To header"))?;
    let to = Address::parse(&to, "To").map_err(|_| Rejection::BadRequest("a malformed To"))?;
    let aor = CanonicalAor::from_uri(&to.uri)
        .map_err(|_| Rejection::BadRequest("the To URI is not an address-of-record"))?;

    let expires_header = request
        .headers
        .value(&HeaderName::Expires)
        .and_then(|value| String::from_utf8_lossy(&value).trim().parse::<u32>().ok());

    let contacts = contact_ops(request)?;

    Ok(RegisterCommand {
        tenant: edge.tenant.clone(),
        aor,
        call_id,
        cseq,
        contacts,
        expires_header,
        // Typed since sipx `T-14` (v0.4.0). Before that `Path` fell through to `Other`, and looking
        // it up under the old name after the kernel started typing it finds nothing at all — a
        // registered route set that silently becomes empty, which is what `register_then_call`'s
        // Path scenario caught.
        path: route_values(request, &HeaderName::Path),
        supports_path: option_tags(request, &HeaderName::Supported)
            .iter()
            .any(|tag| tag == "path"),
        require: option_tags(request, &HeaderName::Require),
        received: edge.received.clone(),
        flow_ref: edge.flow_ref.clone(),
        principal,
        now,
    })
}

/// §5.4 W1/W2's first half: a `*` may not share the request with a URI.
fn contact_ops(request: &Request) -> Result<ContactOps, Rejection> {
    let raw: Vec<Bytes> = request
        .headers
        .get_all(&HeaderName::Contact)
        .map(|header| Bytes::copy_from_slice(header.value().as_ref()))
        .collect();

    let has_wildcard = raw.iter().any(|value| trim(value) == b"*");

    if has_wildcard {
        // W1 — a wildcard alongside anything else. Rejected rather than interpreted: "remove
        // everything" and "register this" in one request have no defined combined meaning.
        if raw.len() > 1 {
            return Err(Rejection::BadRequest(
                "a wildcard Contact cannot appear alongside another Contact",
            ));
        }
        return Ok(ContactOps::Wildcard);
    }

    let mut ops = Vec::new();
    for value in &raw {
        let addresses = Address::parse_list(value, "Contact")
            .map_err(|_| Rejection::BadRequest("a malformed Contact"))?;
        for address in addresses {
            ops.push(contact_op(&address)?);
        }
    }
    Ok(ContactOps::Explicit(ops))
}

fn contact_op(address: &Address) -> Result<ContactOp, Rejection> {
    let expires = address
        .param("expires")
        .and_then(|raw| String::from_utf8_lossy(raw).trim().parse::<u32>().ok());

    // `;q` is a decimal 0–1 with at most three digits (RFC 3261 §20.10), stored in thousandths so
    // ordering is integer comparison and never a float comparison.
    let q = match address.param("q") {
        None => None,
        Some(raw) => Some(parse_q(raw).ok_or(Rejection::BadRequest("a malformed Contact ;q"))?),
    };

    let uri = address.uri.clone();
    let push = push_params(address);

    Ok(ContactOp {
        verbatim: uri.to_bytes(),
        uri,
        expires,
        q,
        instance_id: address.param("+sip.instance").map(Bytes::copy_from_slice),
        reg_id: address
            .param("reg-id")
            .and_then(|raw| String::from_utf8_lossy(raw).trim().parse::<u32>().ok()),
        push,
    })
}

/// RFC 8599 parameters, kept only when the pair that identifies a device is complete.
fn push_params(address: &Address) -> Option<Push> {
    let provider = address.param("pn-provider")?;
    let prid = address.param("pn-prid")?;
    Some(Push {
        provider: Bytes::copy_from_slice(provider),
        prid: Bytes::copy_from_slice(prid),
        param: address.param("pn-param").map(Bytes::copy_from_slice),
    })
}

/// `q=0.7` → 700. Absent is not zero — that is §7 L2's job, not this function's.
fn parse_q(raw: &[u8]) -> Option<u16> {
    let text = String::from_utf8_lossy(raw);
    let text = text.trim();
    let (whole, fraction) = match text.split_once('.') {
        Some((whole, fraction)) => (whole, fraction),
        None => (text, ""),
    };
    if fraction.len() > 3 || !fraction.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let whole: u16 = whole.parse().ok()?;
    if whole > 1 {
        return None;
    }
    let mut thousandths = whole * 1_000;
    if whole == 1 && !fraction.trim_end_matches('0').is_empty() {
        return None; // 1.5 is not a preference
    }
    if whole == 0 {
        let padded = format!("{fraction:0<3}");
        thousandths = padded.parse().ok()?;
    }
    Some(thousandths)
}

/// The verbatim URI values of a route-style header, in header order (topmost first).
fn route_values(request: &Request, name: &HeaderName) -> Vec<Bytes> {
    request
        .headers
        .get_all(name)
        .filter_map(|header| Address::parse_list(&header.value(), "Path").ok())
        .flatten()
        .map(|address| address.uri.to_bytes())
        .collect()
}

/// Comma-separated option tags, lowercased.
fn option_tags(request: &Request, name: &HeaderName) -> Vec<String> {
    request
        .headers
        .get_all(name)
        .flat_map(|header| {
            String::from_utf8_lossy(&header.value())
                .split(',')
                .map(|tag| tag.trim().to_ascii_lowercase())
                .filter(|tag| !tag.is_empty())
                .collect::<Vec<_>>()
        })
        .collect()
}

fn trim(value: &[u8]) -> &[u8] {
    let start = value
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(value.len());
    let end = value
        .iter()
        .rposition(|byte| !byte.is_ascii_whitespace())
        .map_or(start, |index| index + 1);
    value.get(start..end).unwrap_or(&[])
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use sipx_sip::{Method, RequestBuilder, Uri};

    fn register(headers: Vec<(HeaderName, &str)>) -> Request {
        let uri = Uri::parse(Bytes::from_static(b"sip:atlanta.example")).unwrap();
        let mut builder = RequestBuilder::new(Method::Register, uri)
            .header(HeaderName::CallId, "i1")
            .unwrap()
            .header(HeaderName::CSeq, "1 REGISTER")
            .unwrap()
            .header(HeaderName::To, "<sip:alice@atlanta.example>")
            .unwrap();
        for (name, value) in headers {
            builder = builder.header(name, value.to_owned()).unwrap();
        }
        builder.build()
    }

    fn edge() -> EdgeContext {
        EdgeContext {
            tenant: "t1".to_owned(),
            ..EdgeContext::default()
        }
    }

    #[test]
    fn ls_r_8_a_wildcard_alongside_a_contact_is_malformed() {
        let request = register(vec![
            (HeaderName::Contact, "*"),
            (HeaderName::Contact, "<sip:alice@10.0.0.1>"),
        ]);
        let error =
            register_command(&request, &edge(), Timestamp::ZERO).expect_err("W1 must reject this");
        assert_eq!(error.status(), 400);
    }

    #[test]
    fn a_lone_wildcard_is_a_wildcard() {
        let request = register(vec![(HeaderName::Contact, "*")]);
        let cmd = register_command(&request, &edge(), Timestamp::ZERO).unwrap();
        assert!(matches!(cmd.contacts, ContactOps::Wildcard));
    }

    #[test]
    fn a_contact_keeps_its_verbatim_bytes_and_reads_its_parameters() {
        let request = register(vec![(
            HeaderName::Contact,
            "<sip:alice@10.0.0.1;transport=tcp>;expires=1800;q=0.7",
        )]);
        let cmd = register_command(&request, &edge(), Timestamp::ZERO).unwrap();
        let ContactOps::Explicit(ops) = &cmd.contacts else {
            panic!("expected explicit contacts");
        };
        let op = ops.first().expect("one contact");
        assert_eq!(op.expires, Some(1_800));
        assert_eq!(op.q, Some(700));
        // Verbatim: the stored contact is what the UA wrote, parameters and all, because that is
        // what the proxy later forwards.
        assert_eq!(op.verbatim.as_ref(), b"sip:alice@10.0.0.1;transport=tcp");
    }

    #[test]
    fn several_contacts_on_one_line_are_all_read() {
        let request = register(vec![(
            HeaderName::Contact,
            "<sip:a@10.0.0.1>;q=1.0, <sip:b@10.0.0.2>;q=0.5",
        )]);
        let cmd = register_command(&request, &edge(), Timestamp::ZERO).unwrap();
        let ContactOps::Explicit(ops) = &cmd.contacts else {
            panic!("expected explicit contacts");
        };
        assert_eq!(ops.len(), 2);
        assert_eq!(ops.first().and_then(|op| op.q), Some(1_000));
        assert_eq!(ops.get(1).and_then(|op| op.q), Some(500));
    }

    #[test]
    fn a_malformed_q_is_a_bad_request_rather_than_a_silent_default() {
        // Defaulting a malformed `q` to full preference would silently promote a contact the UA
        // meant to deprioritize.
        for bad in ["2.0", "1.5", "0.1234", "abc"] {
            assert!(parse_q(bad.as_bytes()).is_none(), "{bad} should not parse");
        }
        assert_eq!(parse_q(b"0.7"), Some(700));
        assert_eq!(parse_q(b"0.05"), Some(50));
        assert_eq!(parse_q(b"1"), Some(1_000));
        assert_eq!(parse_q(b"1.0"), Some(1_000));
        assert_eq!(parse_q(b"0"), Some(0));
    }

    #[test]
    fn supported_and_require_are_read_as_comma_separated_lists() {
        let request = register(vec![
            (HeaderName::Supported, "path, gruu"),
            (HeaderName::Require, "PATH"),
        ]);
        let cmd = register_command(&request, &edge(), Timestamp::ZERO).unwrap();
        assert!(cmd.supports_path);
        assert_eq!(cmd.require, vec!["path".to_owned()]);
    }

    #[test]
    fn a_to_uri_that_cannot_be_an_aor_is_rejected_at_parse_time() {
        // §3's rejections are S5's, so they land as 400 here rather than surfacing later as a
        // mis-keyed binding. A password in the AoR is N10.
        let uri = Uri::parse(Bytes::from_static(b"sip:atlanta.example")).unwrap();
        let request = RequestBuilder::new(Method::Register, uri)
            .header(HeaderName::CallId, "i1")
            .unwrap()
            .header(HeaderName::CSeq, "1 REGISTER")
            .unwrap()
            .header(HeaderName::To, "<sip:alice:secret@atlanta.example>")
            .unwrap()
            .build();

        let error = register_command(&request, &edge(), Timestamp::ZERO)
            .expect_err("a password in the AoR must be refused");
        assert_eq!(error.status(), 400);
    }

    // ------------------------------------------------------------------- the admission seam -----

    fn contact() -> Vec<(HeaderName, &'static str)> {
        vec![(HeaderName::Contact, "<sip:alice@10.0.0.1>;expires=3600")]
    }

    fn expect_command(admission: Admission) -> RegisterCommand {
        match admission {
            Admission::Command(cmd) => *cmd,
            other => panic!("expected a command, got {other:?}"),
        }
    }

    #[test]
    fn an_open_tenant_admits_with_no_principal() {
        // §3 A1 into location-service's `principal` column: the absence is *written*, so the audit
        // trail can say "nobody proved this" rather than merely have nothing to say.
        let mut auth = crate::auth::TenantAuth::open("t1");
        let credentials = crate::auth::InMemoryCredentials::new();
        let cmd = expect_command(admit(
            &register(contact()),
            &mut auth,
            &credentials,
            &edge(),
            Timestamp::ZERO,
        ));
        assert_eq!(cmd.principal, None);
    }

    #[test]
    fn an_unauthenticated_register_never_becomes_a_command() {
        // The order §2 requires: a challenged request must not exist as a `RegisterCommand` at all,
        // because a command is the thing `process` is willing to store.
        let mut auth = crate::auth::TenantAuth::required("t1", "atlanta.example", [3_u8; 32]);
        let credentials = crate::auth::InMemoryCredentials::new().with("t1", "alice", "s3cr3t");
        let admission = admit(
            &register(contact()),
            &mut auth,
            &credentials,
            &edge(),
            Timestamp::from_secs(1_800_000_000),
        );
        let Admission::Challenge(challenge) = admission else {
            panic!("a bare REGISTER must be challenged");
        };
        assert_eq!(challenge.status, 401);
        assert!(!challenge.stale);
    }

    #[test]
    fn the_principal_a_decision_yields_is_the_one_the_command_carries() {
        // The whole point of the seam: the identity on the command came out of `decide`, and there
        // is no other door it could have come through — `EdgeContext` has no field for one.
        const SECRET: [u8; 32] = [0x5a; 32];
        let mut auth = crate::auth::TenantAuth::required("t1", "atlanta.example", SECRET);
        let credentials =
            crate::auth::InMemoryCredentials::new().with("t1", "alice", "open sesame");
        let now = Timestamp::from_secs(1_800_000_000);

        let Admission::Challenge(challenge) =
            admit(&register(contact()), &mut auth, &credentials, &edge(), now)
        else {
            panic!("expected a challenge first");
        };

        // Answered with the kernel's own client responder, so this proves the two halves agree
        // rather than proving this file agrees with itself.
        let parsed = sipx_ua::auth::Challenge::parse(challenge.value.as_bytes(), false)
            .expect("the challenge parses");
        let authorization = sipx_ua::auth::respond(
            &parsed,
            &sipx_ua::auth::Credentials::new("alice", "open sesame"),
            "REGISTER",
            "sip:atlanta.example",
            1,
            "0a4f113b",
        );

        let mut headers: Vec<(HeaderName, &str)> = contact();
        headers.push((HeaderName::Authorization, &authorization));
        let cmd = expect_command(admit(
            &register(headers),
            &mut auth,
            &credentials,
            &edge(),
            now,
        ));
        assert_eq!(cmd.principal, Some(Bytes::from_static(b"t1:alice")));
    }

    #[test]
    fn the_command_takes_its_tenant_from_the_policy_that_authenticated() {
        // §5's reason for putting the tenant in the principal at all: a username is unique only
        // within one. If the command's tenant came from the edge while the principal's came from
        // the authenticator, a misconfigured listener would write `t-other`'s bindings under
        // `t1:alice` — two names for one row, which is how a cross-tenant lookup becomes a leak.
        const SECRET: [u8; 32] = [0x5a; 32];
        let mut auth = crate::auth::TenantAuth::required("t1", "atlanta.example", SECRET);
        let credentials =
            crate::auth::InMemoryCredentials::new().with("t1", "alice", "open sesame");
        let now = Timestamp::from_secs(1_800_000_000);
        // An edge that disagrees with its own authenticator — the configuration mistake this guards.
        let edge = EdgeContext {
            tenant: "t-other".to_owned(),
            ..EdgeContext::default()
        };

        let Admission::Challenge(challenge) =
            admit(&register(contact()), &mut auth, &credentials, &edge, now)
        else {
            panic!("expected a challenge first");
        };
        let parsed = sipx_ua::auth::Challenge::parse(challenge.value.as_bytes(), false)
            .expect("the challenge parses");
        let authorization = sipx_ua::auth::respond(
            &parsed,
            &sipx_ua::auth::Credentials::new("alice", "open sesame"),
            "REGISTER",
            "sip:atlanta.example",
            1,
            "0a4f113b",
        );

        let mut headers: Vec<(HeaderName, &str)> = contact();
        headers.push((HeaderName::Authorization, &authorization));
        let cmd = expect_command(admit(
            &register(headers),
            &mut auth,
            &credentials,
            &edge,
            now,
        ));
        assert_eq!(
            cmd.tenant, "t1",
            "the authenticator's tenant, not the edge's"
        );
        assert_eq!(cmd.principal, Some(Bytes::from_static(b"t1:alice")));
    }

    #[test]
    fn credentials_for_another_realm_are_refused_rather_than_challenged_again() {
        // §3 A3 lands as a `Rejection`, so the driver answers `403` through the same path every
        // other refusal takes instead of growing a second response shape.
        let mut auth = crate::auth::TenantAuth::required("t1", "atlanta.example", [3_u8; 32]);
        let credentials = crate::auth::InMemoryCredentials::new();
        let mut headers: Vec<(HeaderName, &str)> = contact();
        headers.push((
            HeaderName::Authorization,
            "Digest username=\"alice\", realm=\"biloxi.example\", nonce=\"x\", uri=\"sip:x\", \
             response=\"deadbeef\"",
        ));
        let admission = admit(
            &register(headers),
            &mut auth,
            &credentials,
            &edge(),
            Timestamp::from_secs(1_800_000_000),
        );
        let Admission::Reject(rejection) = admission else {
            panic!("another realm must be refused");
        };
        assert_eq!(rejection.status(), 403);
    }

    #[test]
    fn seconds_are_truncated_so_a_nonce_never_expires_early() {
        // The clock seam: the location service counts nanoseconds and digest counts seconds.
        assert_eq!(Timestamp::from_secs(7).as_secs(), 7);
        assert_eq!(Timestamp::from_nanos(7_999_999_999).as_secs(), 7);
    }

    #[test]
    fn a_register_without_a_call_id_or_cseq_is_malformed() {
        let uri = Uri::parse(Bytes::from_static(b"sip:atlanta.example")).unwrap();
        let bare = RequestBuilder::new(Method::Register, uri).build();
        assert_eq!(
            register_command(&bare, &edge(), Timestamp::ZERO)
                .expect_err("no Call-ID")
                .status(),
            400
        );
    }
}
