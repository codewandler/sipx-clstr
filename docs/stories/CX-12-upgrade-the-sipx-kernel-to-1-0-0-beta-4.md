---
id: CX-12
title: Upgrade the pinned sipx kernel from 0.10.0 to 1.0.0-beta.4
pillar: Platform
status: in-progress
design:
epic:
areas: [build, transport]
note: the kernel crossed into its 1.0 beta line — 313 commits, twelve releases, two source changes here; the tag is already on GitHub, and a temporary [patch] to ../sipx rides along until the user's push decision removes it
---

# Upgrade the pinned sipx kernel from 0.10.0 to 1.0.0-beta.4

## Goal

Move the workspace's pinned kernel from `v0.10.0` to `v1.0.0-beta.4` — the kernel's first 1.0
beta line — so the platform orchestrates the protocol core the kernel project actually maintains,
and so the M4 released-capability rows in [docs/upstream.md](../upstream.md) can be re-read
against a tag instead of against promises.

## Acceptance

- [ ] Every `sipx-*` dependency in the root `Cargo.toml` moves from `tag = "v0.10.0"` to
      `tag = "v1.0.0-beta.4"` — all of them together, as CX-4's rule says: a workspace holding
      two kernel versions is a protocol core disagreeing with itself.
- [ ] `sipx_clstr_node::KERNEL_VERSION` reports `1.0.0-beta.4`, proved failing-first by
      `kernel_pin.rs`, and `sipx-clstr --version` prints it.
- [ ] The full gate is green: `scripts/gate.sh`, including `check-features.sh` and
      `check-msrv.sh`. The MSRV floor is **re-derived on the new tag, not carried forward**
      (CX-4's rule; the current 1.91 is held by local code, but the kernel may have moved past it).
- [ ] `docs/upstream.md` is re-read row by row against `v1.0.0-beta.4`, per its own rule.
      Already known: `challenge.rs` is still blob `30e1d290` at beta.4, so `CX-5` and `RG-15`
      stay open. The upstream story numbers this ledger cites as `T-28`/`T-29` now name
      *different* stories in the kernel repo — the ledger must chase where the outgoing-CANCEL
      and exact-listener-selection asks actually went, because `PX-12` and `FC-1` are blocked on
      exactly those rows.
- [ ] The published site's version strings are regenerated from the built binary, not edited.
- [ ] Behavioural changes that reach this platform are named in the CHANGELOG entry.
- [ ] The temporary `[patch]` to the local `../sipx` checkout is either removed before this
      story closes, or its removal is a named, tracked follow-up with the reason it must stay.

## Progress

- Story opened mid-migration by the session doing the work. The tag `v1.0.0-beta.4` is verified
  present on the GitHub remote (`git ls-remote`), so the pin itself is honest; the `[patch]` to
  `../sipx` exists because the user asked to consume the local checkout until the next push, and
  the ledger's no-patch rule is knowingly, temporarily overridden by that instruction — recorded
  here so it cannot silently become permanent.
- **The bump needed two source changes across 313 kernel commits.** (1) `TransportKind` grew a
  `Quic` variant; `listen.rs`'s `transport_param` gained the arm, unreachable by construction
  because `Listener::new` already refuses everything but UDP/TCP/TLS — the fail-closed gate was
  already in place. (2) The kernel's `ResponseBuilder::to_request` now refuses a request missing
  `Via`/`From`/`To`/`Call-ID`/`CSeq` (beta.4 `X-64`), and that surfaced a **real defect here**:
  `EchoEndpoint::register_request` built its REGISTER with **no `Via` at all** — RFC 3261
  §8.1.1.7 — so its registrar's `200` was constructed against a request it could never have
  answered on the wire. The probe engine had learned exactly this lesson (`engine.rs`'s `via`
  doc); the echo never did. Fixed failing-first:
  `echo::tests::the_registration_carries_a_via_so_the_registrar_can_answer_it`, with `sent_by`
  entering `EchoConfig` the same way it enters `ProbeConfig`.
- `kernel_pin.rs` failed first exactly as designed — its manifest-vs-constant test read the new
  tag out of `Cargo.toml` and caught `KERNEL_VERSION` still saying `0.10.0`; the literal-release
  test now names `1.0.0-beta.4` per its own "edited by whoever moves the pin" contract.
- **The gate is green end to end**, including `check-msrv.sh` on the declared floor: the
  workspace (kernel included) builds on **1.91**, and what holds the floor is unchanged — local
  `Duration::from_hours`, not the kernel, whose own floor rose only to 1.88. Site version
  banners regenerated from `target/debug/sipx-clstr --version`, and the getting-started clone
  command now names the new tag.
- The ledger re-read ran against the tag, not release notes — findings recorded in
  `docs/upstream.md`: `challenge.rs` is **still** blob `30e1d290` at beta.4 (CX-5, RG-15 stay
  open), the shed logging is unchanged in substance with the atomics moved behind
  `meters.shed` (DP-11 stays open), and **the CX-7 filings were never merged upstream** — the
  filing commit sits on the unmerged branch `filing/clstr-CX-7-public` and kernel `main`
  recycled the IDs `T-28`/`T-29` for unrelated stories, so the outgoing-CANCEL and
  exact-listener-selection asks are effectively unfiled again. `CX-13` now owns the re-filing.
- **CI on `main` is red for as long as the `[patch]` stays, and that is the authorized cost, not
  a defect.** The runner has no sibling checkout, so `cargo` cannot resolve
  `../sipx/crates/sipx-sdp` and the `clippy`, `msrv` and `postgres` jobs all fail at dependency
  resolution before running anything. Only `fmt` — which resolves nothing — still reports on the
  code itself. Anyone reading red CI in this window should check the cause is that resolution
  error before believing a real regression: the tag `v1.0.0-beta.4` is on the GitHub remote, so
  deleting the `[patch]` section is the whole fix whenever the decision is made.
- Not done here: the CHANGELOG entry (comes with close), the `[patch]` removal (waits on the
  user's push decision), and the real-phone e2e proof against a `sipx` CLI built from the same
  tag — the story stays `in-progress` until those settle.

## Notes

- The kernel's crate layout did **not** move in this range — `crates/*` since the kernel's first
  scaffold — so ledger citations of the form `sipx-ua/src/…` were written short-handed, not made
  stale by this bump. Re-cited with the `crates/` prefix where touched.
- `sipx-sdp` is declared in `workspace.dependencies` but appears in no member's dependency list
  and is absent from `Cargo.lock` — decide whether to keep or drop the line while touching it.
- AGENTS.md non-negotiable #6 applies unchanged: anything the upgrade reveals as shadowed
  protocol logic files upstream, not here.
