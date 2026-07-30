//! The response context: one server transaction, N branches — §7 and §8.
//!
//! A state machine, and nothing else: inputs in, ordered effects out. No clock, no socket, no
//! location service. That is what makes every rule in
//! [proxy-behavior](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/proxy-behavior.md)
//! reachable from a test that neither sleeps nor opens a port.
//!
//! **Scope.** `PX-5` brought validation, preprocessing, forwarding and response aggregation;
//! `PX-6` added §9 — CANCEL propagation and Timer C. `PB-S-*` (stateless mode) is `PX-4`, in M2.
//!
//! One §9 rule is deliberately *not* here: C4, a CANCEL matching no transaction, is forwarded
//! statelessly. By definition no response context exists for it, so it never reaches this type at
//! all — it is the driver's, and [proxy-transaction-driver](https://github.com/codewandler/sipx-clstr/blob/main/docs/designs/proxy-transaction-driver.md)
//! says so.

use bytes::Bytes;
use sipx_sip::headers::Address;
use sipx_sip::{HeaderName, Method, Request, Response, ResponseBuilder, StatusCode};

use crate::config::ProxyConfig;
use crate::forward::{ForwardPlan, forward};
use crate::types::{BranchId, Effect, Input, ProxyTimer, Target, TargetQuery, TokenVerdict};
use crate::validate::{Refusal, Validated, validate};

/// How one branch is doing.
#[derive(Debug, Clone, PartialEq, Eq)]
enum BranchState {
    /// Forwarded, nothing final yet.
    Pending {
        /// Whether any 101–199 has arrived. Decides both what Timer C does (C5 vs C6) and whether a
        /// CANCEL can go out at all (C2).
        saw_provisional: bool,
        /// A CANCEL is owed to this branch but cannot be sent yet.
        ///
        /// §9.1: a CANCEL must not be sent for an INVITE that has produced no provisional, because
        /// it can overtake the INVITE at the next hop — cancelling a transaction that has not been
        /// created, which the far end answers `481` while the INVITE it was meant to stop proceeds.
        cancel_queued: bool,
    },
    /// Concluded with this final status.
    Concluded(u16),
    /// Cancelled by us, waiting for its `487`.
    Cancelling,
}

/// One proxied request, from arrival to the response that concludes it.
#[derive(Debug)]
pub struct ResponseContext {
    config: ProxyConfig,
    request: Option<Request>,
    validated: Option<Validated>,
    /// Branches in the order they were forwarded, which is the order targets arrived in.
    branches: Vec<(BranchId, BranchState, Target)>,
    /// Targets not yet tried, for sequential (distinct-`q`) forking.
    queued: Vec<Target>,
    /// Every final response a branch produced, for R7's selection.
    finals: Vec<Response>,
    /// Whether a final response has already gone upstream.
    answered: bool,
    /// Whether the context is over.
    finished: bool,
}

impl ResponseContext {
    /// A context for a proxy with this configuration.
    #[must_use]
    pub fn new(config: ProxyConfig) -> Self {
        Self {
            config,
            request: None,
            validated: None,
            branches: Vec::new(),
            queued: Vec::new(),
            finals: Vec::new(),
            answered: false,
            finished: false,
        }
    }

    /// Feed one input; get the effects to perform, in order.
    pub fn on_input(&mut self, input: Input) -> Vec<Effect> {
        if self.finished {
            return Vec::new();
        }
        match input {
            Input::Upstream(request) => self.on_upstream(*request),
            Input::TargetsResolved(targets) => self.on_targets(targets),
            Input::BranchResponse(response, branch) => self.on_branch_response(*response, &branch),
            Input::BranchTransportError(branch) => self.on_branch_failure(&branch),
            Input::TokenFact(verdict) => self.on_token(&verdict),
            Input::TimerFired(ProxyTimer::C, branch) => self.on_timer_c(branch.as_ref()),
            Input::UpstreamCancelled => self.on_upstream_cancelled(),
        }
    }

    /// Whether the context has concluded.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    // ---------------------------------------------------------------- §4, §5 ------------------

    fn on_upstream(&mut self, request: Request) -> Vec<Effect> {
        // §4 — validation, in order, first failure terminates.
        let validated = match validate(&request, &self.config) {
            Ok(validated) => validated,
            Err(refusal) => return self.refuse(&request, &refusal),
        };

        // §5 — route preprocessing, before target determination.
        let mut request = request;
        let preprocessed = self.preprocess(&mut request);
        if let Some(effects) = preprocessed {
            return effects;
        }

        // §16.5 — target determination. The proxy asks; the driver answers. Which URI is asked
        // about is the first `Route` if there is one (it is loose, or F6 already swapped it), else
        // the Request-URI.
        let query = TargetQuery {
            uri: next_hop_uri(&request),
        };
        self.request = Some(request);
        self.validated = Some(validated);
        vec![Effect::ResolveTargets(query)]
    }

    /// §5's P1–P3. Returns effects when preprocessing itself concludes the request.
    fn preprocess(&mut self, request: &mut Request) -> Option<Vec<Effect>> {
        // P1 — a strict-routing predecessor put our Record-Route value in the Request-URI. The real
        // Request-URI is the last Route; recover it and drop that Route.
        if self.config.is_ours(&request.uri) {
            let routes: Vec<Bytes> = request
                .headers
                .get_all(&HeaderName::Route)
                .map(|header| Bytes::copy_from_slice(header.value().as_ref()))
                .collect();
            if let Some(last) = routes.last()
                && let Ok(address) = Address::parse(last, "Route")
            {
                request.uri = address.uri.clone();
                request.headers.remove_all(&HeaderName::Route);
                for route in routes.iter().take(routes.len().saturating_sub(1)) {
                    if let Ok(header) = sipx_sip::Header::build(HeaderName::Route, route.clone()) {
                        request.headers.push(header);
                    }
                }
            }
        }

        // P2 — the first Route resolves to this platform: pop it. Any edge pops any edge's Route,
        // which is the point of the token.
        let routes: Vec<Bytes> = request
            .headers
            .get_all(&HeaderName::Route)
            .map(|header| Bytes::copy_from_slice(header.value().as_ref()))
            .collect();
        if let Some(first) = routes.first()
            && let Ok(address) = Address::parse(first, "Route")
            && self.config.is_ours(&address.uri)
        {
            request.headers.remove_all(&HeaderName::Route);
            for route in routes.iter().skip(1) {
                if let Ok(header) = sipx_sip::Header::build(HeaderName::Route, route.clone()) {
                    request.headers.push(header);
                }
            }
        }

        // P3's rejection arrives as `Input::TokenFact(Invalid)`, because verification is the
        // driver's (it holds the keys). Nothing to do here.
        None
    }

    /// P3 — token verification failed: `403`, no forward, no fallback.
    fn on_token(&mut self, verdict: &TokenVerdict) -> Vec<Effect> {
        match verdict {
            TokenVerdict::Valid { .. } => Vec::new(),
            TokenVerdict::Invalid => {
                let Some(request) = self.request.clone() else {
                    return self.terminate();
                };
                // A hard reject. Falling back to ordinary routing would mean a forged token buys
                // exactly the routing it would have got with no token at all.
                self.respond_and_finish(&request, 403, "Forbidden")
            }
        }
    }

    // ---------------------------------------------------------------- §7 ----------------------

    fn on_targets(&mut self, targets: Vec<Target>) -> Vec<Effect> {
        let Some(request) = self.request.clone() else {
            return self.terminate();
        };

        // §7 — an empty target set concludes with `480`. The location service returns empty sets
        // rather than deciding responses, so the decision is here.
        if targets.is_empty() {
            return self.respond_and_finish(&request, 480, "Temporarily Unavailable");
        }

        // §7 L4/§16.6: equal `q` forks in parallel, distinct `q` in sequence. Sort descending so
        // the most preferred group goes first, then take the leading group.
        let mut targets = targets;
        // Descending, so the most preferred group forks first (§7 L2).
        targets.sort_by_key(|target| std::cmp::Reverse(target.q));
        self.queued = targets;
        self.fork_next_group()
    }

    /// Forward the next `q` group in parallel.
    fn fork_next_group(&mut self) -> Vec<Effect> {
        let Some(request) = self.request.clone() else {
            return self.terminate();
        };
        let Some(validated) = self.validated.clone() else {
            return self.terminate();
        };
        if self.queued.is_empty() {
            return Vec::new();
        }

        let leading_q = self.queued.first().map_or(0, |target| target.q);
        let group: Vec<Target> = {
            let mut split = self
                .queued
                .iter()
                .position(|target| target.q != leading_q)
                .unwrap_or(self.queued.len());

            // RFC 5393 §5.2 / PB-V-8 — `Max-Breadth` caps how many branches may go out *in
            // parallel*. A group wider than the budget is not truncated: the surplus stays queued
            // and is forked when this group concludes, so the targets are serialized rather than
            // dropped. Truncating would silently lose a device the user registered.
            let budget = usize::try_from(validated.max_breadth)
                .unwrap_or(usize::MAX)
                .max(1);
            split = split.min(budget);

            self.queued.drain(..split).collect()
        };

        let parallel = u32::try_from(group.len()).unwrap_or(u32::MAX);
        // V5 again, now that the branch count is known: a branch that cannot receive at least 1 of
        // `Max-Breadth` is not forwarded. §6 divides rather than refuses, so this only bites when
        // the incoming value was already 0 — which V5 caught — but the arithmetic is stated here so
        // a future policy that refuses instead has one place to change.
        let record_route = is_dialog_forming(&request);
        let mut effects = Vec::new();
        let already = self.branches.len();

        for (offset, target) in group.into_iter().enumerate() {
            let plan = ForwardPlan {
                original: &request,
                validated: &validated,
                target: &target,
                index: already + offset,
                parallel_branches: parallel,
                record_route,
                token: None, // AF-4 mints the real one; F4's shape and budget are already here.
            };
            // A target we cannot build a request for is a target that failed, not a request that
            // dies: the remaining branches still deserve their chance.
            let Ok((branch, forwarded)) = forward(&plan, &self.config) else {
                continue;
            };
            effects.push(Effect::Forward {
                branch: branch.clone(),
                request: Box::new(forwarded),
                target: target.clone(),
            });
            // F11 — Timer C guards INVITE branches. Emitted *after* the Forward, because a timer
            // that started before its request went out would measure the wrong interval.
            if request.method == Method::Invite {
                effects.push(Effect::SetTimer {
                    timer: ProxyTimer::C,
                    branch: Some(branch.clone()),
                    after: self.config.effective_timer_c(),
                });
            }
            self.branches.push((
                branch,
                BranchState::Pending {
                    saw_provisional: false,
                    cancel_queued: false,
                },
                target,
            ));
        }

        if self.branches.is_empty() && self.queued.is_empty() {
            // Every target failed to produce a forwardable copy, so there is nothing in flight and
            // nothing left to try: §7's terminal case again.
            return self.respond_and_finish(&request, 480, "Temporarily Unavailable");
        }
        effects
    }

    // ---------------------------------------------------------------- §8 ----------------------

    fn on_branch_response(&mut self, response: Response, branch: &BranchId) -> Vec<Effect> {
        let Some(request) = self.request.clone() else {
            return self.terminate();
        };
        // R1 — a response for a branch we do not know is not ours to forward.
        if !self.branches.iter().any(|(id, _, _)| id == branch) {
            return Vec::new();
        }
        let status = response.status.code();

        // R3 — a `100` is absorbed. Forwarding it would put a hop-by-hop provisional end to end.
        if status == 100 {
            return Vec::new();
        }

        // R4 — any other 1xx goes upstream immediately, and resets that branch's Timer C.
        if response.status.is_provisional() {
            let cancel_now_sendable = self.mark_provisional(branch);
            let mut effects = vec![Effect::respond(pop_via(response))];
            if request.method == Method::Invite {
                effects.push(Effect::SetTimer {
                    timer: ProxyTimer::C,
                    branch: Some(branch.clone()),
                    after: self.config.effective_timer_c(),
                });
            }
            // C2's deferred half: the CANCEL we could not send now can go.
            if cancel_now_sendable {
                self.set_cancelling(branch);
                effects.push(Effect::CancelBranch(branch.clone()));
            }
            return effects;
        }

        // R10/R8 — a `503` from a branch is a branch failure, and must never be forwarded as `503`.
        if status == 503 {
            return self.on_branch_failure(branch);
        }

        self.conclude_branch(branch, status);

        // R5 — a 2xx to an INVITE is forwarded immediately and **always**, including after a final
        // was already chosen (RFC 6026: two 200s for one INVITE is a fork, not a bug). Then the
        // remaining branches are cancelled.
        if status / 100 == 2 && request.method == Method::Invite {
            let mut effects = vec![Effect::respond(pop_via(response))];
            self.answered = true;
            effects.extend(self.cancel_pending());
            return effects;
        }

        // R6 — a 6xx is forwarded immediately and cancels everything.
        if status / 100 == 6 {
            self.finals.push(clone_response(&response));
            let mut effects = vec![Effect::respond(pop_via(response))];
            self.answered = true;
            effects.extend(self.cancel_pending());
            effects.extend(self.finish_if_settled());
            return effects;
        }

        self.finals.push(response);

        // R7 — selection waits until every branch has concluded.
        if self.has_pending() {
            return Vec::new();
        }
        // A distinct-`q` group remains: sequential forking continues rather than answering.
        if !self.queued.is_empty() {
            return self.fork_next_group();
        }
        self.select_and_answer(&request)
    }

    // ---------------------------------------------------------------- §9 ----------------------

    /// C1–C3 — a CANCEL matched our server transaction.
    fn on_upstream_cancelled(&mut self) -> Vec<Effect> {
        // C1 — the CANCEL's own `200` is immediate and unconditional, and goes first. It
        // acknowledges the CANCEL, not the cancellation: §16.10 does not let it wait on whether any
        // branch can actually be stopped.
        let mut effects = vec![Effect::AnswerCancel];

        // §9.2 — a CANCEL for a transaction that has already been answered is a no-op beyond that
        // `200`. Nothing to stop, and re-answering upstream would contradict what we already sent.
        let pending: Vec<(BranchId, bool)> = self
            .branches
            .iter()
            .filter_map(|(id, state, _)| match state {
                BranchState::Pending {
                    saw_provisional, ..
                } => Some((id.clone(), *saw_provisional)),
                _ => None,
            })
            .collect();

        for (branch, saw_provisional) in pending {
            if saw_provisional {
                // C2 — a branch that has answered at least once can be cancelled now.
                self.set_cancelling(&branch);
                effects.push(Effect::CancelBranch(branch));
            } else {
                // C2 — queued until its first provisional. Sending now risks the CANCEL overtaking
                // the INVITE at the next hop.
                self.queue_cancel(&branch);
            }
        }

        effects
    }

    /// C5/C6 — Timer C fired for a branch.
    fn on_timer_c(&mut self, branch: Option<&BranchId>) -> Vec<Effect> {
        let Some(branch) = branch else {
            // Timer C is per-branch by definition (§16.8). A firing with no branch is a driver bug,
            // and acting on it would mean guessing which branch was meant.
            return Vec::new();
        };
        let saw_provisional = self.branches.iter().find_map(|(id, state, _)| match state {
            BranchState::Pending {
                saw_provisional, ..
            } if id == branch => Some(*saw_provisional),
            _ => None,
        });

        match saw_provisional {
            // C5 — provisionals were arriving and then stopped: the far end is alive but stuck, so
            // cancel it and let C3 produce the `487`.
            Some(true) => {
                self.set_cancelling(branch);
                vec![Effect::CancelBranch(branch.clone())]
            }
            // C6 — nothing was ever heard. There is no transaction at the far end worth cancelling,
            // so the branch concludes as a timeout (R9) and selection proceeds.
            Some(false) => self.conclude_as_timeout(branch),
            // Already concluded or cancelling: a late Timer C is stale, not an event.
            None => Vec::new(),
        }
    }

    /// R9 — conclude a branch as `408`, then continue selection.
    fn conclude_as_timeout(&mut self, branch: &BranchId) -> Vec<Effect> {
        let Some(request) = self.request.clone() else {
            return self.terminate();
        };
        self.conclude_branch(branch, 408);
        if let Ok(response) = build_response(&request, 408, "Request Timeout") {
            self.finals.push(response);
        }
        if self.has_pending() {
            return Vec::new();
        }
        if !self.queued.is_empty() {
            return self.fork_next_group();
        }
        self.select_and_answer(&request)
    }

    fn set_cancelling(&mut self, branch: &BranchId) {
        for (id, state, _) in &mut self.branches {
            if id == branch {
                *state = BranchState::Cancelling;
            }
        }
    }

    fn queue_cancel(&mut self, branch: &BranchId) {
        for (id, state, _) in &mut self.branches {
            if id == branch
                && let BranchState::Pending { cancel_queued, .. } = state
            {
                *cancel_queued = true;
            }
        }
    }

    /// R9/R10 — a branch that timed out or whose transport failed.
    fn on_branch_failure(&mut self, branch: &BranchId) -> Vec<Effect> {
        let Some(request) = self.request.clone() else {
            return self.terminate();
        };
        if !self.branches.iter().any(|(id, _, _)| id == branch) {
            return Vec::new();
        }

        // §16.9: a transport error behaves as `503` from that branch — and R8 then keeps that `503`
        // from reaching the caller, because a hop's inability to deliver is not the destination
        // being overloaded.
        self.conclude_branch(branch, 503);
        if let Ok(response) = build_response(&request, 503, "Service Unavailable") {
            self.finals.push(response);
        }

        if self.has_pending() {
            return Vec::new();
        }
        if !self.queued.is_empty() {
            return self.fork_next_group();
        }
        self.select_and_answer(&request)
    }

    /// R7 + R8: pick the best final and send it.
    fn select_and_answer(&mut self, request: &Request) -> Vec<Effect> {
        if self.answered {
            return self.finish_if_settled();
        }
        let Some(best) = self.best_final() else {
            // Every branch failed to produce anything answerable.
            return self.respond_and_finish(request, 500, "Server Internal Error");
        };

        // R8 — a `503` that ends up the best response becomes `500`. §16.7's rule: the caller must
        // not be told the destination is unavailable when what happened is that we could not get
        // there.
        if best == 503 {
            return self.respond_and_finish(request, 500, "Server Internal Error");
        }

        // R7's 401/407 aggregation: one response carrying every challenge from every challenging
        // branch, because a UAC that only saw one realm's challenge cannot satisfy the others.
        let aggregated = if best == 401 || best == 407 {
            self.aggregate_challenges(request, best)
        } else {
            self.finals
                .iter()
                .find(|response| response.status.code() == best)
                .map(clone_response)
        };

        match aggregated {
            Some(response) => {
                self.answered = true;
                let mut effects = vec![Effect::respond(pop_via(response))];
                effects.extend(self.cancel_pending());
                effects.extend(self.finish_if_settled());
                effects
            }
            None => self.respond_and_finish(request, 500, "Server Internal Error"),
        }
    }

    /// R7's order: the lowest class among received finals, then §8.1's within-class rank, then the
    /// lowest code as a tie-break — which makes the choice total and independent of arrival order.
    ///
    /// 6xx is already forwarded on arrival by R6, so it cannot reach here — but it is included in
    /// the ordering anyway, because a rule that only works because of what happens elsewhere is a
    /// rule waiting to break when the elsewhere changes.
    fn best_final(&self) -> Option<u16> {
        self.finals
            .iter()
            .map(|response| response.status.code())
            .min_by_key(|status| (status / 100, within_class_rank(*status), *status))
    }

    fn aggregate_challenges(&self, request: &Request, status: u16) -> Option<Response> {
        let header = if status == 401 {
            HeaderName::WwwAuthenticate
        } else {
            HeaderName::ProxyAuthenticate
        };
        let mut response = build_response(
            request,
            status,
            if status == 401 {
                "Unauthorized"
            } else {
                "Proxy Authentication Required"
            },
        )
        .ok()?;

        for source in self
            .finals
            .iter()
            .filter(|candidate| candidate.status.code() == status)
        {
            for challenge in source.headers.get_all(&header) {
                response.headers.push(challenge.clone());
            }
        }
        Some(response)
    }

    // ---------------------------------------------------------------- helpers -----------------

    fn refuse(&mut self, request: &Request, refusal: &Refusal) -> Vec<Effect> {
        self.finished = true;
        let Some(status) = refusal.status() else {
            // V1 — unanswerable, so nothing is sent. A response built from a message we could not
            // parse would carry fields we invented.
            return vec![Effect::Terminate];
        };
        let Ok(mut response) = build_response(request, status, refusal.reason()) else {
            return vec![Effect::Terminate];
        };
        // V6 must name the offenders, or the UAC cannot act on the refusal.
        if let Refusal::BadExtension(tags) = refusal
            && let Ok(header) = sipx_sip::Header::build(HeaderName::Unsupported, tags.join(", "))
        {
            response.headers.push(header);
        }
        vec![Effect::respond(response), Effect::Terminate]
    }

    fn respond_and_finish(&mut self, request: &Request, status: u16, reason: &str) -> Vec<Effect> {
        self.answered = true;
        self.finished = true;
        match build_response(request, status, reason) {
            Ok(response) => {
                let mut effects = vec![Effect::respond(response)];
                effects.extend(
                    self.pending_branch_ids()
                        .into_iter()
                        .map(Effect::CancelBranch),
                );
                effects.push(Effect::Terminate);
                effects
            }
            Err(()) => vec![Effect::Terminate],
        }
    }

    fn terminate(&mut self) -> Vec<Effect> {
        self.finished = true;
        vec![Effect::Terminate]
    }

    /// R11 — forwarding the chosen final cancels every still-pending branch.
    fn cancel_pending(&mut self) -> Vec<Effect> {
        let ids = self.pending_branch_ids();
        for (id, state, _) in &mut self.branches {
            if ids.contains(id) {
                *state = BranchState::Cancelling;
            }
        }
        ids.into_iter().map(Effect::CancelBranch).collect()
    }

    fn finish_if_settled(&mut self) -> Vec<Effect> {
        if self.has_pending() || !self.queued.is_empty() {
            return Vec::new();
        }
        self.finished = true;
        vec![Effect::Terminate]
    }

    fn pending_branch_ids(&self) -> Vec<BranchId> {
        self.branches
            .iter()
            .filter(|(_, state, _)| matches!(state, BranchState::Pending { .. }))
            .map(|(id, _, _)| id.clone())
            .collect()
    }

    fn has_pending(&self) -> bool {
        self.branches.iter().any(|(_, state, _)| {
            matches!(state, BranchState::Pending { .. } | BranchState::Cancelling)
        })
    }

    /// Record that a branch produced a provisional. Returns whether a queued CANCEL is now
    /// sendable, which is C2's deferred half.
    fn mark_provisional(&mut self, branch: &BranchId) -> bool {
        let mut release = false;
        for (id, state, _) in &mut self.branches {
            if id == branch
                && let BranchState::Pending {
                    saw_provisional,
                    cancel_queued,
                } = state
            {
                release = *cancel_queued;
                *saw_provisional = true;
                *cancel_queued = false;
            }
        }
        release
    }

    fn conclude_branch(&mut self, branch: &BranchId, status: u16) {
        for (id, state, _) in &mut self.branches {
            if id == branch {
                *state = BranchState::Concluded(status);
            }
        }
    }
}

/// R2 — pop the topmost `Via`, which is ours.
///
/// `Headers::remove_first` since sipx `S-15` (v0.4.0). It used to rebuild the whole collection,
/// because `Headers` could remove *every* occurrence of a name but not the first one — correct,
/// and one clone per header per hop. The upstream operation removes at an exact position, which is
/// what §16.7 step 2 actually asks for: `Via` order *is* the return path, so everything below the
/// one we take must keep its place.
fn pop_via(response: Response) -> Response {
    let mut response = response;
    response.headers.remove_first(&HeaderName::Via);
    response
}

fn clone_response(response: &Response) -> Response {
    response.clone()
}

/// §8.1's within-class rank — lower is preferred, and the lowest code breaks a tie inside a rank.
///
/// §16.7 step 6 fixes the class and leaves the response inside it to us (`MAY select any response
/// within that chosen class`), with one steer: prefer what the caller can act on. Bare numeric
/// order is not that steer — it lets `408`, a branch that never answered, outrank `486`, a branch
/// that did. Every ranked code is a 4xx, the class the RFC scopes its preference to, so any other
/// class falls through to the tie-break and is chosen by code alone.
fn within_class_rank(status: u16) -> u8 {
    match status {
        // §16.7 step 6's own SHOULD, verbatim: the caller can change these and retry successfully.
        401 | 407 | 415 | 420 | 484 => 1,
        // A branch reached the user and the user's side answered: present, not available now.
        480 | 486 => 2,
        // Absence, silence, our own cancellation, and rejections of the message rather than the user.
        _ => 3,
    }
}

fn build_response(request: &Request, status: u16, reason: &str) -> Result<Response, ()> {
    let status = StatusCode::new(status).ok_or(())?;
    Ok(
        ResponseBuilder::to_request(request, status, reason.to_owned())
            .map_err(|_| ())?
            .build(),
    )
}

/// Whether this request would establish a dialog, and therefore needs a `Record-Route`.
fn is_dialog_forming(request: &Request) -> bool {
    // INVITE without a `To` tag. A re-INVITE carries one and must not add a Record-Route: RFC 6141
    // confirms mid-dialog Record-Route does not alter an established route set, so adding one is
    // pure noise that also costs a token's worth of bytes.
    if request.method != Method::Invite {
        return false;
    }
    request
        .headers
        .value(&HeaderName::To)
        .and_then(|value| Address::parse(&value, "To").ok())
        .is_none_or(|address| address.tag().is_none())
}

/// The URI whose targets should be resolved: the first `Route` if one survives preprocessing, else
/// the Request-URI (F7).
fn next_hop_uri(request: &Request) -> Bytes {
    request
        .headers
        .value(&HeaderName::Route)
        .and_then(|value| Address::parse(&value, "Route").ok())
        .map_or_else(|| request.uri.to_bytes(), |address| address.uri.to_bytes())
}

/// Timer C's default, exposed so a driver and a test agree on it without repeating the number.
///
/// Re-exported rather than restated: it lives beside the floor it has to satisfy
/// ([`crate::config::TIMER_C_FLOOR`]), because two independently-written copies of this number are
/// exactly how one of them came to sit on the floor (`PX-10`).
pub use crate::config::DEFAULT_TIMER_C;
