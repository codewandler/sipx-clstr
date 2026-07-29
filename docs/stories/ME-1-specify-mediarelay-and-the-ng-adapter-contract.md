---
id: ME-1
title: Specify MediaRelay and the NG adapter contract
pillar: Media
status: done
priority: 
design: docs/designs/media-control.md
epic: media-control
areas: [media]
note: 
---

# Specify MediaRelay and the NG adapter contract

## Goal
Specify the platform's only view of media — the `MediaRelay` trait — and the rtpengine NG integration contract behind it.

## Acceptance
- [x] Trait semantics for offer/answer/update/delete/query are specified, including `NullMediaRelay`'s pass-through behavior. → [media-relay](../specs/media-relay.md) §3.2 (rules O1–O6, A1–A3, U1–U4, D1–D5, Q1–Q4), the session state table §3.3 (S1–S12), and §4 (N1–N6) for the null relay; vectors MR-T-1…13 and MR-N-1…5.
- [x] The NG mapping is pinned: command set, bencode framing, cookie correlation, timeout/retransmission budget, error taxonomy (node down vs command rejected), health signals. → §7.1 (command set), §6.1 (framing), §6.2 (cookie, C1–C4), §6.3 (canonical bencode, E1–E5/D6–D11), §8 (timers and the schedule, K1–K6), §9 (taxonomy, X1–X4), §10 (node states H1–H8, signals P1–P6); vectors MR-E-1…17, MR-X-1…10, MR-F-1…8, MR-H-1…10.
- [x] A tested rtpengine baseline version is named per the AGENTS.md integration carve-out. → §11: `mr13.0.1.10` (series `mr13.0`), the same series `deploy/helm/values.yaml` pins, with V1–V4 fixing what "baseline" means and how drift is handled.

## Progress
- **Spec written:** [docs/specs/media-relay.md](../specs/media-relay.md) — normative, vector prefix `MR`, 63 vectors of which twelve are byte-exact NG datagrams. Registered in `website/sidebars.js` under Specifications.
- **Zero Rust changed.** The `MediaRelay` types in §3.1 are the contract `ME-2` implements; nothing is compiled yet.
- **Upstream answer (AGENTS.md rule 6):** considered for upstream — **no**. Recorded in the spec §2 and in the design's header. The reasoning turns on §3.2 O3: SDP stays opaque bytes end to end, so no SDP model is needed in either repository. If `ME-4` must read the body, that parser is protocol-generic and becomes an `upstream.md` row then.
- **Deferred to a follow-up — do not lose this:** the `MR` prefix is **not** registered in `scripts/check-vectors.py`, and `docs/reference/vector-scope.toml` / `docs/reference/conformance.md` carry no `MR` rows. Those three files are owned by another story in this wave, so wiring the `MR` vectors into the conformance registry (adding `"MR"` to `SPECS`, the six families `MR-T/N/E/X/F/H` to `FAMILIES`, and deferral entries naming `ME-2`) is a follow-up. Until it happens the vector table is unenforced — a spec table nobody executes is prose, which is exactly what that check exists to prevent.
- **Verification used while writing:** every `MR-E` block was machine-checked to frame as `cookie SP bencode`, decode under a strict decoder, and re-encode byte-identically under the §6.3 canonical encoder. `ME-2` should turn that check into a test rather than re-typing the bytes.

## Notes
- Design: [media-control](../designs/media-control.md). Spec: [media-relay](../specs/media-relay.md).
- Consumers of this contract: `ME-2` (adapter), `ME-3` (selection/reselection — §10 P1/P2 and state row S12 are its inputs), `ME-4`/`ME-5` (the anchoring module — §7.3's ICE stance and §9's status mapping are its decisions to make), `KO-7` (pool operation — §10 P3 is the readiness signal), `CF-3` (the interop container at the §11 baseline).
