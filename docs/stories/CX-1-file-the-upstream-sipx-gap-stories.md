---
id: CX-1
title: File the upstream sipx gap stories
pillar: Platform
status: done
priority:
design: 
epic: 
areas: [build]
note: UPSTREAM — touches the sipx repo
---

# File the upstream sipx gap stories

## Goal
Turn the open rows of the [upstream ledger](../upstream.md) into filed stories in the sipx repository, per sipx's own conventions, and cross-link both directions.

## Acceptance
- [x] Each open ledger row has a sipx story: Headers surgery API, Path/Service-Route typed headers, server-side digest primitives, the async/cached resolver option, and the sipx-testkit harness split.
- [x] Ledger rows are updated to `filed` with links; the notes of blocked sipx-clstr stories point at the filed sipx stories.
- [x] Anything sipx declines to take is recorded in the ledger with the local plan instead of silently disappearing.

## Progress
- Six stories filed in sipx, using its prefixes (`S` core · `T` transport · `X` cross-cutting) and
  its frontmatter, each naming a **failing-first test**:
  - `S-15` — editing operations on `Headers` (`remove_first`, `insert`, `retain`).
  - `S-16` — the server side of digest: nonce minting, challenge emission, verification, replay
    window.
  - `T-16` — the `Service-Route` header (RFC 3608).
  - `T-17` — resolution at proxy throughput: async, shared cache, WS/WSS SRV prefixes.
  - `X-14` — generalize `TimerQueue` and ship the loopback link the testkit's own crate doc
    promises.
  - `X-15` — the requirement-grain registry, filed **as an offer**: declining it is a valid close,
    and the ledger already records the local plan either way.
- `Path` needed no new story: sipx already carries `T-14` for it, `ready` at priority 4. The ledger
  row now points at `T-14` for Path and `T-16` for Service-Route.
- Every story earns its place in the kernel on its own, not only as a downstream ask: `S-15` has an
  in-kernel consumer today (`sipx-transport/src/nat.rs:149` rebuilds the whole header collection to
  replace one `Via`), and `X-14`'s loopback link is what lets the kernel test retransmission under
  loss without a socket.

**Two ledger rows were wrong, and re-reading the kernel is what found it:**

- **TLS/WS/WSS were recorded as unreleased M5 work.** They shipped in sipx **0.2.0** — `T-6`
  through `T-10` are all `done`, the CHANGELOG documents the certificate policy and the WebSocket
  framing, and `tls`/`ws`/`wss` are default features of `sipx-transport`. The row is now **landed
  in 0.2.0** and nothing was filed. `DP-2`'s TLS edges and the M3 WSS clients are not waiting on a
  kernel release.
- **The resolver row assumed its own answer.** Whether an async shared-cache resolver belongs
  upstream is genuinely undecided — the cache and the SRV prefix table are protocol-generic, the
  scheduling policy around them may not be. `T-17` is written to *settle* that rather than to
  assume it, per AGENTS.md rule 6 ("record the answer either way").
- A rule was added to the ledger: a row can be wrong, so re-read the kernel before believing one.

## Notes
- Coordinate with the sipx backlog before filing — its board has its own priorities.
- sipx has uncommitted work in progress (`S-11`, session timers) from another session; nothing here
  touched it, and the board there was regenerated from frontmatter only.
