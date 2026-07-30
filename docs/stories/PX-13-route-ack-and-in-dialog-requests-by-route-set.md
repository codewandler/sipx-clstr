---
id: PX-13
title: Route ACK and in-dialog requests by the Route set, not by an address-of-record lookup
pillar: Signalling
status: blocked
priority: 1
design: docs/designs/proxy-transaction-driver.md
epic: proxy-engine
areas: [proxy, node]
note: merged then reverted — the fix works but leaks transactions; CI e2e reports outstanding=3 that never drains
---

# Route ACK and in-dialog requests by the Route set, not by an address-of-record lookup

## Goal

Deliver `ACK` and in-dialog requests to the next hop the dialog names, rather than treating their
Request-URI as an address of record and asking the location service about it.

## Acceptance

- [x] `ACK` for a 2xx is forwarded using the `Route` set and the dialog's remote target, honouring the
      core's route preprocessing rather than bypassing it.
- [x] ACK handling is split by semantics: the kernel-generated downstream ACK for a non-2xx remains
      transaction-scoped; an upstream non-2xx ACK is absorbed by its server transaction; a 2xx ACK is
      a separately routed request and is never answered.
- [x] BYE, re-INVITE and every other in-dialog request use the core's selected next hop directly
      instead of a location lookup. The driver does not edit the message after the pure engine has
      applied Route preprocessing.
- [x] An unroutable in-dialog request settles as an explicit state-machine input — never a silent
      drop.
- [x] **Failing-first test:** a call whose callee's Contact is not a registered AoR — the ordinary
      case — receives its 2xx ACK and is torn down by a Route-set BYE that reaches the far end. The
      test asserts the ACK is sent exactly once, neither request performs an AoR lookup, and the BYE's
      2xx returns on the existing Via path. This fails on `86e6b10`, where the ACK is dropped.
- [x] `ET-7` owns correcting the synthetic probe after this real-node path lands; PX-13's socket test
      uses a protocol-correct test UA and cannot pass with AoR-shaped ACK/BYE shortcuts.
- [x] The two-node proofs continue to pass, and one of them uses a remote target that is not an AoR.
- [x] `scripts/gate.sh` is green.

## Progress
- **MERGED AND REVERTED, 2026-07-30.** Merged as `6651043` with the full gate green and the local
  two-node proof passing, then reverted because CI's `e2e` job failed on the merge. **The revert is
  not a judgement on the work** — the fix is correct and the defect it closes is real; it leaks
  transactions, and that has to be closed before it can land.
- **The failure, quoted.** `scripts/e2e-call.sh` — the proof behind the release's headline claim, run
  by CI and *not* part of `scripts/gate.sh`, which is why nothing local caught it:

  ```
  ✓ bob heard audio
  ✓ bob recorded 24000 samples
  ✓ the node holds one socket — signalling only, so the media was direct
  ── wait for the transaction store to drain (RFC 3261's 64·T1 absorption window)
  e2e-call: FAIL — the node still reports outstanding=3 after 50s — a leaked transaction
  ```

  **The call itself is entirely healthy** — audio flowed, media went direct, every call assertion
  passed. What fails is the drain check afterwards. `outstanding` climbs 6 → 11 → 16, falls 10 → 5 → 3,
  and then stops at 3 rather than reaching 0.
- **Attribution is certain, not inferred.** `e2e` passed on `f949336` (the commit immediately before
  this merge), on `2cb22dd` and on `86e6b10`; it fails on `5d19ff6`. One merge changed.
- **Where to look first.** This story made a 2xx `ACK` a separately routed request. The prime suspect
  is a server transaction that is created or retained for that `ACK` and never concluded — RFC 3261
  §17.2.1's `Accepted` state is what `TuEvent::Ack` is delivered from, and nothing in this diff
  concludes it. Three leaked per call is consistent with one per transaction across the INVITE's
  lifecycle. Note the gate cannot see this class at all: no gate step starts a node and watches
  `outstanding` drain, which is its own gap.
- **What survives the revert, all still valid** — do not re-derive it on the next attempt: the
  failing-first socket proof with its zero-hit trap sockets, the `P1` narrowing (reviewed in both
  directions), the three ACK semantics checked against the pinned kernel's own transaction code, the
  `BranchTransportError` settlement, and the independent review's `PASS` on everything except this,
  which it had no way to see. The branch `impl/PX-13` is preserved at `234ab7b`.

- **The two-node proof was run at integration and passes** on the merged tree, against the `sipx` CLI
  built from the pinned `v0.10.0` tag: two nodes on `127.0.0.1:5060` and `127.0.0.2:5060`, one
  PostgreSQL store holding two binding rows written by different nodes, `RESULT: PASS`. That satisfies
  the item as worded — "continue to pass", i.e. no regression — and **it is not evidence the fix
  works**: this same script reported `PASS` while the ACK was being dropped, which is exactly why this
  story added the header comment recording that. The fix's evidence is the socket test.
- **Independent review: `PASS`,** having re-run the failing-first proof at the true merge base
  `f949336` (the story cited `86e6b10`, an ancestor — the evidence stands, the citation did not) and
  verified the `P1` narrowing in both directions: it cannot hijack a Record-Route it does not own, and
  it still fires on every value this platform places, since those are always `;lr` with the token as a
  URI parameter and never a user part.
- **Two claims in this record were overstated and are corrected here.** `register_then_call.rs`'s
  `Path` scenario does **not** discriminate keying on `next_hop` from keying on `target.uri` — its path
  URI points at the same node as bob's first contact, so it passes either way. Nothing was loosened (a
  required `reachable` entry was added, without which the branch goes unreachable), but it is not the
  strengthening claimed. And `PB-F-4`'s edited row *is* more correct rather than more convenient — the
  strict router in the Request-URI is what §16.6 step 12 requires — which was worth confirming rather
  than asserting.
- **Known, not fixed, and now recorded where they will be found:** `F7`'s `lr` test is not total across
  `F6` — a route set of `[ours;lr, strict(no lr), p2;lr]` skips the strict router the swap exists to
  traverse, and the `F7` row overstates what the code does. And `P3`'s reject can no longer precede the
  forward for in-dialog requests, because `Input::Upstream` now returns `on_targets` synchronously —
  unreachable today (`TokenFact` has no producer), but `AF-4` must place token verification before the
  T1 branch or inside the engine.


**Done, except that the two-node proofs could not be *run* here** — they need Docker, PostgreSQL and
the external `sipx` CLI, none of which this environment has. What is settled about them: both already
use a remote target that is not an address of record, and nothing said so. Their phones bind
`127.0.0.1:15081`/`15091`, so a `Contact` is `sip:bob@127.0.0.1:15091`, and an explicit port is part of
a canonical AoR (location-service §3 N7) — a different key from the registered `sip:bob@127.0.0.1`. So
the ACK and the BYE there could only ever have been routed by the route set, and before this story they
were not routed at all. `scripts/two-node-call.sh`'s header now records that, and why the proof
reported PASS anyway: `sipx dial` had its `200 OK` and never checked that the call was concluded. A
run of either proof is the one piece of evidence still outstanding.

**The shape.** Target determination became a rule in the engine rather than a question the driver
answered by asking the registrar:

- `crates/sipx-clstr-proxy/src/route.rs` is new and holds what both entry points share — §5's P1/P2,
  §5.1's T1/T2 and F7's next hop. The `ACK` path and the response context call the *same* functions,
  which is what "honouring the core's route preprocessing" has to mean.
- `Effect::Forward` carries `next_hop` (spec §2, F7). It is not `target.uri`: the target is what goes
  into the Request-URI, the hop is where the copy goes, and they differ for every surviving `Route`
  and every registered `Path`. The simulated drivers were switched to it too, which is how
  `register_then_call.rs`'s `Path` test came to prove the path is *followed* and not merely carried.
- `crates/sipx-clstr-proxy/src/ack.rs` is the 2xx ACK: a pure function to a forwardable copy plus its
  hop, or an explicit unroutable outcome with a reason. It is not a `ResponseContext`, because an ACK
  has no transaction to answer and every refusal a context can reach is a status that must not be sent.
- **P1 had to be narrowed to what §16.4 actually says.** `is_ours` alone matches every URI at the
  edge's host, so on a loopback deployment P1 fired on ordinary mid-dialog requests, replaced the
  remote target with our own `Record-Route` and consumed the route set. Spec §5 now states the rule and
  `PB-P-7` pins it. Without this the ACK routes to the node's own advertised address instead.

**Where the evidence is.** `crates/sipx-clstr-node/tests/in_dialog_routing.rs` — three socket tests
with a protocol-correct test UA; the first fails at the merge base by delivering the ACK to a trap
socket registered under the contact's canonical AoR. Vector rows `PB-P-6`, `PB-P-7`, `PB-F-6`,
`PB-F-7`, `PB-F-8` in `crates/sipx-clstr-proxy/tests/vectors_proxy.rs`.

**Adjacent, deliberately not fixed.** An *out-of-dialog* request with a pre-existing route set still
asks the location service about the first `Route` and then overwrites its Request-URI with the answer
(`route::aor_query`, and `PB-F-4`'s vector encodes the resulting double swap). T1 covers the in-dialog
case the story names; the initial-INVITE case wants its own story and a rewritten `PB-F-4`.
`destination_of` also still assumes UDP and ignores `;transport=`, so an in-dialog request routed to a
TCP or TLS `Record-Route` goes out over UDP.

## Notes

- Validated synthesis finding [**V-03**](../reviews/00-validated-synthesis.md#v-03--ack-and-in-dialog-requests-are-routed-as-registrar-lookups); the protocol and assurance reviews found it independently.
- Evidence: `crates/sipx-clstr-node/src/driver.rs:800-807` routes every `Ack` through
  `forward_statelessly`; `:1189-1196` and `:1237-1257` resolve the Request-URI as an AoR, take the
  first registration, and drop the request when there is none. Correct preprocessing and next-hop
  selection already exist at `crates/sipx-clstr-proxy/src/context.rs:111-169`. The probe builds
  `ACK`/`BYE` to a configured AoR at `crates/sipx-clstr-probe/src/engine.rs:446-455`, `:635-658`.
- **Why the harness did not catch it:** the simulation's `ACK`/`BYE` are AoR-shaped, so they resolve
  by accident. A real remote Contact is not the registered AoR.
- **Upstream boundary:** generic URI resolution and ACK transaction semantics are sipx capabilities;
  choosing location lookup versus direct route/flow delivery and feeding the result to the local
  response context are cluster orchestration.
