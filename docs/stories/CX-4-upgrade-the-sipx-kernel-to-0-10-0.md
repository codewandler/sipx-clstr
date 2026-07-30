---
id: CX-4
title: Upgrade the pinned sipx kernel from 0.7.0 to 0.10.0
pillar: Platform
status: done
epic:
areas: [build, transport]
note: three releases behind — mostly UA-side work, so this is hygiene rather than a blocker
---

# Upgrade the pinned sipx kernel from 0.7.0 to 0.10.0

## Goal

Move the workspace's pinned kernel forward three releases, so the platform is not orchestrating a
protocol core that has since been corrected, and so the next upgrade is one step rather than four.

## Acceptance

- [x] Every `sipx-*` dependency in the root `Cargo.toml` moves from `tag = "v0.7.0"` to
      `tag = "v0.10.0"` — **all of them together**. A workspace holding two kernel versions is a
      protocol core disagreeing with itself.
- [x] `sipx_clstr_node::KERNEL_VERSION` reports `0.10.0`, and `sipx-clstr --version` prints it.
- [x] The full gate is green: `scripts/gate.sh`, including `check-features.sh` and `check-msrv.sh`.
      The kernel may have raised its own MSRV; if the declared floor moves, that is part of this
      story and the README/docs claims move with it.
- [x] The end-to-end proof still passes against a `sipx` CLI **built from the same tag**:
      `scripts/e2e-call.sh`. Pinning the library and testing against a different client build would
      make the proof meaningless.
- [x] `docs/upstream.md` is re-read row by row, not assumed. Its own rule says so: "Re-read the
      kernel before trusting a row." Any row that these releases closed is updated; any row that
      reopened is filed.
- [x] The published site's version strings are regenerated from the built binary, not edited —
      `website/docs/getting-started.md` and `website/docs/reference/cli.md` both quote
      `sipx-clstr <v> (sipx kernel <v>)`.
- [x] Behavioural changes that reach this platform are named in the CHANGELOG entry, not just the
      version number.

## Progress

- **The bump needed no source change.** All four pins moved together to `v0.10.0`; the workspace
  reaches the kernel through constructors (`Target::new`, `TimerQueue::new`) rather than struct
  literals, so `Target`/`ConnectionKey` gaining a `path` field is source-compatible, and the one
  breaking `sipx-ua` change in the range (`Config`/`Registration` dropping `outbound`,
  `registrar::interpret`'s signature) is in surface this workspace does not use — it takes
  `challenge`, `auth` and `Algorithm`. `tests/sans_io.rs` still passes, so the
  `default-features = false` seam that keeps `tokio` out of the registrar survived.
- **Failing-first**: `kernel_pin.rs`'s `the_reported_kernel_is_the_release_this_workspace_moved_to`
  asserts the literal `0.10.0`, which the pre-existing manifest/constant consistency test cannot —
  that one passes just as well when both say `0.7.0`. A second test runs the binary and reads
  `--version`, which is where the two site pages' strings now come from.
- **The MSRV floor moved *down*, 1.94 → 1.91,** and the reason it exists moved with it. `v0.10.0`
  bounds `impl<K: Eq, I: Ord> Default for TimerQueue<K, I>`, which is exactly what the upstream
  ledger row asked for, so the kernel no longer forces 1.94 on its consumers. Re-derived by
  bisecting 1.88 → 1.94 on this tag: 1.88 and 1.90 fail, 1.91 and up pass. What holds it at 1.91 is
  now local — `sipx-clstr-proxy/src/from_registrar.rs:53`'s `Duration::from_hours`. `Cargo.toml`'s
  comment, the `Dockerfile` `ARG` and the ledger row all say so; the release build was verified on
  1.91 because the image pins that number.
- **`docs/upstream.md` re-read row by row against the kernel at `v0.10.0`**, each cited symbol
  opened. Two rows changed: the `TimerQueue` `Default` row to **landed in 0.10.0** (closed by the
  kernel without ever being filed), and the `Path`/`Service-Route` citation, corrected from
  `address.rs:264,271` to `271,280` — **wrong when filed, not stale**: that file is one blob
  (`14817570`) at `v0.4.0`, `v0.7.0` and `v0.10.0`, and 264 is `Path`'s doc comment while 271 is
  `Path`'s match arm, so the old pair described `Path` twice and `Service-Route` never. My first
  write-up called this a drift, which would have meant the kernel moved and the ledger fell behind;
  it did not, and the ledger's own rule is aimed at exactly the other case. `CX-5` and `RG-15` stay open, now proved rather than assumed:
  `challenge.rs` is one blob (`30e1d290`) at `v0.7.0`, `v0.8.0`, `v0.9.0`, `v0.10.0` and `main`.
- **The e2e proof ran against a `sipx` CLI built from `v0.10.0`** (`sipx 0.10.0`), which is also
  what CI's `e2e` job now builds — its `sed` extraction reads `v0.10.0` off the bumped manifest.
- Not done here: the CHANGELOG entry, which is the integrator's. The behavioural deltas that reach
  this platform were handed over for it.

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
