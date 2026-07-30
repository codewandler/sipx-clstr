---
id: PX-13
title: Route ACK and in-dialog requests by the Route set, not by an address-of-record lookup
pillar: Signalling
status: in-progress
priority: 1
design: docs/designs/proxy-transaction-driver.md
epic: proxy-engine
areas: [proxy, node]
note: release blocker — every ACK is resolved as an AoR and silently dropped when no binding exists
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
- [ ] The two-node proofs continue to pass, and one of them uses a remote target that is not an AoR.
- [x] `scripts/gate.sh` is green.

## Progress

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
