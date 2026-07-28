---
id: PX-1
title: Specify proxy behavior
pillar: Signalling
status: done
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
- [x] Spec covers request validation (§16.3) including `Max-Forwards` and RFC 5393 loop detection, routing-information preprocessing (§16.4), target determination, forwarding (§16.6), response processing (§16.7), CANCEL propagation with `487` generation, and Timer C.
- [x] Stateless mode (§16.11) is defined as a strict subset with explicit applicability rules.
- [x] Every normative rule carries a numbered vector or state-table row, executable as message-in / effects-out by the harness (CF-1).
- [x] House decisions are marked as `[sipx-clstr]` rules with rationale, citing RFCs only.
- [x] The Record-Route insertion rules are reviewed against AF-1's token byte budget.
- [x] Transaction-affinity behavior is specified: which messages must reach the edge holding 
the transaction (retransmissions, CANCEL, ACK to a non-2xx), and the degraded behavior when the 
dataplane delivers one elsewhere (stateless CANCEL forwarding, retransmission handling).

## Progress
- 2026-07-28 — integration: AF-1 fixed the layout; worst-case token parameter is 157 B
  ≤ 200 B (incl. the 64 B module-facts ceiling), so the F4 budget box is closed; F4 now
  cites affinity-token §3 as budget authority. Hook phase names (V7, R2, R11) and the
  empty-target-set 480 rule (PB-F-5) folded in from EX-1/RG-1 review. Story done.
- 2026-07-28 — spec drafted at `docs/specs/proxy-behavior.md`: sans-IO engine contract, mode
  applicability table, §16.3–§16.11 rule tables with [sipx-clstr] decisions, RFC 5393 branch
  cookie + Max-Breadth, transaction-affinity section, 37 numbered vectors (PB-V/P/F/R/C/S/A).
- Open: the F4 token byte budget (≤ 200 B) is provisional — the "reviewed against AF-1"
  acceptance box stays unchecked until AF-1 fixes the layout and this spec's F4 row is
  re-reviewed.
- Open: RFC 5393 branch-cookie computation flagged as an upstream candidate (spec §1) — CX-1
  raises it with the ledger rows.

## Notes
- Design: [proxy-engine](../designs/proxy-engine.md).
