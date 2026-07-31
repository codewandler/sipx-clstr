//! M2's defining criterion, executed: a dialog-forming request through **edge A** yields a
//! `Record-Route` token, and the mid-dialog request arriving at **edge B** routes on it with the
//! cross-node dialog-lookup counter at zero.
//!
//! `AF-5`. This is the scenario the roadmap's M2 exit condition names — "mid-dialog requests route
//! by token with **zero** cross-node dialog lookups, asserted by metric" — and the assertion is a
//! trace query, not a component's own word for it.
//!
//! **What makes it a proof rather than a demonstration.** Edge B has never seen this dialog and
//! holds nothing that could tell it anything about one: its target map is empty, it runs no
//! registrar, and the two edges share no state of any kind. The only thing it has that edge A also
//! has is the **key set** — configuration, not dialog state — so everything it needs in order to
//! forward the `BYE` correctly arrived inside the `BYE`. That is AGENTS.md non-negotiable #5, and
//! the counter reading zero is what it looks like from outside.
//!
//! The two edges advertise one **cluster-wide** `Record-Route` host (affinity-token §5: "The URI
//! host is a cluster-wide service identity — any edge recognizes and pops it — never an individual
//! node name"), which is what lets edge B recognize edge A's `Route` as the platform's at all.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;

use bytes::Bytes;
use sipx_clstr_affinity::{
    Algorithm, Claims, Direction, Expect, KeyEntry, KeySet, MintKey, NonceSource, TOKEN_PARAM,
    TOKEN_PARAM_BUDGET, Verdict, mint_with, verify,
};
use sipx_clstr_proxy::{
    AckRoute, BranchId, CookieKey, Effect as ProxyEffect, Input as ProxyInput, ProxyConfig,
    RecordRouteTokens, ResponseContext, Target, TokenVerdict, route_ack,
};
use sipx_clstr_sim::node::{Effect, Input, SimNode, TimerId, send};
use sipx_clstr_sim::rng::SimRng;
use sipx_clstr_sim::{LinkKind, LinkPolicy, NodeId, Sim, SimTime, viz};
use sipx_sip::headers::Address;
use sipx_sip::{
    Header, HeaderName, Message, Method, Request, RequestBuilder, Response, ResponseBuilder,
    StatusCode, Uri,
};

/// The cluster's service identity — one host for every edge (affinity-token §5).
const CLUSTER: &str = "cluster.example";
const RECORD_ROUTE: &[u8] = b"<sip:cluster.example;lr>";
const BOB_CONTACT: &str = "sip:bob@10.0.0.2:5060";
const CALL: &str = "af5-the-call";

/// The wall-clock second the simulation's zero stands for.
///
/// A token's `expiry` is absolute UNIX seconds (affinity-token §3) while `SimTime` is nanoseconds
/// since the scenario began, so the two are bridged by a constant rather than by a clock. `T0` is
/// §10's own fixture time, so the numbers in a failing trace line up with the spec's.
const T0: u32 = 1_785_240_000;

/// The token lifetime `L` (§7 M5). Well past this scenario, which runs in milliseconds.
const LIFETIME: u32 = 86_400;

fn uri(text: &str) -> Uri {
    Uri::parse(Bytes::copy_from_slice(text.as_bytes())).expect("a valid URI")
}

/// The cluster key set: one `chacha20-poly1305` mint key, held by **both** edges.
///
/// Configuration, deliberately — affinity-token §6 is config-first, there is no key exchange, and
/// this is the whole of what the two edges share. If this were dialog state the scenario would
/// prove nothing.
fn cluster_keys() -> (MintKey, KeySet) {
    let key = MintKey::new(0x01, Algorithm::ChaCha20Poly1305, [0x11; 32]);
    let set = KeySet::new(vec![KeyEntry::new(key.clone(), 0, u32::MAX, true)])
        .expect("a one-key set is well formed");
    (key, set)
}

/// The harness's replayable randomness seam (affinity-token §7 M4, AGENTS.md rule 2).
///
/// The affinity crate implements [`NonceSource`] for nothing on purpose and `getrandom` is kept out
/// of its dependency graph, so this is where a nonce comes from here — a named stream off the
/// scenario's seed, which is what makes a failing run a value you paste back rather than a flake.
#[derive(Debug)]
struct SeededNonces(SimRng);

impl NonceSource for SeededNonces {
    fn next_nonce(&mut self) -> [u8; 12] {
        let mut nonce = [0u8; 12];
        rand::RngCore::fill_bytes(&mut self.0, &mut nonce);
        nonce
    }
}

// ------------------------------------------------------------------------------- the edge -------

/// One edge of the cluster: the real forwarding core, plus the driver half `AF-5` is about.
#[derive(Debug)]
struct Edge {
    name: String,
    /// Contexts by Call-ID.
    contexts: HashMap<String, ResponseContext>,
    /// Where a next-hop URI actually is — the sim's stand-in for a socket address.
    reachable: HashMap<String, NodeId>,
    /// Who to answer, by Call-ID.
    upstream: HashMap<String, NodeId>,
    branch_nodes: HashMap<BranchId, NodeId>,
    branch_call: HashMap<BranchId, String>,
    /// What this edge can resolve. **Empty on edge B**, which is the point of the scenario.
    targets: Vec<Target>,
    /// Every lookup this edge performed. Edge B's must stay empty.
    lookups: Vec<String>,
    /// Call-IDs whose current request arrived **inside** a dialog — the `To` tag test (§5.1 T1).
    ///
    /// It exists so that the cross-node dialog-lookup note below has a real condition to fire on
    /// rather than being a string nobody could ever emit.
    in_dialog: std::collections::HashSet<String>,
    /// Verified tokens, newest last — what edge B learned with no lookup anywhere.
    verified: Vec<(Direction, u32)>,
    keys: KeySet,
    mint_key: Option<MintKey>,
    nonces: SeededNonces,
    /// Absolute seconds, captured from the scheduler on every input.
    now: u32,
    timers: Vec<BranchId>,
}

impl Edge {
    fn new(name: &str, seed: u64, mints: bool, targets: Vec<Target>) -> Self {
        let (mint_key, keys) = cluster_keys();
        Self {
            name: name.to_owned(),
            contexts: HashMap::new(),
            reachable: HashMap::new(),
            upstream: HashMap::new(),
            branch_nodes: HashMap::new(),
            branch_call: HashMap::new(),
            targets,
            lookups: Vec::new(),
            in_dialog: std::collections::HashSet::new(),
            verified: Vec::new(),
            keys,
            // Edge B mints nothing. It has the key set — it must, or it could not verify — and no
            // dialog ever starts there in this scenario.
            mint_key: mints.then_some(mint_key),
            nonces: SeededNonces(SimRng::stream(seed, &format!("node:{name}:nonce"))),
            now: T0,
            timers: Vec::new(),
        }
    }

    /// Both edges answer to the same cluster-wide identity (§5).
    fn config() -> ProxyConfig {
        ProxyConfig::new(
            CLUSTER,
            Bytes::from_static(RECORD_ROUTE),
            CookieKey::new(Bytes::from_static(b"cluster-cookie-key")),
        )
    }

    /// F4's mint draft: the pair, one fresh nonce each (§7 M1–M4).
    ///
    /// The claims are identical apart from direction and nonce — M3, and what §8 S9 checks at the
    /// far end. The ORIG entry faces the side that sent the dialog-forming request; the TERM entry
    /// faces the side it is forwarded to.
    fn mint_pair(&mut self) -> Option<RecordRouteTokens> {
        let key = self.mint_key.clone()?;
        let claims = |direction| Claims {
            tenant: 7,
            home_shard: 3,
            edge: 5,
            direction,
            media_node: 0,
            policy_version: 41,
            expiry: self.now.saturating_add(LIFETIME),
            module_facts: Bytes::new(),
        };
        let originating =
            mint_with(&claims(Direction::Originating), &key, &mut self.nonces).ok()?;
        let terminating =
            mint_with(&claims(Direction::Terminating), &key, &mut self.nonces).ok()?;
        Some(RecordRouteTokens {
            originating: Bytes::from(originating.to_param_value()),
            terminating: Bytes::from(terminating.to_param_value()),
        })
    }

    /// P2's answer: decode, verify, and report the verdict as an input (§8).
    ///
    /// Everything this needs is data the driver already holds — the key set from configuration and
    /// the clock from the scheduler. **Nothing is looked up**, which is what makes the counter's
    /// reading structural rather than lucky.
    fn verdict_for(&mut self, token: &Bytes, partner: Option<&Bytes>) -> (TokenVerdict, String) {
        let Ok(bytes) = decode(token) else {
            // §5: padding and out-of-alphabet bytes are rejected before `verify` sees anything.
            return (TokenVerdict::Invalid, "token rejected at decode".to_owned());
        };
        let decoded_partner = partner.and_then(|value| decode(value).ok());
        let mut expect = Expect::new();
        if let Some(partner) = decoded_partner.as_deref() {
            // S9's pair check — the two entries of one Record-Route pair, popped together.
            expect = expect.with_partner(partner);
        }
        match verify(&bytes, &self.keys, self.now, &expect) {
            Verdict::Valid(claims) => {
                self.verified.push((claims.direction, claims.tenant));
                (
                    TokenVerdict::Valid {
                        tenant: claims.tenant.to_string(),
                    },
                    format!(
                        "token verified: {:?}, tenant {}, no lookup",
                        claims.direction, claims.tenant
                    ),
                )
            }
            // The reason is telemetry and never the wire's (§8): one `403` for every failure.
            Verdict::Invalid(reason) => (
                TokenVerdict::Invalid,
                format!("token rejected at {}", reason.step()),
            ),
        }
    }

    fn on_request(&mut self, from: NodeId, request: &Request) -> Vec<Effect> {
        if request.method == Method::Ack {
            return self.on_ack(request);
        }
        let call = call_id(request);
        if in_dialog(request) {
            self.in_dialog.insert(call.clone());
        } else {
            self.in_dialog.remove(&call);
        }
        let mut context = ResponseContext::new(Self::config());
        let mut effects = Vec::new();

        // §7 M1: tokens are minted where F4 record-routes — dialog-forming requests only. A
        // target refresh is an ordinary mid-dialog request and re-mints nothing (M6).
        if is_dialog_forming(request)
            && let Some(tokens) = self.mint_pair()
        {
            effects.extend(context.on_input(ProxyInput::TokensMinted(Box::new(tokens))));
        }
        effects.extend(context.on_input(ProxyInput::Upstream(Box::new(request.clone()))));

        self.upstream.insert(call.clone(), from);
        self.contexts.insert(call.clone(), context);
        self.perform(&call, effects)
    }

    /// K3 — the `ACK` for a 2xx, which takes §5 like any other request and is never answered.
    fn on_ack(&mut self, request: &Request) -> Vec<Effect> {
        let config = Self::config();
        // Two calls: the first asks what to verify, the second supplies the verdict. A tokened ACK
        // cannot reach `Forward` without one, which is P2 before P3 for a method that has no `403`.
        let (verdict, mut out) = match route_ack(request.clone(), &config, None) {
            AckRoute::Verify { token, partner } => {
                let (verdict, note) = self.verdict_for(&token, partner.as_ref());
                (Some(verdict), vec![Effect::Note(format!("ACK {note}"))])
            }
            _ => (None, Vec::new()),
        };
        match route_ack(request.clone(), &config, verdict.as_ref()) {
            AckRoute::Forward { request, next_hop } => {
                let key = String::from_utf8_lossy(&next_hop).into_owned();
                match self.reachable.get(&key).copied() {
                    Some(node) => {
                        out.push(Effect::Note(format!("ACK forwarded to {key}")));
                        out.push(send(node, Message::Request(*request)));
                    }
                    None => out.push(Effect::Note(format!("ACK unreachable {key}"))),
                }
            }
            // Never a response: an ACK has none (§7.2 K3), so a refusal is a record.
            AckRoute::Unroutable(refusal) => {
                out.push(Effect::Note(format!("ACK dropped: {}", refusal.describe())));
            }
            AckRoute::Verify { .. } => out.push(Effect::Note("ACK verdict ignored".to_owned())),
        }
        out
    }

    fn on_response(&mut self, response: &Response) -> Vec<Effect> {
        let Some(branch) = top_via_branch(response).map(BranchId) else {
            return Vec::new();
        };
        let Some(call) = self.branch_call.get(&branch).cloned() else {
            return Vec::new();
        };
        let Some(mut context) = self.contexts.remove(&call) else {
            return Vec::new();
        };
        let effects = context.on_input(ProxyInput::BranchResponse(
            Box::new(response.clone()),
            branch,
        ));
        if !context.is_finished() {
            self.contexts.insert(call.clone(), context);
        }
        self.perform(&call, effects)
    }

    /// The driver half: proxy effects in, sim effects out, in order.
    fn perform(&mut self, call: &str, effects: Vec<ProxyEffect>) -> Vec<Effect> {
        let mut out = Vec::new();
        for effect in effects {
            match effect {
                // P2. The verdict is computed from the key set and the clock and re-entered as an
                // input — the engine never sees a key, and never forwards before this returns.
                ProxyEffect::VerifyToken { token, partner } => {
                    let (verdict, note) = self.verdict_for(&token, partner.as_ref());
                    out.push(Effect::Note(note));
                    let Some(mut context) = self.contexts.remove(call) else {
                        continue;
                    };
                    let more = context.on_input(ProxyInput::TokenFact(verdict));
                    if !context.is_finished() {
                        self.contexts.insert(call.to_owned(), context);
                    }
                    out.extend(self.perform(call, more));
                }
                ProxyEffect::ResolveTargets(query) => {
                    let wanted = String::from_utf8_lossy(&query.uri).into_owned();
                    self.lookups.push(wanted.clone());
                    // **The metric.** A lookup is only a *cross-node dialog* lookup when the
                    // request is inside a dialog this node did not start — which is precisely the
                    // shape T1 exists to make impossible, and precisely what `V-03` used to do to
                    // every ACK and every BYE. Naming it here is what gives the M2 counter a
                    // source: if the token path ever regressed, this note is what would fire.
                    out.push(Effect::Note(if self.in_dialog.contains(call) {
                        format!("cross-node dialog lookup for {wanted}")
                    } else {
                        format!("looked up {wanted}")
                    }));
                    let targets = self.targets.clone();
                    let Some(mut context) = self.contexts.remove(call) else {
                        continue;
                    };
                    let more = context.on_input(ProxyInput::TargetsResolved(targets));
                    if !context.is_finished() {
                        self.contexts.insert(call.to_owned(), context);
                    }
                    out.extend(self.perform(call, more));
                }
                ProxyEffect::Forward {
                    branch,
                    request,
                    next_hop,
                    ..
                } => {
                    // F7's hop, the way the real driver reads it.
                    let key = String::from_utf8_lossy(&next_hop).into_owned();
                    let Some(node) = self.reachable.get(&key).copied() else {
                        out.push(Effect::Note(format!("unreachable {key}")));
                        continue;
                    };
                    self.branch_nodes.insert(branch.clone(), node);
                    self.branch_call.insert(branch.clone(), call.to_owned());
                    out.push(Effect::Note(format!("forward to {key}")));
                    out.push(send(node, Message::Request(*request)));
                }
                ProxyEffect::Respond(response) => {
                    if let Some(&upstream) = self.upstream.get(call) {
                        out.push(Effect::Note(format!("respond {}", response.status.code())));
                        out.push(send(upstream, Message::Response(*response)));
                    }
                }
                ProxyEffect::SetTimer { branch, after, .. } => {
                    if let Some(branch) = branch {
                        let timer = self.timer_id(&branch);
                        out.push(Effect::SetTimer { timer, after });
                    }
                }
                ProxyEffect::ClearTimer { branch, .. } => {
                    if let Some(branch) = branch {
                        let timer = self.timer_id(&branch);
                        out.push(Effect::ClearTimer(timer));
                    }
                }
                ProxyEffect::Terminate => out.push(Effect::Note("terminate".to_owned())),
                ProxyEffect::CancelBranch(_) | ProxyEffect::AnswerCancel => {}
            }
        }
        out
    }

    fn timer_id(&mut self, branch: &BranchId) -> TimerId {
        if let Some(index) = self.timers.iter().position(|known| known == branch) {
            return TimerId(u64::try_from(index).unwrap_or(u64::MAX));
        }
        self.timers.push(branch.clone());
        TimerId(u64::try_from(self.timers.len() - 1).unwrap_or(u64::MAX))
    }
}

impl SimNode for Edge {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_input(&mut self, now: SimTime, input: Input<'_>) -> Vec<Effect> {
        // The clock reaches the driver and stops there: `verify` takes `now` as an argument, and
        // the engine never sees it at all (affinity-token §2, AGENTS.md rule 2).
        self.now = T0.saturating_add(u32::try_from(now.as_nanos() / 1_000_000_000).unwrap_or(0));
        match input {
            Input::Message {
                from,
                message: Message::Request(request),
            } => self.on_request(from, request),
            Input::Message {
                message: Message::Response(response),
                ..
            } => self.on_response(response),
            _ => Vec::new(),
        }
    }
}

// ----------------------------------------------------------------------------- the endpoints ----

/// Alice: places the call through edge A, then sends the `BYE` to edge B.
#[derive(Debug)]
struct Alice {
    edge_a: NodeId,
    /// Where the mid-dialog request goes. **A different edge** — the whole point.
    edge_b: NodeId,
    /// The route set learned from the 200's `Record-Route`, RFC 3261 §12.1.2: *reversed*.
    route_set: Vec<String>,
    remote_target: Option<String>,
    answered: Vec<u16>,
    cseq: u32,
}

impl SimNode for Alice {
    fn name(&self) -> &'static str {
        "alice"
    }

    fn on_input(&mut self, _now: SimTime, input: Input<'_>) -> Vec<Effect> {
        match input {
            Input::Started => vec![send(self.edge_a, self.invite())],
            Input::Message {
                message: Message::Response(response),
                ..
            } => self.on_response(response),
            _ => Vec::new(),
        }
    }
}

impl Alice {
    fn on_response(&mut self, response: &Response) -> Vec<Effect> {
        if response.status.is_provisional() {
            return Vec::new();
        }
        let cseq = header(response, &HeaderName::CSeq).unwrap_or_default();
        self.answered.push(response.status.code());

        if cseq.ends_with("BYE") {
            return vec![Effect::Note(format!(
                "alice got {} for BYE",
                response.status.code()
            ))];
        }

        // RFC 3261 §12.1.2 — the UAC's route set is the `Record-Route` values **reversed**, so the
        // pair's ORIG entry (pushed first, therefore lowest) becomes the first `Route` it sends.
        // That is affinity-token §7 M2's entire purpose: each side presents its own direction.
        self.route_set = record_routes(response);
        self.route_set.reverse();
        self.remote_target = Some(BOB_CONTACT.to_owned());

        vec![
            Effect::Note("alice answered".to_owned()),
            // The `ACK` for a 2xx goes back through edge A: a separately routed request whose first
            // `Route` is the one alice's own route set names.
            send(self.edge_a, self.ack()),
            // ...and the `BYE` goes to **edge B**, which has never seen this dialog. In a
            // deployment the L4 dataplane decides which edge a datagram lands on; here the scenario
            // decides, because "any edge can route it" is the claim under test.
            send(self.edge_b, self.bye()),
        ]
    }

    fn dialog_request(&mut self, method: &Method) -> Message {
        let target = self
            .remote_target
            .clone()
            .unwrap_or_else(|| BOB_CONTACT.to_owned());
        // §17.1.1.3: the `ACK` for a 2xx carries the INVITE's CSeq number, not a new one.
        if *method != Method::Ack {
            self.cseq += 1;
        }
        let mut builder = RequestBuilder::new(method.clone(), uri(&target))
            .header(HeaderName::CallId, CALL)
            .unwrap()
            .cseq(self.cseq, method)
            .unwrap()
            .header(HeaderName::From, "<sip:alice@a.example>;tag=alice-tag")
            .unwrap()
            .header(HeaderName::To, "<sip:bob@b.example>;tag=bob-tag")
            .unwrap()
            .header(HeaderName::MaxForwards, "70")
            .unwrap()
            .header(
                HeaderName::Via,
                format!(
                    "SIP/2.0/UDP alice.example;branch=z9hG4bK-alice-{}-{method}",
                    self.cseq
                ),
            )
            .unwrap();
        for route in &self.route_set {
            builder = builder.header(HeaderName::Route, route.clone()).unwrap();
        }
        Message::Request(builder.build())
    }

    fn ack(&mut self) -> Message {
        self.dialog_request(&Method::Ack)
    }

    fn bye(&mut self) -> Message {
        self.dialog_request(&Method::Bye)
    }

    fn invite(&mut self) -> Message {
        self.cseq += 1;
        Message::Request(
            RequestBuilder::new(Method::Invite, uri("sip:bob@b.example"))
                .header(HeaderName::CallId, CALL)
                .unwrap()
                .cseq(self.cseq, &Method::Invite)
                .unwrap()
                .header(HeaderName::From, "<sip:alice@a.example>;tag=alice-tag")
                .unwrap()
                .header(HeaderName::To, "<sip:bob@b.example>")
                .unwrap()
                .header(HeaderName::MaxForwards, "70")
                .unwrap()
                .header(
                    HeaderName::Via,
                    "SIP/2.0/UDP alice.example;branch=z9hG4bK-alice-invite",
                )
                .unwrap()
                .build(),
        )
    }
}

/// Bob: answers the INVITE, echoes the `Record-Route` pair into the 200, answers the BYE.
#[derive(Debug, Default)]
struct Bob {
    /// The `Record-Route` values as they arrived, topmost first — the pair as it went on the wire.
    arrived_record_routes: Vec<String>,
    /// The `Route` values each arriving request carried, by method.
    arrived_routes: HashMap<String, Vec<String>>,
}

impl SimNode for Bob {
    fn name(&self) -> &'static str {
        "bob"
    }

    fn on_input(&mut self, _now: SimTime, input: Input<'_>) -> Vec<Effect> {
        let Input::Message {
            from,
            message: Message::Request(request),
        } = input
        else {
            return Vec::new();
        };
        self.arrived_routes.insert(
            request.method.to_string(),
            request
                .headers
                .get_all(&HeaderName::Route)
                .map(|header| String::from_utf8_lossy(&header.value()).trim().to_owned())
                .collect(),
        );
        match request.method {
            // An `ACK` is never answered (§7.2 K3), and a UAS does not answer one either.
            Method::Ack => vec![Effect::Note("bob got the ACK".to_owned())],
            Method::Invite => {
                self.arrived_record_routes = record_routes_of(request);
                // RFC 3261 §12.1.1: a UAS copies the `Record-Route` values into the 2xx, in order.
                // That is how the UAC learns a route set at all, and it is why the pair's order at
                // mint time decides which direction each side ends up presenting.
                let mut response = build(request, 200, "OK");
                for value in &self.arrived_record_routes {
                    if let Ok(header) = Header::build(HeaderName::RecordRoute, value.clone()) {
                        response.headers.push(header);
                    }
                }
                vec![
                    Effect::Note("bob answers 200".to_owned()),
                    send(from, Message::Response(response)),
                ]
            }
            _ => vec![
                Effect::Note(format!("bob answers {}", request.method)),
                send(from, Message::Response(build(request, 200, "OK"))),
            ],
        }
    }
}

// --------------------------------------------------------------------------------- helpers ------

fn decode(value: &Bytes) -> Result<Vec<u8>, sipx_clstr_affinity::DecodeError> {
    sipx_clstr_affinity::decode_param_value(&String::from_utf8_lossy(value))
}

fn header(response: &Response, name: &HeaderName) -> Option<String> {
    response
        .headers
        .value(name)
        .map(|value| String::from_utf8_lossy(&value).trim().to_owned())
}

fn call_id(request: &Request) -> String {
    request
        .headers
        .value(&HeaderName::CallId)
        .map(|value| String::from_utf8_lossy(&value).trim().to_owned())
        .unwrap_or_default()
}

/// §5.1 T1's test: a `To` tag is what puts a request inside a dialog.
fn in_dialog(request: &Request) -> bool {
    request
        .headers
        .value(&HeaderName::To)
        .and_then(|value| Address::parse(&value, "To").ok())
        .is_some_and(|address| address.tag().is_some())
}

fn is_dialog_forming(request: &Request) -> bool {
    request.method == Method::Invite && !in_dialog(request)
}

fn record_routes_of(request: &Request) -> Vec<String> {
    request
        .headers
        .get_all(&HeaderName::RecordRoute)
        .map(|header| String::from_utf8_lossy(&header.value()).trim().to_owned())
        .collect()
}

fn record_routes(response: &Response) -> Vec<String> {
    response
        .headers
        .get_all(&HeaderName::RecordRoute)
        .map(|header| String::from_utf8_lossy(&header.value()).trim().to_owned())
        .collect()
}

/// The `aft` parameter value on a `Record-Route`/`Route` value, or `None`.
fn token_param(value: &str) -> Option<String> {
    let address = Address::parse(&Bytes::copy_from_slice(value.as_bytes()), "Route").ok()?;
    let raw = address.uri.params()?.value(TOKEN_PARAM)?.to_vec();
    String::from_utf8(raw).ok()
}

fn top_via_branch(response: &Response) -> Option<String> {
    let via = response.headers.value(&HeaderName::Via)?;
    let text = String::from_utf8_lossy(&via).into_owned();
    let start = text.find("branch=")? + "branch=".len();
    let rest = text.get(start..)?;
    let end = rest.find(';').unwrap_or(rest.len());
    rest.get(..end).map(str::to_owned)
}

fn build(request: &Request, status: u16, reason: &str) -> Response {
    ResponseBuilder::to_request(
        request,
        StatusCode::new(status).expect("a valid status"),
        reason.to_owned(),
    )
    .expect("a response")
    .build()
}

/// The direction a minted parameter value carries, verified under the cluster key set.
fn direction_of(value: &str) -> Direction {
    let bytes = sipx_clstr_affinity::decode_param_value(value).expect("base64url");
    let (_, keys) = cluster_keys();
    match verify(&bytes, &keys, T0 + 1, &Expect::new()) {
        Verdict::Valid(claims) => claims.direction,
        Verdict::Invalid(reason) => panic!("a freshly minted token verifies: {reason:?}"),
    }
}

// -------------------------------------------------------------------------------- the scenario --

const EDGE_A: NodeId = NodeId::from_index(0);
const EDGE_B: NodeId = NodeId::from_index(1);
const ALICE: NodeId = NodeId::from_index(2);
const BOB: NodeId = NodeId::from_index(3);

/// Two edges, two endpoints, one key set and **no shared dialog state whatsoever**.
fn scenario(seed: u64) -> Sim {
    let mut sim = Sim::new(seed);
    sim.link_default(LinkKind::Datagram, LinkPolicy::CLEAN);

    // Where each next hop lives. Both edges resolve the same names, because both *are* the cluster.
    let mut reachable = HashMap::new();
    reachable.insert(BOB_CONTACT.to_owned(), BOB);
    reachable.insert("sip:bob@b.example".to_owned(), BOB);
    reachable.insert("sip:alice@a.example".to_owned(), ALICE);

    // Edge A resolves bob and mints. Edge B resolves **nothing** and mints nothing: it has the key
    // set and the message, and that has to be enough.
    let mut edge_a = Edge::new(
        "edge-a",
        seed,
        true,
        vec![Target {
            uri: Bytes::from_static(BOB_CONTACT.as_bytes()),
            route_set: Vec::new(),
            q: 1_000,
        }],
    );
    edge_a.reachable.clone_from(&reachable);
    let mut edge_b = Edge::new("edge-b", seed, false, Vec::new());
    edge_b.reachable = reachable;

    sim.add_node(Box::new(edge_a));
    sim.add_node(Box::new(edge_b));
    sim.add_node(Box::new(Alice {
        edge_a: EDGE_A,
        edge_b: EDGE_B,
        route_set: Vec::new(),
        remote_target: None,
        answered: Vec::new(),
        cseq: 0,
    }));
    sim.add_node(Box::new(Bob::default()));

    for (a, b) in [
        (EDGE_A, ALICE),
        (EDGE_A, BOB),
        (EDGE_B, ALICE),
        (EDGE_B, BOB),
    ] {
        sim.link(a, b, LinkKind::Datagram, LinkPolicy::CLEAN);
    }
    sim
}

/// The M2 counter, read the way the HUD and the metrics pipeline read it.
fn cross_node_dialog_lookups(sim: &Sim) -> Option<u64> {
    viz::invariants(sim.trace())
        .into_iter()
        .find(|invariant| invariant.name == "cross_node_dialog_lookups")
        .and_then(|invariant| invariant.value)
}

// ----------------------------------------------------------------------------------- the rows ---

#[test]
fn a_dialog_forming_request_through_edge_a_yields_a_record_route_token() {
    // The mint half of the round trip: F4 stamps the **pair**, TERM topmost, each entry carrying
    // exactly one `aft` inside the budget (affinity-token §5, §7 M1/M2).
    let mut sim = scenario(0x_af05_0001);
    sim.run_until_idle().expect("the scenario runs");
    let trace = sim.trace().render();

    let bob = sim.node::<Bob>(BOB).expect("bob");
    let pair = bob.arrived_record_routes.clone();
    assert_eq!(pair.len(), 2, "M1: one entry per side — the pair\n{trace}");

    let tokens: Vec<String> = pair.iter().filter_map(|value| token_param(value)).collect();
    assert_eq!(tokens.len(), 2, "both entries carry a token: {pair:?}");
    assert_ne!(
        tokens.first(),
        tokens.get(1),
        "M4: the two entries of a pair must not share a nonce, so they cannot share bytes"
    );
    for value in &tokens {
        let parameter = format!(";{TOKEN_PARAM}={value}");
        assert!(
            parameter.len() <= TOKEN_PARAM_BUDGET,
            "F4's budget is per parameter, and this one is {} B",
            parameter.len()
        );
    }

    // M2's order, and it is what makes each side present its own direction: the TERM entry is
    // pushed last and is therefore topmost, so the UAS reads it first (§12.1.1, in order) and the
    // UAC reads ORIG first (§12.1.2, reversed).
    assert_eq!(
        tokens
            .iter()
            .map(|value| direction_of(value))
            .collect::<Vec<_>>(),
        vec![Direction::Terminating, Direction::Originating],
        "the TERM entry is topmost (§7 M2)\n{trace}"
    );
}

#[test]
fn the_mid_dialog_request_at_edge_b_routes_on_the_token_with_zero_cross_node_lookups() {
    // **M2's exit criterion.** The `BYE` arrives at an edge that has never seen this dialog and is
    // forwarded correctly on nothing but what the message carried.
    let mut sim = scenario(0x_af05_0002);
    sim.run_until_idle().expect("the scenario runs");
    let trace = sim.trace().render();

    let edge_b = sim.node::<Edge>(EDGE_B).expect("edge b");
    assert_eq!(
        edge_b.lookups,
        Vec::<String>::new(),
        "edge B resolved nothing at all\n{trace}"
    );
    assert_eq!(
        cross_node_dialog_lookups(&sim),
        Some(0),
        "the M2 counter, read from the trace the way the HUD reads it\n{trace}"
    );

    // Not vacuous: edge B verified a real token and learned the presenting side from it. §8 S9
    // makes the **first-popped** entry govern, and alice's reversed route set puts ORIG first.
    assert_eq!(
        edge_b.verified,
        vec![(Direction::Originating, 7)],
        "edge B learned the direction and the tenant from the message alone\n{trace}"
    );

    // And the call concluded: the BYE reached bob through an edge with an empty target map.
    assert!(
        edge_b.targets.is_empty(),
        "edge B has nothing to look anything up in — that is what makes the zero mean something"
    );
    let bob = sim.node::<Bob>(BOB).expect("bob");
    assert_eq!(
        bob.arrived_routes.get("BYE"),
        Some(&Vec::<String>::new()),
        "the BYE reached bob with **both** platform Routes popped — P2 over the pair\n{trace}"
    );

    // The route set alice learned, and therefore the pair edge B was presented with: ORIG first,
    // because §12.1.2 reverses what §12.1.1 gave the UAS. That ordering is the reason the verdict
    // above says `Originating` and not `Terminating`, and getting it backwards is a defect no
    // single-entry test could see.
    let alice = sim.node::<Alice>(ALICE).expect("alice");
    assert_eq!(alice.route_set.len(), 2, "alice learned the pair\n{trace}");
    assert_eq!(
        alice
            .route_set
            .iter()
            .filter_map(|value| token_param(value))
            .map(|value| direction_of(&value))
            .collect::<Vec<_>>(),
        vec![Direction::Originating, Direction::Terminating],
        "the UAC presents its own side first (§12.1.2 reversed, §7 M2)\n{trace}"
    );
    assert_eq!(
        alice.answered,
        vec![200, 200],
        "the INVITE and the BYE were both answered 200\n{trace}"
    );

    // Edge A *did* look up — a dialog-forming request is a T2 address of record, and a lookup there
    // is correct. Asserted so that "zero" at edge B is a property of the mid-dialog path rather
    // than of a scenario in which nothing happened at all.
    let edge_a = sim.node::<Edge>(EDGE_A).expect("edge a");
    assert_eq!(
        edge_a.lookups,
        vec!["sip:bob@b.example".to_owned()],
        "edge A resolved the AoR exactly once (T2)\n{trace}"
    );
}

#[test]
fn the_edges_share_a_key_set_and_nothing_else() {
    // The claim the counter is evidence *for*, stated directly. If edge B held any dialog state,
    // the zero above would be measuring the wrong thing.
    let mut sim = scenario(0x_af05_0003);
    sim.run_until_idle().expect("the scenario runs");

    let edge_b = sim.node::<Edge>(EDGE_B).expect("edge b");
    assert!(edge_b.mint_key.is_none(), "edge B never minted anything");
    assert!(
        edge_b.targets.is_empty() && edge_b.lookups.is_empty(),
        "edge B has no location state and consulted none"
    );
    assert_eq!(
        edge_b.verified.len(),
        1,
        "exactly one token was verified there: the BYE's"
    );
}

#[test]
fn a_tampered_token_at_a_foreign_edge_verifies_as_nothing() {
    // P3 at the edge that did not mint — the case the whole design rests on. A token that does not
    // verify buys nothing, and "does not verify" includes one flipped byte of tag (§8 S4).
    let mut sim = scenario(0x_af05_0004);
    sim.run_until_idle().expect("the scenario runs");

    let bob = sim.node::<Bob>(BOB).expect("bob");
    let value = bob
        .arrived_record_routes
        .iter()
        .find_map(|entry| token_param(entry))
        .expect("a minted token");

    let mut edge = Edge::new("edge-b", 1, false, Vec::new());
    edge.now = T0 + 1;

    let mut bytes = sipx_clstr_affinity::decode_param_value(&value).expect("base64url");
    if let Some(last) = bytes.last_mut() {
        *last ^= 0x01;
    }
    let tampered = Bytes::from(sipx_clstr_affinity::encode_param_value(&bytes));
    let (verdict, note) = edge.verdict_for(&tampered, None);
    assert_eq!(verdict, TokenVerdict::Invalid);
    assert_eq!(
        note, "token rejected at S4",
        "the AEAD tag is what caught it"
    );

    // And the intact one still verifies, so the flipped byte is what made the difference.
    let (verdict, _) = edge.verdict_for(&Bytes::from(value), None);
    assert!(matches!(verdict, TokenVerdict::Valid { .. }));
}

#[test]
fn the_round_trip_replays_byte_for_byte_under_the_same_seed() {
    // The harness's own contract, and the reason the nonce source is a seeded stream rather than an
    // operating-system one: a token is random by construction, so a scenario that mints two per call
    // is exactly where non-determinism would enter if it could.
    for seed in [0x_af05_0010_u64, 0x_af05_0011, 0x_dead_beef] {
        let mut first = scenario(seed);
        let mut second = scenario(seed);
        first.run_until_idle().expect("runs");
        second.run_until_idle().expect("runs");
        assert_eq!(
            first.trace().render(),
            second.trace().render(),
            "seed {seed:#x} did not replay"
        );
    }
}
