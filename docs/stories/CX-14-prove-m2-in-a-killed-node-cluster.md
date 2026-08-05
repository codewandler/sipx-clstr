---
id: CX-14
title: Prove M2 end to end, including the node kill
pillar: Platform
status: backlog
design:
epic:
areas: [e2e, deploy]
note: M2 #21 · the milestone's own proof — cross-edge call with relayed media, mid-dialog routed by token with the counter at zero, and a node killed without taking the platform's next call with it
---

# Prove M2 end to end, including the node kill

## Goal

Do for M2 what `CX-3` did for M1: run the milestone's *done means* as one scripted, repeatable
proof against real processes, so the claim is a command someone else can run rather than a
paragraph. M2's claim is bigger than M1's, and so is the proof: two sipx CLI phones registered
on **different edges** call each other **with media through the relay**; a mid-dialog request is
routed by its token, asserted by `DP-3`'s cross-node dialog-lookup counter reading **zero**; and
killing any single node leaves new registrations and new calls working.

## Acceptance

- [ ] A scripted run against the `KO-2` k3s deployment (or the `DP-2` topology where zone spread
      matters): install, register two phones on different edges, complete a call with audio
      proved through the relay — not direct — and hang up.
- [ ] During that call, a mid-dialog request (re-INVITE or BYE) arrives at an edge that did not
      create the dialog and is routed by its token: the `DP-3` invariant counter is scraped
      before and after and reads zero both times. The assertion is in the script, not the
      write-up.
- [ ] Kill one node (any role the run selects, edge included). A new registration and a new
      cross-edge call complete within the probe's timer budget; the probe deployed by the same
      chart reports `pass` after the kill.
- [ ] The run is repeatable from a clean cluster with one command, and CI runs it or the recorded
      reason it cannot is a checked reason (`CF-15`'s rule).
- [ ] What the run does **not** prove is stated in its output the way `KO-2`'s README states it:
      single-host k3s proves no zone spread and no store HA.

## Progress

- Filed with the M2 commitment (2026-08-05, goal set: M4 done). Blocked by the M2 story set —
  in practice the tail of it: `ME-5` (relayed media), `DP-3` (the counter), `KO-2` (the
  deployment), with `AF-5`/`AF-7` underneath.

## Notes

- `DP-9` proved a two-node call in devspace with audio, twice — this story is its M2-shaped
  successor, with the relay, the token assertion and the kill added. Reuse its scaffolding
  before writing new.
- The phones are sipx CLI phones built from the pinned kernel tag, per `CX-3`'s rule: pinning
  the library and testing against a different client build would make the proof meaningless.
