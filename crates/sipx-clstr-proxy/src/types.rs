//! The sans-IO contract: what goes in, what comes out.
//!
//! Verbatim from [proxy-behavior §2](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/proxy-behavior.md),
//! with the one addition the spec's own §2 note implies: effects are **ordered**, and the driver
//! performs them in order. `Forward` always precedes the `SetTimer` that guards it, so a
//! retransmission or Timer C can never start before the thing it guards has gone out.

use std::time::Duration;

use bytes::Bytes;
use sipx_sip::{Request, Response};

/// One fork of one proxied request.
///
/// The value *is* the `Via` branch we minted for it, so the branch on the wire and the branch in
/// the response context are the same string — no second mapping to get out of step.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BranchId(pub String);

impl std::fmt::Display for BranchId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Somewhere to forward to, as the location service or the route plan produced it.
///
/// Deliberately this crate's own type rather than the registrar's: the proxy forwards to registered
/// contacts, to trunks (`RT-*`) and to plain Request-URIs, and coupling the forwarding core to one
/// producer of targets would make the other two awkward.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// The URI to put in the Request-URI, verbatim — parameters and all (F2).
    pub uri: Bytes,
    /// The route set to apply toward it, topmost first (a registration's `Path`).
    pub route_set: Vec<Bytes>,
    /// Preference in thousandths. Equal values fork in parallel; distinct values fork in sequence.
    pub q: u16,
}

/// What the proxy asked the driver to resolve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetQuery {
    /// The URI whose targets are wanted.
    pub uri: Bytes,
}

/// The verdict of verifying an affinity token found in a `Route` (P2, P3).
///
/// The verdict is an **input**, never something this crate computes: verification needs the key
/// set, a clock and a constant-time comparison, and all three are the driver's (AGENTS.md rule 2).
/// `sipx-clstr-affinity` is the library that produces it; the engine only consumes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenVerdict {
    /// The token verified. Its facts are the driver's to act on.
    Valid {
        /// The tenant the token names.
        tenant: String,
    },
    /// The token did not verify — tampered, expired, minted under a retired key, or out of scope.
    ///
    /// P3: a hard reject, with no fallback routing. Falling back would mean a forged token buys an
    /// attacker exactly the routing they would have got without one. Every failure collapses to
    /// this one verdict on purpose (affinity-token §8): the *reason* is telemetry, and putting it
    /// on the wire would hand an attacker a debugging oracle.
    Invalid,
}

/// What the platform `Route`s popped by P2 carried.
///
/// Produced by route preprocessing, which is the only thing that knows which `Route`s are ours.
///
/// **One case is deliberately absent.** affinity-token §8 also makes a *tokenless* platform `Route`
/// on a mid-dialog request a verification failure, on §5's premise that "there is no tokenless
/// platform Route on a mid-dialog request". That premise holds only once every edge mints — and a
/// deployment with no key set configured mints nothing, so its own `Record-Route` is tokenless and
/// its own mid-dialog requests present exactly the shape the rule rejects. Enforcing it before key
/// configuration is mandatory would answer `403` to every in-call message on such a node. So an
/// absent `aft` is reported as "nothing to verify" (`None` from preprocessing) rather than as a
/// failure, and closing that gap belongs to the story that makes a key set required.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenCarriage {
    /// The governing token: the **first-popped** one, whose direction names the presenting side.
    pub token: Bytes,
    /// The pair's other entry, when a second consecutive platform `Route` was popped carrying one
    /// — affinity-token §8 S9's pair check.
    pub partner: Option<Bytes>,
}

/// The `Record-Route` pair a dialog-forming request is stamped with — affinity-token §7 M1/M2.
///
/// One entry per side, one fresh token per entry, claims identical apart from direction and nonce.
/// Both are minted by the driver, because a nonce is randomness and randomness is injected (§7 M4);
/// the engine only decides *whether* and *where* they go on the wire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordRouteTokens {
    /// The entry facing the originating side — the side that sent the dialog-forming request.
    pub originating: Bytes,
    /// The entry facing the terminating side — the side it is forwarded to.
    pub terminating: Bytes,
}

/// A timer the proxy owns, above the transaction layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProxyTimer {
    /// RFC 3261 §16.8's Timer C: an INVITE branch that has stopped producing provisionals.
    C,
}

/// Something that happened to a proxied request.
#[derive(Debug)]
pub enum Input {
    /// A request arrived on the server transaction.
    Upstream(Box<Request>),
    /// A branch answered.
    BranchResponse(Box<Response>, BranchId),
    /// A branch's transport failed — §16.9, which the engine treats as `503` from that branch.
    BranchTransportError(BranchId),
    /// The targets the engine asked for.
    TargetsResolved(Vec<Target>),
    /// The targets could **not** be determined: the location service reported failure rather than an
    /// answer (location-service §6 K7, §7 L8).
    ///
    /// Separate from `TargetsResolved(vec![])` because they are different facts, and §7 gives them
    /// different responses: an empty answer is what this platform knows about the callee (`480`),
    /// while this is what it does not know about itself (`503`). One input for both would put the
    /// distinction back in the driver, where each caller would have to rediscover it.
    TargetsUnavailable,
    /// A token found in a `Route` was verified, or rejected — the answer to
    /// [`Effect::VerifyToken`], and the input P2 turns into P3.
    TokenFact(TokenVerdict),
    /// The `Record-Route` pair to stamp a dialog-forming request with (F4, affinity-token §7 M1).
    ///
    /// Fed **before** [`Input::Upstream`], because minting needs a nonce and a clock and neither is
    /// the engine's to reach. A driver that supplies none — a node with no key set — gets the
    /// tokenless single `Record-Route` the platform has always emitted, rather than a stalled
    /// request: no token is a missing feature, and there is no decision waiting on it.
    TokensMinted(Box<RecordRouteTokens>),
    /// A timer the engine set has fired.
    TimerFired(ProxyTimer, Option<BranchId>),
    /// A CANCEL matched our server transaction.
    UpstreamCancelled,
}

/// Something the proxy wants done, in the order it wants it done.
///
/// Deliberately not `PartialEq`: the kernel's `Request` and `Response` are not, because comparing
/// two SIP messages for equality is a question with several defensible answers (bytes? parsed
/// fields? ignoring header order?) and the kernel declines to pick one. Tests compare what they
/// actually mean to compare — [`Effect::kind`] for the shape, and the message's own bytes for the
/// content.
#[derive(Debug)]
pub enum Effect {
    /// Answer the server transaction upstream.
    Respond(Box<Response>),
    /// Put this request on a new client transaction toward `target`.
    Forward {
        /// The branch, which is also the `Via` branch already in the request.
        branch: BranchId,
        /// The request as it should go on the wire — the engine has already done every §16.6 edit.
        request: Box<Request>,
        /// Where to send it.
        target: Target,
        /// The URI whose address this copy must actually reach — F7, computed from the copy after
        /// F6: the first `Route` if it is loose, else the Request-URI.
        ///
        /// **Not the same thing as `target.uri`,** which is what goes *into* the Request-URI (F2).
        /// The two coincide for a bare contact and diverge whenever a `Route` survived preprocessing
        /// or the target carries a `Path`: then the copy is addressed to the far end and sent to the
        /// route set's first hop. A driver that resolved `target.uri` instead would skip that hop —
        /// which is what sent every in-dialog request to a location lookup (`V-03`), because the
        /// only URI it had was the one it was addressed to.
        next_hop: Bytes,
    },
    /// Cancel a branch that has not concluded.
    CancelBranch(BranchId),
    /// Answer the CANCEL's **own** server transaction with `200 OK` (C1).
    ///
    /// A separate effect from [`Effect::Respond`] because it answers a different transaction: the
    /// CANCEL is its own transaction (RFC 3261 §9), and §16.10 makes its `200` immediate and
    /// unconditional — it says "I received your CANCEL", not "the call is cancelled". Conflating
    /// the two would make the acknowledgement wait on the INVITE's outcome.
    AnswerCancel,
    /// Ask for the targets of a URI. The answer returns as `Input::TargetsResolved`.
    ResolveTargets(TargetQuery),
    /// Verify the affinity token a popped platform `Route` carried — P2. The answer returns as
    /// [`Input::TokenFact`].
    ///
    /// **Nothing else is emitted alongside it, and nothing follows until the verdict arrives.**
    /// That is the whole point of it being an effect rather than a courtesy the driver performs on
    /// its own initiative. `PX-13` gave in-dialog requests their predetermined target set (T1) and
    /// resolved it synchronously inside `Input::Upstream`, which put `Effect::Forward` ahead of any
    /// verdict a driver could have supplied — so P3's `403` arrived after the request had already
    /// gone, and a forged token bought exactly the routing P3 exists to deny it. Feeding the verdict
    /// first was no repair either: with no request yet, the engine had nothing to reject. Making the
    /// engine *ask* is what puts the two in the only order that is safe.
    VerifyToken {
        /// The governing token — the first-popped entry's, whose direction names the presenting side.
        token: Bytes,
        /// The pair's other entry when one was popped, for affinity-token §8 S9's consistency check.
        partner: Option<Bytes>,
    },
    /// Arm a timer.
    SetTimer {
        /// Which timer.
        timer: ProxyTimer,
        /// Which branch it guards, if it is per-branch.
        branch: Option<BranchId>,
        /// How long from now.
        after: Duration,
    },
    /// Disarm a timer.
    ClearTimer {
        /// Which timer.
        timer: ProxyTimer,
        /// Which branch.
        branch: Option<BranchId>,
    },
    /// The context is finished; the driver may drop it.
    Terminate,
}

impl Effect {
    /// Build a `Respond` without spelling out the box.
    #[must_use]
    pub fn respond(response: Response) -> Self {
        Self::Respond(Box::new(response))
    }

    /// The effect's shape, for assertions about the *sequence* of effects.
    ///
    /// The order matters as much as the content — `Forward` before the `SetTimer` that guards it —
    /// so a test that asserts on a list of these is asserting something the spec requires.
    #[must_use]
    pub fn kind(&self) -> Kind {
        match self {
            Self::Respond(_) => Kind::Respond,
            Self::AnswerCancel => Kind::AnswerCancel,
            Self::Forward { .. } => Kind::Forward,
            Self::CancelBranch(_) => Kind::CancelBranch,
            Self::ResolveTargets(_) => Kind::ResolveTargets,
            Self::VerifyToken { .. } => Kind::VerifyToken,
            Self::SetTimer { .. } => Kind::SetTimer,
            Self::ClearTimer { .. } => Kind::ClearTimer,
            Self::Terminate => Kind::Terminate,
        }
    }

    /// The status code, if this effect answers upstream.
    #[must_use]
    pub fn status(&self) -> Option<u16> {
        match self {
            Self::Respond(response) => Some(response.status.code()),
            _ => None,
        }
    }

    /// The branch this effect concerns, if any.
    #[must_use]
    pub fn branch(&self) -> Option<&BranchId> {
        match self {
            Self::Forward { branch, .. } | Self::CancelBranch(branch) => Some(branch),
            Self::SetTimer { branch, .. } | Self::ClearTimer { branch, .. } => branch.as_ref(),
            _ => None,
        }
    }

    /// The request this effect forwards, if it forwards one.
    #[must_use]
    pub fn forwarded(&self) -> Option<&Request> {
        match self {
            Self::Forward { request, .. } => Some(request),
            _ => None,
        }
    }

    /// The URI this effect's next hop is, if it forwards anything (F7).
    #[must_use]
    pub fn next_hop(&self) -> Option<&Bytes> {
        match self {
            Self::Forward { next_hop, .. } => Some(next_hop),
            _ => None,
        }
    }
}

/// An [`Effect`]'s shape, with the message left out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// [`Effect::Respond`].
    Respond,
    /// [`Effect::Forward`].
    Forward,
    /// [`Effect::CancelBranch`].
    CancelBranch,
    /// [`Effect::AnswerCancel`].
    AnswerCancel,
    /// [`Effect::ResolveTargets`].
    ResolveTargets,
    /// [`Effect::VerifyToken`].
    VerifyToken,
    /// [`Effect::SetTimer`].
    SetTimer,
    /// [`Effect::ClearTimer`].
    ClearTimer,
    /// [`Effect::Terminate`].
    Terminate,
}
