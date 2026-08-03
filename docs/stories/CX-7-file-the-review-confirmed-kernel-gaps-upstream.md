---
id: CX-7
title: File the review-confirmed kernel gaps upstream
pillar: Platform
status: done
priority:
design:
epic:
areas: [upstream, proxy, transport, registrar]
note: UPSTREAM — V-02 and V-07 need released kernel surfaces; V-13 already has typed parsing
---

# File the review-confirmed kernel gaps upstream

## Goal

Turn the protocol-generic gaps exposed by the independent review of `86e6b10` into sipx stories,
and record the one suspected gap that disappeared when the pinned kernel was read. Local stories may
orchestrate these primitives; they may not construct a second transaction or transport layer.

## Acceptance

- [x] File a sipx story for a proxy-usable outgoing-CANCEL operation. It preserves the INVITE's
      branch and RFC 3261 §9.1 fields, observes the provisional-response precondition, uses the
      original target, and returns a transaction/delivery result the proxy driver can associate
      with its branch. Link the story from `docs/upstream.md`; `PX-12` remains blocked until a
      tagged sipx release carries it.
- [x] File a sipx story for exact cleartext listener selection. A consumer can request UDP-only,
      TCP-only, or both without an undeclared socket being bound; the existing same-port behavior
      remains available when both are selected. Link the story from the ledger. `FC-1` may fail
      closed on TCP-only configuration before that release, but may not claim TCP-only service
      until the kernel can provide it.
- [x] Each filed story carries a minimal failing test against sipx `v0.10.0`: proxy cancellation
      cannot be expressed through the public endpoint handle, and `Config`/`bind` necessarily
      creates a UDP socket even when only TCP is wanted.
- [x] Re-read V-13 against the pinned tag and retain the decision in the ledger/story: typed
      `sipx_sip::headers::Expires` already distinguishes malformed-present from absent. The local
      registrar must consume it and reject malformed Contact parameters and Path values atomically;
      no new kernel story is filed without a concrete missing generic primitive.
- [x] Update the dependent story notes once upstream IDs exist, preserving the ledger states
      `open`, `filed`, `implemented upstream, unreleased`, and `landed` rather than treating kernel
      `main` as consumable.
- [x] `scripts/gate.sh` is green.

## Progress

- Filed sipx [`T-28`](https://github.com/codewandler/sipx/blob/09d5518dc587dd77db61abd220ad309e00eda688/docs/stories/T-28-cancel-an-outgoing-invite-transaction.md)
  for proxy-usable outgoing CANCEL and
  [`T-29`](https://github.com/codewandler/sipx/blob/09d5518dc587dd77db61abd220ad309e00eda688/docs/stories/T-29-bind-only-the-selected-cleartext-transports.md)
  for exact cleartext listener selection. Each story names the minimal `v0.10.0` failure and the
  behavioral loopback test the implementation must make pass.
- Re-read V-13 against the pinned tag and retained the negative finding in the ledger: fallible
  typed `Expires` already exists, so `RG-20` owns the consumer fix and no parser story was filed.
- Updated `PX-12`, `FC-1` and `RG-20` with the exact dependency decision. The two real gaps are
  `filed; not in the pinned release`; neither active kernel `main` nor the separate dirty checkout
  was treated as a consumable dependency.
- `scripts/gate.sh` is green. The kernel filing lives in the isolated `sipx-CX-7` worktree so the
  active `C-1`/`T-22` changes in `../sipx` remain untouched.
- Published as [sipx PR #2](https://github.com/codewandler/sipx/pull/2) from a branch based directly
  on public kernel `main`; the story links above pin its filing commit, so the release does not
  depend on an uncommitted worktree or publish unrelated local kernel work.

## Notes

- Findings: [validated synthesis](../reviews/00-validated-synthesis.md) V-02, V-07 and V-13.
- Kernel evidence at the pinned `v0.10.0`: `sipx-transport/src/endpoint.rs:27-126` has one cleartext
  `bind` plus a `tcp: bool`, and `bind_matching_ports` binds UDP first at `:894-919`; the public
  `Handle` has no branch-cancellation operation while `sipx-call/src/call.rs:3442` keeps its CANCEL
  builder private. The transaction association primitive already exists as
  `TransactionKey::for_cancelled_invite`.
- V-13's `Expires` surface already exists at `sipx-sip/src/headers/misc.rs:85-99`. Treating a decode
  error as absence here is a consumer defect, not evidence for another parser.
- Considered for upstream: **yes.** CANCEL construction/transaction control, transport listener
  capability, and typed header parsing are protocol-generic. Two capabilities are missing and are
  filed here; the third exists and is consumed locally by `RG-20`.
