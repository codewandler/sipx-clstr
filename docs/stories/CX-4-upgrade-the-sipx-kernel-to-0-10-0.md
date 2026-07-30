---
id: CX-4
title: Upgrade the pinned sipx kernel from 0.7.0 to 0.10.0
pillar: Platform
status: ready
priority: 2
epic:
areas: [build, transport]
note: three releases behind — mostly UA-side work, so this is hygiene rather than a blocker
---

# Upgrade the pinned sipx kernel from 0.7.0 to 0.10.0

## Goal

Move the workspace's pinned kernel forward three releases, so the platform is not orchestrating a
protocol core that has since been corrected, and so the next upgrade is one step rather than four.

## Acceptance

- [ ] Every `sipx-*` dependency in the root `Cargo.toml` moves from `tag = "v0.7.0"` to
      `tag = "v0.10.0"` — **all of them together**. A workspace holding two kernel versions is a
      protocol core disagreeing with itself.
- [ ] `sipx_clstr_node::KERNEL_VERSION` reports `0.10.0`, and `sipx-clstr --version` prints it.
- [ ] The full gate is green: `scripts/gate.sh`, including `check-features.sh` and `check-msrv.sh`.
      The kernel may have raised its own MSRV; if the declared floor moves, that is part of this
      story and the README/docs claims move with it.
- [ ] The end-to-end proof still passes against a `sipx` CLI **built from the same tag**:
      `scripts/e2e-call.sh`. Pinning the library and testing against a different client build would
      make the proof meaningless.
- [ ] `docs/upstream.md` is re-read row by row, not assumed. Its own rule says so: "Re-read the
      kernel before trusting a row." Any row that these releases closed is updated; any row that
      reopened is filed.
- [ ] The published site's version strings are regenerated from the built binary, not edited —
      `website/docs/getting-started.md` and `website/docs/reference/cli.md` both quote
      `sipx-clstr <v> (sipx kernel <v>)`.
- [ ] Behavioural changes that reach this platform are named in the CHANGELOG entry, not just the
      version number.

## Progress

- (running log)

## Notes

- **This upgrade does not fix the nonce-uniqueness defect, and it was expected to.** `CX-5` checked:
  `crates/sipx-ua/src/challenge.rs` is byte-identical between `v0.7.0`, `v0.10.0` and kernel `main`, so
  the bump moves the pin and leaves the mint alone. Do not close `CX-5` on the strength of this story.


- 0.8.0 → 0.10.0 is predominantly user-agent, media and ICE work — SRTP keying, an ICE agent, RFC
  8839 SDP attributes, `sipx peers`, push (RFC 8599). Little of it is on this platform's path, which
  is why this is `priority: 2` and not `1`: it is unlikely to unblock clustering, and equally
  unlikely to stay cheap if left for another four releases.
- Two changes worth reading before assuming the upgrade is mechanical: the connection pool key is
  now generated from the type that defines it, and the kernel's gate became a program that checks
  itself against CI. Either could touch how this workspace consumes the transport layer.
- The tag is a tag and not a branch on purpose — reproducibility. Do not switch to a branch or a
  revision to make this easier.
- `AGENTS.md` non-negotiable #6 still applies: if the upgrade reveals protocol logic shadowed here
  that belongs upstream, file it upstream rather than fixing it locally.
