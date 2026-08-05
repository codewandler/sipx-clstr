---
id: CX-15
title: Prepare and cut sipx-clstr 0.14.0 on sipx 1.0.0-beta.5
pillar: Platform
status: done
priority: 1
design:
epic:
areas: [release, build, docs]
note: released as v0.14.0 from one patch-free commit after the complete gate and same-kernel real-socket call passed
---

# Prepare and cut sipx-clstr 0.14.0 on sipx 1.0.0-beta.5

## Goal

Cut the work since `0.13.0` as one honest `0.14.0` release built against the immutable sipx
`1.0.0-beta.5` tag, with the source, chart, public documentation and release record naming the same
artifacts and no local checkout participating in the proof.

## Acceptance

- [x] Every sipx dependency moves together to the published `v1.0.0-beta.5` tag, `Cargo.lock`
      resolves that one kernel version, the temporary `[patch]` to `../sipx` is removed, and the
      binary plus `kernel_pin.rs` report `1.0.0-beta.5`.
- [x] The workspace package version, all internal dependency constraints, six lockfile package
      records, Helm `version`/`appVersion`, checked CLI banners and public current-release marker all
      name `0.14.0`; dated historical release records remain unchanged.
- [x] `CHANGELOG.md` and the public What's new page lead with the user-visible release: REGISTER
      authorization and complete responses, fallible store reads, loaded membership/key/shard-map
      validation, stronger configuration redaction, and assurance checks that now fail closed. They
      do not call blocked `CF-20` closed or claim an operator, applied runtime keys, owner delivery,
      outgoing CANCEL, exact TCP-only selection, or an independently implemented interop peer.
- [x] `AGENTS.md`, the roadmap and upstream ledger are re-read against the released kernel tag. Any
      beta.5 change that closes or moves a kernel row is recorded from source evidence; unrelated
      endpoint/application additions are not presented as platform capability.
- [x] The generated board, conformance report, README/site release claims and documented commands
      agree, and the full `scripts/gate.sh` plus the real-socket call pass with the sibling patch
      absent and a phone built from the same released kernel tag.
- [x] The release commit is the commit tagged by one annotated `v0.14.0` tag, and the GitHub release
      publishes that tag without rebuilding the source or deploying documentation from another
      revision. Commit, tag, push and publication require the user's explicit instruction.

## Progress

- 2026-08-05 — Opened for release preparation while sipx's `1.0.0-beta.5` candidate exists only in
  the sibling checkout. Preparatory release prose and version-surface auditing can proceed; the pin,
  lockfile, no-patch proof, final gate and immutable tag cannot be completed before beta.5 is
  published.
- 2026-08-05 — Prepared every repository-owned `0.14.0` version surface and the release notes,
  corrected current README/site capability claims, and regenerated the board. Documentation, all
  68 checker self-tests, static site/CLI claims, the real-socket counting fixture, Helm metadata and
  values checks, formatting and `git diff --check` are green. A fresh remote-tag query still returns
  no `v1.0.0-beta.5`, so the kernel pin, generated kernel lock records, full gate and released-phone
  call remain deliberately untouched.
- 2026-08-05 — Proved the published annotated tag (`cd28f17e`, peeled commit `0133a464`), moved all
  four manifest pins together, refreshed the lockfile without a sibling source, and removed the
  temporary patch. Cargo resolves one beta.5 kernel; `kernel_pin.rs` passes all four assertions and
  the binary reports `sipx-clstr 0.14.0 (sipx kernel 1.0.0-beta.5)`. Re-read the beta.4→beta.5
  source delta, `AGENTS.md`, the roadmap and every open ledger row: kernel `C-6` and `A-10` remain
  backlog; proxy CANCEL, exact cleartext listener selection, nonce uniqueness, the linear replay
  window and per-message overload logging remain absent. Beta.5's endpoint/application additions
  therefore do not become platform capability claims here.
- 2026-08-05 — The complete patch-free `scripts/gate.sh` is green: all workspace/all-feature tests
  and doctests, every optional-feature combination, Rust 1.91, provenance, 219/621 proved vectors,
  the exact-one socket fixture, docs, proof domains, and the site's CLI/version/release claims. The
  host's `/tmp` tmpfs made `ld.lld` bus-error while linking a doctest; rerunning the unmodified gate
  with `TMPDIR` on disk passed without a skipped check. Built the diagnostic phone from a detached
  worktree at the released beta.5 commit (`0133a464`), then passed `scripts/e2e-call.sh`: both phones
  registered, the call answered, Bob recorded 24,000 audio samples, the node owned exactly one UDP
  socket, and the transaction store drained to zero after the RFC 3261 absorption window. Only the
  explicitly user-authorized release operations remain.
- 2026-08-05 — The user explicitly authorized the release operations. This closure record and the
  complete `0.14.0` source are one release commit; the annotated `v0.14.0` tag, `main` push and
  GitHub publication all name that commit, and the release-triggered website workflow checks out
  the tag rather than rebuilding documentation from a later revision.

## Notes

- Considered for upstream: **no** — this is this repository's release orchestration and artifact
  consistency. Kernel capability and defect changes remain in sipx and are only consumed here from
  an immutable release.
- `CF-20` is implemented locally but remains `blocked` in this release record: workflow wiring and
  the local live proof are not evidence that a GitHub runner reached its socket assertion. The
  release push can provide that evidence, after which `CF-20` closes in a later commit rather than
  rewriting the already-tagged release.
