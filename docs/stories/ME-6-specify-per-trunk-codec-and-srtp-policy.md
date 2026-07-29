---
id: ME-6
title: Specify per-trunk codec and SRTP policy
pillar: Media
status: in-progress
priority: 
design: docs/designs/media-control.md
epic: media-control
areas: [media, trunks]
note: 
---

# Specify per-trunk codec and SRTP policy

## Goal
Make codec handling and SRTP selection a declared per-trunk policy rather than a consequence of which branch of the routing logic a call took.

## Acceptance
- [x] A trunk declares its offered codecs, any transcoding, and its SRTP mode. → [media-relay](../specs/media-relay.md) §13.1: `TrunkMediaPolicy { codecs, transcode, srtp }`, with `CodecPolicy::{AsReceived, Restrict}`, `Transcode::{None, To}` and `SrtpPolicy::{Disabled, Sdes, DtlsSrtp}`; §13.3 maps each value onto the exact NG keys, and vectors MR-P-4…MR-P-10 assert the mapping value by value.
- [x] SRTP selection is per trunk, not a global domain pattern. → §13.2 MP1 makes the derivation `fn media_keys(policy, command)` — no request, URI, domain, source address, body, clock or randomness is in scope, so a pattern match is unrepresentable rather than merely forbidden (the technique §7.3 I3 already uses). MP6 states it; MP5 explains why per-leg independence is what makes per-trunk coherent; MR-P-1, MR-P-2, MR-P-3 and MR-P-14 assert it.
- [x] The default is explicit: no transcoding unless declared. → §13.2 MP2 (`Transcode::None` is the default *and* is emitted into §13.5's effective-policy record) and MP3 (`Restrict` filters; only `transcode` adds — a codec list is never a licence to transcode). Vectors MR-P-4, MR-P-5, MR-P-6, MR-P-7; byte-exact in MR-E-18, which carries `offer` and `strip` and no `transcode`.
- [ ] A test asserts the offer sent to a trunk matches its declared policy. → **Specified, not executed.** The assertion is MR-P-1 (§12.5), pinned byte-for-byte against MR-E-18, and it is the row the acceptance item names. It cannot run yet: the `MediaRelay` types are a spec contract and no Rust exists (`ME-2` lands the trait). Registry wiring is blocked the same way — see *Progress*.

## Progress
- **Spec written into [media-relay](../specs/media-relay.md) §13** — the section ME-1's own §7.4
  invited, taken up on the terms it set (amend §7.1, §7.2 and §12 together, so the bytes and the
  tests move with the rule). Twelve rules `MP1`–`MP12`, six startup checks `G-M1`–`G-M6`, the
  policy → NG-key mapping §13.3, and 24 new vectors: `MR-P-1…16`, `MR-C-1…8`, plus byte-exact
  `MR-E-18/19/20`. Amended in place: §1 (references), §2 (scope and the upstream line), §3.1
  (`OfferRequest.toward`), §7.1/§7.2 (`transport-protocol`, `DTLS`, `SDES`, `codec` and rule M4),
  §7.4 (rewritten — it was the hand-off point), §11 (`V5`), §12 intro, §14.
- **Zero Rust changed.** As with ME-1, §13.1's types are the contract `ME-2` implements.
- **The old §13 became §14.** Nothing in the repo cited §13; §1–§12 kept their numbers because
  ME-1's story and the design cite §3.2, §11 and §12.
- **Is the policy expressible under ME-1's opaque-SDP decision (O3)? Yes, and the constraint
  improved the design.** The policy never touches a body: it is expressed as NG control keys and
  the relay does the SDP surgery, so no SDP model is needed in either repository and ME-1's
  decision stands unchanged. What O3 *does* forbid is the negotiated forms — opportunistic SRTP,
  and a codec list conditioned on what the peer offered — because both must read the description
  that came back. MP9 rules them out of v1 and MP10 extends O3 to verification as well as
  decision: a test may parse SDP, the platform may not. That constraint and the story's goal point
  the same way, which is the argument for the declarative shape rather than a concession to it.
- **Upstream answer (AGENTS.md rule 6):** considered for upstream — **no**. Per-trunk codec and
  SRTP policy is orchestration by construction: its subject is a trunk, which the kernel does not
  have, and its whole output is control keys for a relay the kernel never speaks to. Recorded in
  the spec §2 and the design header. No new [upstream ledger](../upstream.md) row: the only
  protocol-generic thing nearby is the SDP model, and that is the row ME-4 already reserves.
- **The EX-7 seam, from this side (§13.6, MP11):** a carrier quirk profile may **require** an SRTP
  mode and may never **assign** one. If a profile could assign, SRTP selection would go back to
  being a consequence of which pattern matched — this story's defect, relocated — and profiles
  compose, which assignments cannot do without a precedence rule. Constraints intersect instead: a
  contradiction with the trunk's declaration is a startup error naming both (`G-M5`, MR-C-3), two
  profiles both requiring SRTP agree (MR-C-4), and a schema offering an assignment field is
  rejected (MR-C-5). `EX-7` keeps the vocabulary; §13.6 fixes only the direction. Nothing was
  written into `docs/designs/extension-framework.md`.
- **The safety point ME-2 must not lose:** a media-policy key the node does not recognise is
  *ignored*, and ignoring an SRTP key means clear-text media on a leg whose policy said encrypted —
  silent, because ignore-unknown-keys binds the far end too. So MP12 refuses to start on any policy
  beyond the identity `{ AsReceived, None, Disabled }` until `CF-3` has confirmed each key against
  the §11 baseline, §11 `V5` records why, and MR-C-8 discharges it by observing the **media**, not
  the reply.
- **Verification used while writing:** the three new `MR-E` blocks were machine-checked to frame as
  `cookie SP bencode`, decode under a strict decoder and re-encode byte-identically under §6.3's
  canonical encoder — and the same encoder reproduces ME-1's `MR-E-2` at exactly 376 bytes, so it
  is known to agree with the bytes already pinned. All fifteen blocks in §12.2 now pass that check
  (MR-E-11 excepted from the re-encode step, being ME-1's deliberately out-of-order decoder input).
- **Deferred, and tracked — do not re-file:** the `MR-P` and `MR-C` rows are **not** registered in
  `scripts/check-vectors.py`, and `docs/reference/vector-scope.toml` / `docs/reference/conformance.md`
  carry no `MR` rows. Those three files were fenced to another story during the implementation
  wave. [CF-8](https://github.com/codewandler/sipx-clstr/blob/main/docs/stories/CF-8-bring-every-spec-under-the-vector-gate.md)
  tracks the gap repo-wide and already names `MR`; these rows join it there. Until then the table
  is unenforced, which is exactly the condition that check exists to prevent.

## Notes
- One deployment transcodes to a specific codec on one branch of a NAT test and not the other — the policy is an accident of control flow. SRTP is selected by a global domain regex.
- Neither is reviewable, and neither can be changed per carrier without editing routing logic.
- Filed from a downstream deployment of this platform, whose capability inventory records this as `upstream` (its ledger entry **U-12**). The evidence sits in that deployment's own reference material.
