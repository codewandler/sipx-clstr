---
id: PX-16
title: An out-of-dialog request with a pre-existing route set must not lose the callee's URI
pillar: Signalling
status: ready
priority: 2
design: docs/designs/proxy-engine.md
epic: proxy-engine
areas: [proxy]
note: found by PX-13, which fixed the in-dialog half; F2 overwrites the Request-URI with a location answer about the first Route
---

# An out-of-dialog request with a pre-existing route set must not lose the callee's URI

## Goal
Forward an out-of-dialog request that already carries a `Route` set to the route it names, without
replacing the Request-URI — the callee's own URI — with the answer to a question about a proxy.

## Acceptance
- [ ] An out-of-dialog request carrying a pre-existing `Route` set is forwarded to the first `Route`,
      and its Request-URI still names the callee on the wire.
- [ ] The location service is not asked about a `Route` URI. A `Route` names a proxy; an address of
      record names a user, and asking the former as the latter is the category error this closes.
- [ ] RFC 3261 §16.6 step 12's strict-router swap still happens where it is required, and `F7`'s `lr`
      test is made total across `F6` rather than assumed — a route set of
      `[ours;lr, strict(no lr), p2;lr]` currently skips the strict router the swap exists to traverse.
- [ ] **Failing-first vector.** `PB-F-4` encodes the current double swap as expected bytes, so this
      story changes a vector's expectation: state in the row and in the story why the new bytes are
      correct and the old ones were not, per the discipline `CF-12` established.
- [ ] `scripts/gate.sh` green, and the two-node proofs still pass.

## Progress
- (not started)

## Notes
- Found by `PX-13`, which fixed the **in-dialog** half of this (`T1`) and deliberately left the
  out-of-dialog half alone because closing it means rewriting a landed vector's expectation, which
  wants its own story and its own argument.
- Mechanism: `route::aor_query` asks the location service about the first `Route`, and `F2` then writes
  the resolved target into the Request-URI — so the real destination is lost. `PB-F-4`'s comment, *"the
  original Request-URI moved to the end"*, describes the **resolved** target rather than the original,
  which is how the shape survived review.
- Reachability: this is the path a request from an **upstream proxy** takes. It is not exercised by the
  two-node proofs, which originate at a UA with no pre-existing route set — so nothing today would
  surface it.
- The `F7` item is a second finding from the same review: the `lr` test tells you the swap has already
  happened when the first `Route` lacks `lr`, but the converse the rule leans on does not hold, so a
  strict router in the middle of a route set is skipped. Both belong here because both are `route.rs`'s
  handling of a route set the request arrived with.
- Considered for upstream: **no.** This is `proxy-behavior`'s route preprocessing, which is this
  repository's state machine; the kernel owns header surgery and transaction semantics, neither of
  which changes here.
