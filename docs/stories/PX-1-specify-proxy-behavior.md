---
id: PX-1
title: Specify proxy behavior
pillar: Signalling
status: ready
priority: 1
design: docs/designs/proxy-engine.md
epic: proxy-engine
areas: [proxy]
note: gates PX-2 … PX-7
---

# Specify proxy behavior

## Goal
Write `docs/specs/proxy-behavior.md`: the normative contract for the proxy engine — RFC 3261 §16 amended by RFC 5393 — in both stateless and transaction-stateful modes, with test vectors the deterministic harness will execute.

## Acceptance
- [ ] Spec covers request validation (§16.3) including `Max-Forwards` and RFC 5393 loop detection, routing-information preprocessing (§16.4), target determination, forwarding (§16.6), response processing (§16.7), CANCEL propagation with `487` generation, and Timer C.
- [ ] Stateless mode (§16.11) is defined as a strict subset with explicit applicability rules.
- [ ] Every normative rule carries a numbered vector or state-table row, executable as message-in / effects-out by the harness (CF-1).
- [ ] House decisions are marked as `[sipx-clstr]` rules with rationale, citing RFCs only.
- [ ] The Record-Route insertion rules are reviewed against AF-1's token byte budget.

## Progress
- (not started)

## Notes
- Design: [proxy-engine](../designs/proxy-engine.md).
