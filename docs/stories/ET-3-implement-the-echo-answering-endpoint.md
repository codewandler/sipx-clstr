---
id: ET-3
title: Implement the echo answering endpoint
pillar: Platform
status: done
priority:
design: docs/designs/e2e-tester.md
epic: e2e-tester
areas: [probe]
note: M1 #13 · the echo endpoint; end to end through the real proxy, registrar and probe
---

# Implement the echo answering endpoint

## Goal
Provide the other end of the probe call: an endpoint that registers in the test tenant, answers probe INVITEs, and reflects the correlation marker — so a probe run traverses the real path end to end.

## Acceptance
- [x] The echo endpoint registers as an ordinary AoR in the test tenant and answers INVITEs with 200 OK, reflecting the probe's correlation marker.
- [x] A failing-first harness test: probe → edge → location lookup → echo → 200 with marker → BYE, asserted end to end on a seed.
- [x] No proxy role links a UAS: the echo runs as its own role/mode per ET-1's decision, and a build check or module-graph assertion proves the separation.
- [x] No RTP is forwarded in-process; the media assertion path is left as a documented extension point over `MediaRelay`.
- [x] Malformed or unauthenticated calls to the echo are rejected the way any UAS would reject them — the test tenant is not a bypass.

## Progress
`sipx-clstr-probe::echo`, and **the first scenario in which every component is the real one**:
`ProbeEngine`, the registrar's store and REGISTER processing, the proxy's forwarding core, and the
echo. The only things the test supplies are a driver and a network — which is exactly the seam the
sans-IO design exists to create, and the closest thing to M1's exit criterion that runs without
sockets.

**The separation is structural, not promised.** §9's constraint is that a process running `echo` runs
no proxy role. `tests/role_separation.rs` asserts against the manifest that this crate depends on
neither the proxy, the registrar, the node crate, nor `tokio`. A comment saying "do not add the proxy
here" would be a comment; a test that reads `Cargo.toml` is a check that fails when someone adds the
forwarding core to reuse "just one helper".

### The defect the end-to-end scenario found

**The probe's requests carried no `Via` at all.** RFC 3261 §8.1.1.7 makes `Via` how a response finds
its way back, so a compliant proxy — ours — refused the INVITE as unanswerable, and the probe then
reported `Fail { Invite, Timeout }`: a platform failure it had caused itself. Every earlier probe test
passed because the peer answering them was a stub that did not care.

That is the case for building the end-to-end scenario at all. 24 unit vectors and 5 harness scenarios
all agreed the engine was right; the first component that actually *validated* a request disagreed
within one run. The branch is now derived from the marker and the sequence number — unique per
transaction, which is what RFC 3261 asks, and reproducible, which is what the harness asks.

### Decisions worth recording

- **`403`, not `404`, for an unmarked call.** The address-of-record exists and is registered; the call
  is refused on policy. `404` would tell a caller to look elsewhere for something that is right here.
- **The echo answers `OPTIONS` and refuses unknown methods with `405`, rather than ignoring either.**
  Silence would make the echo look like a dead listener — which is precisely the condition the probe
  exists to detect, so the echo must not counterfeit it.
- **It refreshes at half the granted lifetime**, so one lost REGISTER does not lapse the binding and
  leave the echo unreachable while looking healthy.
- **No SDP answer** (E4). A signalling-only echo that advertised RTP ports would be lying to the
  offerer, which then waits for audio that never arrives. `MediaPolicy` has exactly one value today,
  so the extension point is visible in the code rather than only in a document, and adding a
  relay-mediated variant is a change to that enum instead of to the answering path.
- **It holds no per-call state** (E3), which is what makes it safe for a probe to abandon a dialog on
  a failure path without leaving the echo wedged. The only state is two counters, and only because
  `ET-5` will want to publish them.

### An assumption I made about the kernel, corrected

`ResponseBuilder::to_request` builds a response even for a request with no `Via`, `Call-ID` or `CSeq`.
I had assumed it would refuse, and relied on that for the "unanswerable" path. The kernel is right to
be permissive — a UAS may know something the builder does not — so deciding *whether* to answer is
the application's, and the echo now checks respondability itself, as the proxy already did.

## Notes
- Design: [e2e-tester](../designs/e2e-tester.md). Spec: [e2e-probe](../specs/e2e-probe.md) §9.
- The refusal test asserts the echo *distinguishing* rather than merely being closed: in the same
  scenario it answers the probe's legitimate call and refuses the stranger's.
- The test tenant is not a bypass, and the division is the spec's: the platform decides
  **reachability** (no trunks, no cross-tenant lookup), the echo decides **whether this is a probe**.
