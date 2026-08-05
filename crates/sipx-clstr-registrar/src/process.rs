//! The REGISTER decision function.
//!
//! Pure: the command in, the current binding set in, the policy in; an [`Outcome`] out. No clock,
//! no socket, no store handle — which is what lets every vector in
//! [location-service §9](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/location-service.md)
//! run as an ordinary unit test, and lets the CAS driver loop re-run it after a conflict without
//! re-doing anything else.
//!
//! Steps run in the order §5.1 fixes, and the first failure terminates with nothing committed.
//! That ordering is normative, not stylistic: RFC 3261 §10.3 requires a REGISTER to be processed
//! "completely or not at all".

use std::time::Duration;

use crate::binding::{BindingSet, Timestamp};
use crate::command::{
    Accepted, ContactOp, ContactOps, Outcome, RegisterCommand, Rejection, TenantPolicy,
    binding_for, describe,
};

/// The option tags this registrar implements, for S2.
const SUPPORTED: &[&str] = &["path"];

/// Decide what a REGISTER does, without doing it.
///
/// `current` is the set as read from the store. On [`Outcome::Commit`] the driver commits the
/// returned set under the revision it read, and on conflict re-reads and calls this again.
#[must_use]
pub fn process(cmd: &RegisterCommand, current: &BindingSet, policy: &TenantPolicy) -> Outcome {
    // S2 — Require. Answered before anything is looked at: a registrar that acted on a request it
    // only partly understood would be guessing at the part it did not.
    let unsupported: Vec<String> = cmd
        .require
        .iter()
        .filter(|tag| !SUPPORTED.contains(&tag.to_ascii_lowercase().as_str()))
        .cloned()
        .collect();
    if !unsupported.is_empty() {
        return Outcome::Reject(Rejection::BadExtension(unsupported));
    }

    // §5.6 — Path from a UAC that did not advertise it. Committing the binding would make the UA
    // reachable only through a route set it does not know exists, so this platform refuses where
    // RFC 3327 leaves it to local policy.
    if !cmd.path.is_empty() && !cmd.supports_path {
        return Outcome::Reject(Rejection::ExtensionRequired("path"));
    }

    // S6.1 — §5.5.1 Q1/Q2, the bound on the *request*, decided before any per-contact work and
    // before a single stored binding is read. Position is the whole rule: reconciling one REGISTER
    // compares every operation it carries against every stored binding, so the work is
    // `operations × bindings`, and §5.5's quota constrains only the second factor. A bound applied
    // after reconciliation would refuse the same requests and prevent nothing.
    //
    // Not a cheaper spelling of the quota, either. A REGISTER made entirely of refreshes and
    // removals never grows the set, so §5.5 cannot refuse it however long it is (Q5) — which is why
    // `RG-14`'s pre-check could never have covered this class, and why the input needs a bound of
    // its own rather than an earlier evaluation of the outcome's.
    //
    // A wildcard is exempt (Q4): it is one operation whose cost is proportional to the stored set
    // alone, which the quota already bounds, so the request's length cannot amplify it.
    if let ContactOps::Explicit(ops) = &cmd.contacts
        && ops.len() > policy.max_contact_ops
    {
        return Outcome::Reject(Rejection::Forbidden(
            "the REGISTER carries more contact operations than this tenant permits",
        ));
    }

    match &cmd.contacts {
        ContactOps::Wildcard => wildcard(cmd, current),
        ContactOps::Explicit(ops) => explicit(cmd, current, policy, ops),
    }
}

/// §5.4 — `Contact: *`.
fn wildcard(cmd: &RegisterCommand, current: &BindingSet) -> Outcome {
    // W2. The wildcard's whole meaning is "remove everything"; without an explicit `Expires: 0`
    // the request is asking for something the grammar cannot express, and §10.3 step 6 says to
    // reject rather than interpret.
    if cmd.expires_header != Some(0) {
        return Outcome::Reject(Rejection::BadRequest(
            "a wildcard Contact requires an explicit Expires: 0",
        ));
    }
    // W1 is enforced where the request is parsed — a `*` alongside a URI is a malformed header
    // set, not a decision. See `parse::register_command`.

    let mut set = current.clone();
    set.drop_expired(cmd.now);

    // W3 — the same ordering check as B2/B3/B5, applied to every binding at once.
    for binding in set.all() {
        if binding.call_id == cmd.call_id && cmd.cseq <= binding.cseq {
            return Outcome::Reject(Rejection::StaleSequence);
        }
    }

    if set.all().is_empty() {
        // Nothing to remove. Not a failure, and not a write either: the requested state already
        // holds, which is exactly B4's idempotency rule applied to a wildcard.
        return Outcome::Noop {
            response: Accepted::default(),
        };
    }

    Outcome::Commit {
        set: BindingSet::new(),
        response: Accepted::default(),
    }
}

/// S7 — the granted lifetime of every contact operation, decided before any mutation.
///
/// E6 fails the *whole* request, so every contact has to be decided before the first one is
/// applied; otherwise a too-brief second contact would leave the first one committed.
///
/// This is also the **first per-operation work** a reconciliation does, which is why §5.5.1's meter
/// sits here: an over-limit request is refused in [`process`] and never reaches this function, so
/// the count it reports is zero.
fn granted_expiries(
    cmd: &RegisterCommand,
    policy: &TenantPolicy,
    ops: &[ContactOp],
) -> Result<Vec<u32>, Rejection> {
    let mut granted = Vec::with_capacity(ops.len());
    for op in ops {
        #[cfg(any(test, feature = "test-suite"))]
        op_meter::record();

        let requested = op
            .expires // E1
            .or(cmd.expires_header) // E2
            .unwrap_or(policy.default_expires); // E3

        if requested == 0 {
            granted.push(0); // E4 — removal, never subject to E5 or E6.
            continue;
        }
        if requested < policy.min_expires {
            // E6 — and the response must state the minimum, or the UA cannot correct itself.
            return Err(Rejection::IntervalTooBrief {
                min: policy.min_expires,
            });
        }
        // E5 — silently lowered. §10.3 step 7 allows shortening, and the response states what was
        // actually granted, so the UA is not misled about when to refresh.
        granted.push(requested.min(policy.max_expires));
    }
    Ok(granted)
}

/// §5.2, §5.3, §5.5 — explicit `Contact` values.
///
/// §5.5.1's bound on how many of them there may be is decided in [`process`], before this is
/// reached: an over-limit request must not pay for anything here.
fn explicit(
    cmd: &RegisterCommand,
    current: &BindingSet,
    policy: &TenantPolicy,
    ops: &[ContactOp],
) -> Outcome {
    // RFC 3261 §10.2.3: a REGISTER with no Contact is a query. It answers with the complete set
    // and changes nothing.
    if ops.is_empty() {
        return Outcome::Noop {
            response: describe(current, cmd.now),
        };
    }

    let grants = match granted_expiries(cmd, policy, ops) {
        Ok(grants) => grants,
        Err(rejection) => return Outcome::Reject(rejection),
    };

    let mut set = current.clone();
    set.drop_expired(cmd.now);
    // Whether reaping expired bindings already made the set differ from the durable one. Needed at
    // the end, where a reconciled set equal to `current` means there is nothing to commit — which is
    // only true if nothing was reaped on the way in.
    let reaped = set.all().len() != current.all().len();
    let mut view = Reconciling::new(set);
    let mut changed = false;
    // §5.3.2 B8's deferred decisions. Empty for every request that does not re-present a spent
    // ordering token, which is every request but a retransmission.
    let mut pending: Vec<Pending> = Vec::new();

    // §5.5 is decided **after** the loop, on the reconciled set, and `RG-14`'s pre-check ahead of it
    // is gone with no replacement.
    //
    // The rule is a test on the *committed outcome* — "a REGISTER whose committed outcome would
    // exceed it fails `403 Forbidden`" — and it says in terms that "refreshes, replacements and
    // removals never grow the set and never trip the quota". Nothing computed before this loop knows
    // that outcome, because deciding whether an operation adds a binding or lands on one *is* the
    // loop: B6 resolves each operation against the set the preceding ones left, so several operations
    // can collapse onto one binding (B7) and a later removal can take back an earlier addition.
    //
    // Two pre-checks have now been wrong in the same direction, which is why there is not a third.
    // The first counted every positive-expiry operation as an addition and refused refreshes
    // (LS-R-15). The second counted a candidate unless it was equivalent to one already counted —
    // an *upper* bound on additions, so `x;line=1, x, x;line=2` against nine held bindings was
    // answered `403` where the committed outcome is ten (LS-R-32). Both were conservative, and
    // conservative is the wrong direction here: a `403` is a policy refusal a UA cannot retry out of,
    // so refusing what the quota permits is worse than costing one more pass over the operations.
    //
    // **What the pre-check bought is bought elsewhere now, which is why deleting it costs nothing.**
    // Its only purpose was to bound reconciliation work, and it was sound solely because the most
    // active bindings a request could reach was `current_active + genuine_additions` — a premise B6
    // and B7 retired by letting several operations collapse onto one binding. §5.5.1 bounds the
    // request's *length* instead (`max_contact_ops`, refused in [`process`] before a single stored
    // binding is read), and that needs no premise about reconciliation at all: with both factors of
    // `operations × bindings` bounded by policy, the quota is free to be asked once, on the only set
    // that can answer it.
    for (op, granted) in ops.iter().zip(grants.iter().copied()) {
        // §5.3 — binding identity by §19.1.4 comparison. Equivalence is non-transitive, so an
        // incoming contact can match more than one stored binding; the first in creation order is
        // the one updated, which is a deterministic choice a vector can assert. Matching reads the
        // view's parsed contacts, so the cost per op is a scan of `equivalent` calls, not
        // `bindings` parses (`RG-14`).
        //
        // §5.3.2 B6 — and it is matched against the set **as the preceding operations left it**,
        // which is the whole reason the view owns the set rather than sitting beside it.
        match view.find(&op.uri) {
            None => {
                if granted == 0 {
                    continue; // B1 — removing a contact that is not there is not an error.
                }
                view.insert(binding_for(op, cmd, granted, cmd.now), op.uri.clone());
                changed = true;
            }
            Some(index) => {
                let Some(slot) = view.slot_id(index) else {
                    continue;
                };
                let Some(stored) = view.get(index) else {
                    continue;
                };

                // §5.3.2 B8 — this operation is now the request's last word on the binding it
                // resolved to, so every decision still waiting on that binding is re-based onto this
                // grant. Recorded as the loop walks forward rather than predicted from where it
                // stands, because "the last operation that resolves to it" is B6's resolution: which
                // binding an operation lands on depends on what the operations *between* the two did
                // to the set, and nothing that has not run yet knows that.
                for deferred in &mut pending {
                    if deferred.slot == slot {
                        deferred.net = granted;
                    }
                }

                // §5.3.2 B7 — B2–B5 compare this request's token against the token of the request
                // that last *wrote* the matched binding. When an earlier operation of this same
                // request wrote it, that token is this command's own, so the comparison decides
                // nothing and B6 has already fixed the order: the operation applies.
                //
                // The flag is what tells this apart from a retransmission, which arrives carrying
                // the token stored on the binding too and must stay B4's (§5.3.1). The two cases
                // are token-identical, so the registrar answers from what it knows — it performed
                // this write itself, a moment ago — rather than inferring it from a comparison
                // that cannot separate them.
                if !view.written_here(index) && stored.call_id == cmd.call_id {
                    if cmd.cseq < stored.cseq {
                        // B5 — the ordering token went backwards. Abort everything.
                        return Outcome::Reject(Rejection::StaleSequence);
                    }
                    if cmd.cseq == stored.cseq {
                        // B4 — an idempotent retry, but only if the stored state already *is* what
                        // this command asks for. Same token with a different request is not a
                        // retry; it is a second write under a spent token, which B5 refuses.
                        if already_holds(stored, cmd, granted) {
                            continue;
                        }
                        // §5.3.2 B8 — and "what this command asks for" is stated per *binding*, not
                        // per operation. Once B6 lets several operations of one request resolve to
                        // one binding, the command's requested outcome for it is the **last** of
                        // them; the earlier ones are writes this same request overwrites before it
                        // commits anything. Comparing this operation's own grant instead answered a
                        // verbatim retransmission of `CC;expires=3600, CC;expires=7200` with B5's
                        // `500` — the stored binding holds 7200, exactly what the command asks — so
                        // the one request B7 exists to legalise was the one a UA could not
                        // retransmit (LS-R-33).
                        //
                        // Which later operation supersedes this one is not knowable here, so it is
                        // not guessed at: the decision is deferred and this operation is left
                        // unapplied. Deferring is exactly self-consistent, because the two answers
                        // this branch can give are "no mutation" (B4) and "abort the request" (B5) —
                        // never "apply" — so the continuation the rest of the loop reconciles is the
                        // one the answer will turn out to have described, whichever it is.
                        pending.push(Pending {
                            slot,
                            stored: stored.clone(),
                            net: granted,
                        });
                        continue;
                    }
                    // B3 — newer CSeq, same Call-ID: apply.
                } // B2 — a different Call-ID applies regardless of CSeq: the UA restarted, and its
                // sequence numbering restarted with it.

                let registered_at = stored.registered_at;
                if granted == 0 {
                    view.remove(index);
                } else {
                    view.replace(
                        index,
                        binding_for(op, cmd, granted, registered_at),
                        op.uri.clone(),
                    );
                }
                changed = true;
            }
        }
    }

    // §5.3.2 B8, decided now that the request's last word on every binding is known. Each deferred
    // operation re-presented a spent ordering token; it is B4's retry when the store already holds
    // what the request asks of that binding, and B5's second write under a spent token when it does
    // not. B4's remedy needs nothing further — the operation was left unapplied, which is exactly
    // "no mutation" (B4.2).
    //
    // Ahead of the quota deliberately. §5.5 says that where a request would both exceed the quota and
    // abort under S9, S9's failure is the one reported, so a `500` here must not be overtaken by a
    // `403` computed from a set this request is not allowed to commit in the first place.
    for deferred in &pending {
        if !already_holds(&deferred.stored, cmd, deferred.net) {
            return Outcome::Reject(Rejection::StaleSequence);
        }
    }

    let set = view.into_set();

    // S8 — the quota (§5.5), and the only place it is asked: on the reconciled set, which is the
    // "committed outcome" the rule names. A refresh, a replacement or a removal cannot grow the set,
    // so none of them can trip it, and a request whose operations collapse onto fewer bindings than
    // it names is measured by what it commits rather than by what it carries.
    if set.active_count(cmd.now) > policy.max_bindings_per_aor {
        return Outcome::Reject(Rejection::Forbidden(
            "the address-of-record already holds its maximum bindings",
        ));
    }

    let response = describe(&set, cmd.now);
    // §5.3.2 B9 — a request whose reconciled set is the set it read commits nothing, so the revision
    // does not move (B4.2). `changed` records that an operation *mutated the view*, which is not the
    // same question: an addition a later operation of the same request takes back leaves the durable
    // set exactly as it was (LS-R-30's shape), and committing it would spend a revision and publish a
    // change event describing no change. The two deliveries of such a request are also
    // indistinguishable from the store — nothing survives to carry the ordering token — so the only
    // way the retransmission can be idempotent is for neither delivery to write.
    //
    // Only ever a downgrade: `reaped` keeps a set that lost expired bindings on the way in from
    // comparing equal to the durable one.
    if changed && (reaped || set != *current) {
        Outcome::Commit { set, response }
    } else {
        Outcome::Noop { response }
    }
}

/// The binding set under reconciliation, beside the parsed contacts §5.3 matches against.
///
/// One type rather than two locals, because the correctness of the whole loop is the invariant that
/// the two stay the same length in the same order — and `RG-16` was that invariant belonging to
/// nobody. The view was parsed once from the *original* vector and every operation resolved against
/// it, so the first removal left every later operation naming an entry that had moved: one REGISTER
/// carrying `CA;expires=0`, `CB;expires=0` and a new `CC` removed CA, resolved CB's removal past the
/// end of the shortened set, and committed CB as though the request had never mentioned it. With a
/// third binding present it was worse than a survivor — the refresh landed on whichever binding had
/// slid into the index, overwriting a contact the request never named.
///
/// Single-contact REGISTER cannot reach any of that, which is why every existing proof was green.
/// The fix is structural rather than a re-parse: only this type mutates the set, and every mutation
/// moves the view with it (§5.3.2 B6), so the per-reconciliation parse budget `RG-14` bought — one
/// parse per stored binding, none per operation — is unchanged.
///
/// Resolving operations against the live set is also what makes B7 reachable, so the view carries
/// the second fact B7 needs: which bindings *this* request wrote. Nothing else can supply it — a
/// binding written a moment ago by an earlier operation and a binding written by an earlier
/// delivery of the same REGISTER are byte-identical in `(Call-ID, CSeq)`, and only the first is
/// outside B4/B5's reach.
struct Reconciling {
    set: BindingSet,
    /// `slots[i]` describes `set.all()[i]`. One vector rather than one per fact, so there is no
    /// second alignment to keep — the invariant is `slots.len() == set.all().len()`, and the three
    /// mutators below are the only things that can change either side of it.
    slots: Vec<Slot>,
    /// The next unused [`Slot::id`]. Monotonic for the life of one reconciliation.
    next_id: usize,
}

/// What reconciliation knows about one binding beyond the binding itself.
struct Slot {
    /// A name for this binding that survives the set being reordered under it (§5.3.2 B8).
    ///
    /// An index is not a name here: a removal shifts every later binding down one, so a decision
    /// deferred against index 3 would silently re-target when index 2 went away — the very class of
    /// defect `RG-16` exists to close, reintroduced one level up. The id is minted on the way in and
    /// carried through `replace`, because a replaced binding is still the binding the request matched.
    id: usize,
    /// The binding's contact URI, or `None` when those bytes do not parse.
    ///
    /// A binding whose stored contact will not parse matches nothing, which is the same answer a
    /// comparison against it would have produced; it is kept in place rather than dropped so the
    /// indices of everything after it are still the set's.
    uri: Option<sipx_sip::Uri>,
    /// Whether an earlier operation of **this** request wrote this binding (§5.3.2 B7).
    ///
    /// Recorded rather than derived, because it cannot be derived: a binding this request just
    /// wrote and a binding an earlier delivery of the same request wrote carry the same
    /// `(Call-ID, CSeq)`, and the second is B4's retransmission while the first is not a write
    /// under a spent token at all. Only the reconciliation that performed the write knows which
    /// it is looking at.
    written_here: bool,
}

/// One §5.3.2 B8 decision the loop could not answer where it arose.
///
/// An operation that re-presents a spent ordering token is B4's retry or B5's abort depending on the
/// **net** outcome the request asks of the binding it matched — the grant of the last operation of the
/// request that resolves there. Resolution is B6's, so what that last operation is depends on what the
/// operations between the two do to the set, and no amount of looking at the set as it stands now can
/// tell: predicting it from the current view is the defect this record exists to remove, which
/// committed `[CC;line=9, CC;line=9]` for a request whose first operation should have aborted it
/// (LS-R-35).
///
/// Deferring costs nothing in soundness, because this branch's two answers are "no mutation" and
/// "abort the whole request" — never "apply". Leaving the operation unapplied is therefore the
/// continuation both answers describe, so the rest of the loop reconciles the set that will actually
/// have happened, and the answer can be read off at the end.
struct Pending {
    /// The [`Slot::id`] of the binding the deferred operation matched.
    slot: usize,
    /// That binding as the request found it — B4's comparison base, and durable state rather than
    /// anything this request wrote.
    stored: crate::binding::Binding,
    /// The grant of the newest operation seen so far that resolves to `slot`; the deferred
    /// operation's own until a later one supersedes it.
    net: u32,
}

impl Reconciling {
    /// Parse each stored contact **once** for the whole reconciliation (`RG-14`).
    ///
    /// The loop this replaces re-parsed every stored binding for every incoming op —
    /// `O(contacts · bindings)` parses, and against an open registrar that is a CPU amplifier an
    /// attacker can tune. The parse is the expensive part; equivalence on two already-parsed URIs
    /// is not.
    fn new(set: BindingSet) -> Self {
        let slots: Vec<Slot> = set
            .all()
            .iter()
            .enumerate()
            .map(|(index, binding)| {
                #[cfg(test)]
                parse_meter::record();
                Slot {
                    id: index,
                    uri: sipx_sip::Uri::parse(binding.contact.clone()).ok(),
                    // Everything the set arrived with was written by some earlier request — this
                    // one has not applied an operation yet.
                    written_here: false,
                }
            })
            .collect();
        let next_id = slots.len();
        Self {
            set,
            slots,
            next_id,
        }
    }

    /// The first binding in creation order whose contact is §19.1.4-equivalent to `uri`.
    ///
    /// Equivalence is non-transitive, so more than one stored binding can match; §5.3 makes the
    /// first the one that is updated.
    fn find(&self, uri: &sipx_sip::Uri) -> Option<usize> {
        self.slots.iter().position(|slot| {
            slot.uri
                .as_ref()
                .is_some_and(|stored| stored.equivalent(uri))
        })
    }

    /// The binding at `index`, if the set still holds one there.
    fn get(&self, index: usize) -> Option<&crate::binding::Binding> {
        self.set.all().get(index)
    }

    /// Whether an earlier operation of this request wrote the binding at `index` (§5.3.2 B7).
    fn written_here(&self, index: usize) -> bool {
        self.slots.get(index).is_some_and(|slot| slot.written_here)
    }

    /// The reorder-proof name of the binding at `index` (§5.3.2 B8).
    fn slot_id(&self, index: usize) -> Option<usize> {
        self.slots.get(index).map(|slot| slot.id)
    }

    /// Add a binding whose contact parsed to `uri`, keeping both halves in creation order.
    fn insert(&mut self, binding: crate::binding::Binding, uri: sipx_sip::Uri) {
        let at = self.set.insert_at(binding);
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        // `insert_at` computes its position by `partition_point` over the set *before* the insert,
        // so with `slots.len() == set.all().len()` holding on entry, `at` is at most `slots.len()`
        // — exactly the range `Vec::insert` accepts. The clamp is therefore unreachable, and kept
        // only so that a future change breaking that invariant mis-places a slot instead of
        // panicking on network input (AGENTS.md #3). It is a belt on a proof, not a live hazard.
        let at = at.min(self.slots.len());
        self.slots.insert(
            at,
            Slot {
                id,
                uri: Some(uri),
                written_here: true,
            },
        );
    }

    /// Replace the binding at `index` with one whose contact parsed to `uri`.
    ///
    /// The slot keeps its `id`: a replaced binding is still the binding an operation matched, which is
    /// what §5.3.2 B8's deferred decisions are recorded against.
    fn replace(&mut self, index: usize, binding: crate::binding::Binding, uri: sipx_sip::Uri) {
        if let Some(slot) = self.slots.get_mut(index) {
            slot.uri = Some(uri);
            slot.written_here = true;
            self.set.replace(index, binding);
        }
    }

    /// Drop the binding at `index` from both halves.
    fn remove(&mut self, index: usize) {
        if index < self.slots.len() {
            let _ = self.slots.remove(index);
            self.set.remove(index);
        }
    }

    /// The reconciled set. The parsed view has no life beyond the loop.
    fn into_set(self) -> BindingSet {
        self.set
    }
}

/// A test-only meter over how many times a **stored** contact URI is parsed while reconciling one
/// REGISTER (`RG-14`).
///
/// The amplification this story fixes is not visible in wall-clock time without flakiness, so the
/// cost is made observable: parsing a stored contact once per comparison is `ops · bindings` work,
/// and parsing each once is `bindings`.
///
/// **A thread-local rather than an atomic**, and that distinction is load-bearing. `process` runs
/// synchronously on its caller's thread, so a thread-local lets each test see only its own parses.
/// A global atomic does not: the suite runs tests in parallel, and a sibling test's parses leak into
/// the delta — which is exactly how the first version of this meter reported 3 where 2 was correct.
#[cfg(test)]
pub(crate) mod parse_meter {
    use std::cell::Cell;

    thread_local! {
        static STORED_PARSES: Cell<usize> = const { Cell::new(0) };
    }

    /// Forget every parse counted so far on this thread.
    pub(crate) fn reset() {
        STORED_PARSES.with(|count| count.set(0));
    }

    /// Count one parse of a stored contact URI.
    pub(crate) fn record() {
        STORED_PARSES.with(|count| count.set(count.get() + 1));
    }

    /// How many stored-contact parses have happened on this thread since the last [`reset`].
    pub(crate) fn count() -> usize {
        STORED_PARSES.with(Cell::get)
    }
}

/// A meter over how many **contact operations of the request** one reconciliation examined
/// (`RG-25`).
///
/// A second instrument beside `parse_meter`, because the two count different factors of the same
/// product and neither can see the other's. `parse_meter` counts *stored* contacts, so it measures
/// work proportional to the binding set: against an empty address-of-record it reads `0` whether a
/// REGISTER carries one contact operation or three thousand, which is precisely the class §5.5.1
/// bounds. This one counts the request's own operations, so an over-limit request reads `0` and a
/// conforming one reads its length.
///
/// A thread-local, for `parse_meter`'s reason: `process` runs synchronously on its caller's
/// thread, and a global atomic would let a sibling test's operations leak into the delta.
///
/// Compiled under `test-suite` as well as `test` so the shared conformance suite can assert the
/// bound's *cost* on every backend rather than only its answer — the in-memory store and PostgreSQL
/// run the identical rows, and a row that checked the status alone would pass on a backend that had
/// paid for the whole reconciliation first.
#[cfg(any(test, feature = "test-suite"))]
pub(crate) mod op_meter {
    use std::cell::Cell;

    thread_local! {
        static OPS_EXAMINED: Cell<usize> = const { Cell::new(0) };
    }

    /// Forget every operation counted so far on this thread.
    pub(crate) fn reset() {
        OPS_EXAMINED.with(|count| count.set(0));
    }

    /// Count one contact operation examined.
    pub(crate) fn record() {
        OPS_EXAMINED.with(|count| count.set(count.get() + 1));
    }

    /// How many contact operations have been examined on this thread since the last [`reset`].
    pub(crate) fn count() -> usize {
        OPS_EXAMINED.with(Cell::get)
    }
}

/// Whether the stored binding already is what this command asks for — B4's precise condition.
fn already_holds(stored: &crate::binding::Binding, cmd: &RegisterCommand, granted: u32) -> bool {
    if granted == 0 {
        // A removal whose target is still present has not been applied.
        return false;
    }
    // §5.3.1 B4.1 — the base is the granted **duration**, not the absolute deadline. `now` is
    // stamped when a request is admitted, so two deliveries of one REGISTER never share one: a UDP
    // retransmission after a lost `200`, a CAS re-read and a re-presentation at a second node all
    // arrive later than the delivery that wrote the binding. Comparing deadlines would call every
    // retry that actually happens a second write and refuse it (B5), which is how this rule came to
    // answer an ordinary retransmission `500`.
    //
    // What the caller does with a `true` is `continue` — no mutation at all (B4.2). The deadline is
    // deliberately *not* pushed out: a retry that refreshed it would let one ordering token buy a
    // second lifetime.
    stored.refreshed_at.until(stored.expires_at) == Duration::from_secs(u64::from(granted))
        && stored.path == cmd.path
}

/// Whether a set is empty of active bindings at `now` — for the driver's housekeeping decisions.
#[must_use]
pub fn is_empty_at(set: &BindingSet, now: Timestamp) -> bool {
    set.active_count(now) == 0
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod rg14_tests {
    use super::*;
    use crate::binding::{Binding, Timestamp};
    use crate::command::{ContactOp, ContactOps, RegisterCommand};
    use bytes::Bytes;
    use sipx_sip::Uri;

    fn aor(uri: &str) -> crate::CanonicalAor {
        crate::CanonicalAor::parse(uri.to_owned()).expect("a well-formed AoR")
    }

    fn op(contact: &str) -> ContactOp {
        ContactOp {
            uri: Uri::parse(Bytes::copy_from_slice(contact.as_bytes())).expect("a valid contact"),
            verbatim: Bytes::copy_from_slice(contact.as_bytes()),
            expires: Some(3600),
            q: None,
            instance_id: None,
            reg_id: None,
            push: None,
        }
    }

    fn command(contacts: Vec<ContactOp>) -> RegisterCommand {
        RegisterCommand {
            tenant: "t".to_owned(),
            aor: aor("sip:alice@example.test"),
            call_id: Bytes::from_static(b"rg14-call"),
            cseq: 1,
            contacts: ContactOps::Explicit(contacts),
            expires_header: None,
            path: Vec::new(),
            supports_path: false,
            require: Vec::new(),
            received: None,
            flow_ref: None,
            principal: None,
            now: Timestamp::from_secs(1000),
        }
    }

    /// **`RG-14`'s failing-first test.** The number of times a *stored* contact is parsed during one
    /// reconciliation must be the number of stored bindings, not (ops × bindings). Before this
    /// story, matching re-parsed every stored contact for every incoming op, so a REGISTER with many
    /// contacts against an address-of-record holding many bindings cost `O(ops · bindings)` parses —
    /// and the quota that would cap the result was applied after all of that work was done.
    ///
    /// Counted, not timed: wall-clock would be flaky, a parse is the expensive unit.
    #[test]
    fn rg14_reconciliation_parses_each_stored_contact_once() {
        // Ten bindings already stored for this address-of-record.
        let mut current = BindingSet::new();
        for i in 0..10 {
            current.insert(binding(&format!("sip:alice@10.0.0.{i}:5060")));
        }

        // And a REGISTER carrying ten contacts.
        let contacts: Vec<ContactOp> = (0..10)
            .map(|i| op(&format!("sip:alice@198.51.100.{i}:5060")))
            .collect();
        let cmd = command(contacts);

        // A quota big enough not to interfere: this test is about *parses*, and a `403` on the way out
        // would say nothing about how the view was built on the way in.
        let generous = TenantPolicy {
            max_bindings_per_aor: 64,
            ..TenantPolicy::default()
        };
        parse_meter::reset();
        let _ = process(&cmd, &current, &generous);
        let parses = parse_meter::count();

        assert_eq!(
            parses, 10,
            "one parse per stored binding, not one per (op × binding); got {parses}"
        );
    }

    /// An op that matches a stored binding still resolves, through the precomputed view rather than
    /// a fresh parse of every candidate.
    #[test]
    fn rg14_a_matching_contact_still_updates_the_same_binding() {
        let mut current = BindingSet::new();
        current.insert(binding("sip:alice@10.0.0.5:5060"));

        let cmd = command(vec![op("sip:alice@10.0.0.5:5060")]);
        let outcome = process(&cmd, &current, &TenantPolicy::default());

        // A refresh of the same contact must not insert a second one. The active count is read
        // back out of the set that comes back, however it came back.
        match outcome {
            Outcome::Commit { set, .. } => {
                assert_eq!(set.active(cmd.now).count(), 1, "a refresh updates in place");
            }
            Outcome::Noop { response } => {
                let _ = response;
                // No change is also a correct answer for a refresh that is already the committed
                // state — but the stored set must still hold exactly one active binding.
                assert_eq!(current.active(cmd.now).count(), 1);
            }
            Outcome::Reject(reason) => panic!("a refresh must not be refused: {reason:?}"),
        }
    }

    fn binding(contact: &str) -> Binding {
        Binding {
            contact: Bytes::copy_from_slice(contact.as_bytes()),
            q: 1000,
            call_id: Bytes::from_static(b"other"),
            cseq: 1,
            expires_at: Timestamp::from_secs(3600),
            registered_at: Timestamp::from_secs(1),
            refreshed_at: Timestamp::from_secs(1),
            path: Vec::new(),
            received: None,
            instance_id: None,
            reg_id: None,
            flow_ref: None,
            push: None,
            principal: None,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod rg14_quota_tests {
    use super::*;
    use crate::binding::{Binding, Timestamp};
    use crate::command::{ContactOp, ContactOps, RegisterCommand};
    use bytes::Bytes;
    use sipx_sip::Uri;

    fn aor(uri: &str) -> crate::CanonicalAor {
        crate::CanonicalAor::parse(uri.to_owned()).expect("a well-formed AoR")
    }

    fn policy(max: usize) -> TenantPolicy {
        TenantPolicy {
            max_bindings_per_aor: max,
            ..TenantPolicy::default()
        }
    }

    fn op(contact: &str, expires: u32) -> ContactOp {
        ContactOp {
            uri: Uri::parse(Bytes::copy_from_slice(contact.as_bytes())).expect("a valid contact"),
            verbatim: Bytes::copy_from_slice(contact.as_bytes()),
            expires: Some(expires),
            q: None,
            instance_id: None,
            reg_id: None,
            push: None,
        }
    }

    fn command(contacts: Vec<ContactOp>) -> RegisterCommand {
        RegisterCommand {
            tenant: "t".to_owned(),
            aor: aor("sip:alice@example.test"),
            call_id: Bytes::from_static(b"rg14-quota"),
            cseq: 1,
            contacts: ContactOps::Explicit(contacts),
            expires_header: None,
            path: Vec::new(),
            supports_path: false,
            require: Vec::new(),
            received: None,
            flow_ref: None,
            principal: None,
            now: Timestamp::from_secs(1000),
        }
    }

    fn binding(contact: &str, expires_at: u64) -> Binding {
        Binding {
            contact: Bytes::copy_from_slice(contact.as_bytes()),
            q: 1000,
            call_id: Bytes::from_static(b"other"),
            cseq: 1,
            expires_at: Timestamp::from_secs(expires_at),
            registered_at: Timestamp::from_secs(1),
            refreshed_at: Timestamp::from_secs(1),
            path: Vec::new(),
            received: None,
            instance_id: None,
            reg_id: None,
            flow_ref: None,
            push: None,
            principal: None,
        }
    }

    /// **The quota half of `RG-14`, restated on what remains of it.** `RG-14` refused an over-quota
    /// request *before* reconciliation, on `current_active + genuine_additions`. `RG-16` deleted that
    /// pre-check — B6/B7 retired the premise it was sound under — so the refusal now comes from the
    /// single check on the reconciled set, and `RG-14`'s own numbers are what this still holds:
    /// the answer is unchanged, and the parse budget is unchanged.
    ///
    /// One parse per stored binding is the whole cost either way, because building the match view is
    /// what the parses are for, and the view is built once. The per-op × per-binding re-parse the
    /// amplifier was is what this is measured against; §5.5.1 is what bounds the other factor now.
    #[test]
    fn rg14_an_over_quota_request_is_refused_for_one_parse_per_stored_binding() {
        // The address-of-record holds two active bindings; the quota is three. Two additions follow.
        let mut current = BindingSet::new();
        current.insert(binding("sip:alice@10.0.0.1:5060", 3600));
        current.insert(binding("sip:alice@10.0.0.2:5060", 3600));

        let cmd = command(vec![
            op("sip:alice@198.51.100.1:5060", 3600),
            op("sip:alice@198.51.100.2:5060", 3600),
        ]);

        parse_meter::reset();
        let outcome = process(&cmd, &current, &policy(3));
        let parses = parse_meter::count();

        assert!(
            matches!(outcome, Outcome::Reject(Rejection::Forbidden(_))),
            "2 active + 2 additions over a quota of 3 must be refused, got {outcome:?}"
        );
        // One parse per stored binding to build the match view — the floor, not the amplifier's
        // ops × bindings. 2 stored bindings, 2 parses, no more.
        assert_eq!(
            parses, 2,
            "a refusal costs one parse per stored binding to distinguish a refresh from a new binding; got {parses}"
        );
    }

    /// Two operations naming one contact are one binding against the quota (§5.3.2 B6/B7). Empty set,
    /// quota 1, and one REGISTER naming the same contact twice: the committed set is one binding, so
    /// the request fits. This is the shape that retired `RG-14`'s pre-check rather than tightening it —
    /// a check that counted both operations as additions answers `403` where the reconciled set is
    /// within the quota, and `403` is a refusal a UA cannot retry out of.
    #[test]
    fn b7_two_operations_naming_one_contact_are_one_addition_against_the_quota() {
        let cmd = command(vec![
            op("sip:alice@10.0.0.9:5060", 3600),
            op("sip:alice@10.0.0.9:5060", 7200),
        ]);
        let outcome = process(&cmd, &BindingSet::new(), &policy(1));

        let Outcome::Commit { set, .. } = outcome else {
            panic!("one contact named twice is one binding, so a quota of 1 fits: {outcome:?}");
        };
        assert_eq!(set.active_count(cmd.now), 1);
    }

    /// The early check must not change *which* requests are accepted. A request that fits within the
    /// upper bound proceeds to the same answer it gave before — a refresh of an existing contact.
    #[test]
    fn rg14_a_request_that_fits_is_accepted_unchanged() {
        let mut current = BindingSet::new();
        current.insert(binding("sip:alice@10.0.0.1:5060", 3600));

        // One refresh against a quota of 2: 1 active + 1 add of the *same* contact = still 2.
        let cmd = command(vec![op("sip:alice@10.0.0.1:5060", 3600)]);
        let outcome = process(&cmd, &current, &policy(2));

        assert!(
            matches!(outcome, Outcome::Commit { .. } | Outcome::Noop { .. }),
            "a refresh that does not grow the set must be accepted, got {outcome:?}"
        );
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod rg25_tests {
    use super::*;
    use crate::binding::{Binding, Timestamp};
    use crate::command::{ContactOp, ContactOps, RegisterCommand};
    use bytes::Bytes;
    use sipx_sip::Uri;

    fn aor(uri: &str) -> crate::CanonicalAor {
        crate::CanonicalAor::parse(uri.to_owned()).expect("a well-formed AoR")
    }

    /// A removal of `contact`. Removals are what these tests carry, deliberately — see
    /// `rg25_a_register_above_the_contact_bound_is_refused_before_reconciliation`.
    fn removal(contact: &str) -> ContactOp {
        ContactOp {
            uri: Uri::parse(Bytes::copy_from_slice(contact.as_bytes())).expect("a valid contact"),
            verbatim: Bytes::copy_from_slice(contact.as_bytes()),
            expires: Some(0),
            q: None,
            instance_id: None,
            reg_id: None,
            push: None,
        }
    }

    fn removals(count: usize) -> Vec<ContactOp> {
        (0..count)
            .map(|i| removal(&format!("sip:alice@198.51.100.9:{}", 5060 + i)))
            .collect()
    }

    fn command(contacts: Vec<ContactOp>) -> RegisterCommand {
        RegisterCommand {
            tenant: "t".to_owned(),
            aor: aor("sip:alice@example.test"),
            call_id: Bytes::from_static(b"rg25-call"),
            cseq: 1,
            contacts: ContactOps::Explicit(contacts),
            expires_header: None,
            path: Vec::new(),
            supports_path: false,
            require: Vec::new(),
            received: None,
            flow_ref: None,
            principal: None,
            now: Timestamp::from_secs(1_000),
        }
    }

    fn binding(contact: &str) -> Binding {
        Binding {
            contact: Bytes::copy_from_slice(contact.as_bytes()),
            q: 1_000,
            call_id: Bytes::from_static(b"other"),
            cseq: 1,
            expires_at: Timestamp::from_secs(3_600),
            registered_at: Timestamp::from_secs(1),
            refreshed_at: Timestamp::from_secs(1),
            path: Vec::new(),
            received: None,
            instance_id: None,
            reg_id: None,
            flow_ref: None,
            push: None,
            principal: None,
        }
    }

    /// **`RG-25`'s failing-first test.** A REGISTER carrying more contact operations than the bound
    /// is refused, and refused *before* reconciliation: not one stored contact is parsed.
    ///
    /// The operations are **removals**, deliberately. A removal never grows the set, so §5.5's quota
    /// cannot refuse this request however long it is — which is exactly why a bound on the *input*
    /// has to exist beside the bound on the *committed outcome*. Before this story the request below
    /// is accepted, answers `200`, and pays a full reconciliation on the way there.
    ///
    /// Counted rather than timed, for `RG-14`'s reason: wall-clock would be flaky and a parse is the
    /// expensive unit. Ten bindings are stocked so `parse_meter` has something to count at all; the
    /// class it is structurally blind to is
    /// `rg25_the_bound_is_counted_in_contact_operations_not_in_stored_parses`.
    #[test]
    fn rg25_a_register_above_the_contact_bound_is_refused_before_reconciliation() {
        let mut current = BindingSet::new();
        for i in 0..10 {
            current.insert(binding(&format!("sip:alice@10.0.0.{i}:5060")));
        }

        // 65 removals — one more than the default bound of 64.
        let cmd = command(removals(65));

        parse_meter::reset();
        let outcome = process(&cmd, &current, &TenantPolicy::default());
        let parses = parse_meter::count();

        assert!(
            matches!(outcome, Outcome::Reject(Rejection::Forbidden(_))),
            "a REGISTER above the contact-operation bound must be refused, got {outcome:?}"
        );
        assert_eq!(
            parses, 0,
            "the refusal precedes reconciliation, so no stored contact is parsed; got {parses}"
        );
    }

    /// The class `parse_meter` is **structurally blind to**, and the reason §5.5.1 needed a second
    /// instrument rather than a second assertion on the first.
    ///
    /// The address-of-record here holds nothing, so there are no stored contacts to parse and
    /// `parse_meter` reads `0` whether the request carries one operation or thousands. The cost that
    /// still exists is the request's own length, and `op_meter` is what can see it: an over-limit
    /// request examines **no** contact operation, and a conforming one examines exactly its own.
    #[test]
    fn rg25_the_bound_is_counted_in_contact_operations_not_in_stored_parses() {
        let empty = BindingSet::new();
        let policy = TenantPolicy::default();

        op_meter::reset();
        parse_meter::reset();
        let refused = process(
            &command(removals(policy.max_contact_ops + 1)),
            &empty,
            &policy,
        );
        let examined = op_meter::count();

        assert!(
            matches!(refused, Outcome::Reject(Rejection::Forbidden(_))),
            "one operation above the bound must be refused, got {refused:?}"
        );
        assert_eq!(
            examined, 0,
            "an over-limit request must examine no contact operation at all; got {examined}"
        );
        assert_eq!(
            parse_meter::count(),
            0,
            "and the old meter reads zero either way here — which is why this one exists"
        );

        // Q3 — the bound is inclusive, and a conforming request is unaffected: it examines exactly
        // the operations it carries, which is the linear cost the story is buying.
        op_meter::reset();
        let accepted = process(&command(removals(policy.max_contact_ops)), &empty, &policy);
        let examined = op_meter::count();

        assert_eq!(
            accepted.status(),
            200,
            "exactly the bound is accepted, not one fewer"
        );
        assert_eq!(
            examined, 64,
            "a conforming request examines exactly its own operations; got {examined}"
        );
    }
}
