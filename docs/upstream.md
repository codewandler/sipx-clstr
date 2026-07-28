# Upstream dependency ledger — what sipx-clstr needs from sipx

sipx-clstr builds on the [sipx](https://github.com/codewandler/sipx) kernel and, per the working agreement
([AGENTS.md](https://github.com/codewandler/sipx-clstr/blob/main/AGENTS.md) → *Upstream first*), never shadow-implements protocol logic that
belongs there. This ledger tracks every kernel gap this platform depends on: what it is, which
sipx story covers it (filed by `CX-1`), and which sipx-clstr stories it blocks. Two repos sharing
a boundary is this project's biggest coordination risk; this table keeps it auditable.

**Decided (not upstream):** the proxy transaction driver — one server transaction fanning out to
N client transactions — is built **here**, directly over `sipx-sip`'s sans-IO `TransactionLayer`.
The sipx transport driver stays UA-shaped; only generic primitives move upstream.

| Gap | What sipx has today | sipx story | Blocks here | Status |
|---|---|---|---|---|
| Header surgery API | `Headers` exposes `push`/`push_front`/`remove_all` but no `remove_first`, `insert_at` or `retain`; sipx-transport privately rebuilds the collection to rewrite the top Via (`nat.rs:149`) | [S-15](https://github.com/codewandler/sipx/blob/main/docs/stories/S-15-header-editing-operations.md) | PX-3, PX-4 | filed |
| `Path` and `Service-Route` headers | Not in the `HeaderName` enum; fall through to `Other`. Typed accessors and compact handling missing | [T-14](https://github.com/codewandler/sipx/blob/main/docs/stories/T-14-register-a-path-header.md) (Path, pre-existing), [T-16](https://github.com/codewandler/sipx/blob/main/docs/stories/T-16-service-route-header.md) (Service-Route) | AF-5, RG-1, M3 Path work | filed |
| Server-side digest primitives | Hash formulas and challenge parsing exist client-side (`sipx-ua/src/auth.rs`); nonce minting, replay window, challenge emission and verification are absent | [S-16](https://github.com/codewandler/sipx/blob/main/docs/stories/S-16-server-side-digest.md) | RG-2 | filed |
| Async / shared-cache resolver option | `Resolver` trait is sync with per-URI prefetch; fine for a UA, insufficient at proxy throughput; `_sip._ws`/`_sips._wss` SRV prefixes not prefetched | [T-17](https://github.com/codewandler/sipx/blob/main/docs/stories/T-17-resolution-at-proxy-throughput.md) | RT-1 | filed — upstream-vs-local still open, and that story exists to settle it |
| Deterministic timer queue + seeded loopback link (`sipx-testkit`) | `sipx-transport`'s `TimerQueue` reads `Instant::now()` inside `set` and is keyed to `(TransactionKey, Timer)`; the testkit's crate doc promises a loopback transport and ships none — its modules are `certs`, `load`, `rfc4475`, `soak`. CF-1's asks: generalize the queue (generic key, `now` passed in) and add a seeded in-process byte link with loss/duplication/latency knobs | [X-14](https://github.com/codewandler/sipx/blob/main/docs/stories/X-14-testkit-timer-queue-and-loopback-link.md) | CF-4, CF-5 | filed |
| Multi-node simulation runtime (virtual clock, simulated network + LB stickiness, fault schedules, scenario runner, load model) | Nothing — and per CF-1, nothing should: no kernel `Clock` trait either, since sans-IO layers take fired timers by contract and the clock is the harness scheduler's loop variable | — | CF-5 | decided: stays here (CF-1) |
| Requirement-grain conformance registry | Per-RFC grain only: `docs/rfc/registry.toml` → `rfc-report.py` → `docs/compliance.md`, gate-enforced, claims checked against parser tables and cited files | [X-15](https://github.com/codewandler/sipx/blob/main/docs/stories/X-15-requirement-grain-registry.md) (an offer; declining is a valid close) | CF-2, CF-6, EX-2 | decided: local extension of the kernel schema, kernel rows inherited by reference (CF-1) |
| Unmatched **responses** are dropped | `endpoint.rs`'s `Dispatch::Unmatched` arm forwards only `Message::Request` to the application; a response matching no client transaction is logged and discarded. RFC 3261 §16.7 step 1 requires a stateful proxy with no response context to forward it statelessly — which it cannot do without seeing it | [T-18](https://github.com/codewandler/sipx/blob/main/docs/stories/T-18-surface-unmatched-responses.md) | PX-5 (M2 assertions; not M1 — one node always finds its context) | filed |
| Incoming requests are dropped silently under backpressure | `let _ = self.incoming.try_send(…)`: on a full channel the request is gone, nothing logged, no counter. A dropped 2xx ACK is a **call that never ends** — the ACK is the only thing that concludes it and nothing retransmits after Timer H | [T-19](https://github.com/codewandler/sipx/blob/main/docs/stories/T-19-stop-dropping-incoming-requests-silently.md) | RT-3 (overload); M2 node-loss assertions | filed — a kernel defect on its own terms |
| Released TLS/WS/WSS transports | Shipped in sipx **0.2.0** — TLS with a certificate policy (`T-6`, `T-7`), SIP over WebSocket (`T-8`), WSS (`T-9`), with interop runs against independent implementations (`T-10`). The `sipx-transport` features `tls`, `ws` and `wss` are on by default | — | M2 TLS edges (DP-2), M3 WSS clients | landed in 0.2.0 |

Rules of the ledger:

- A row is **open** until the sipx story is filed, **filed** once it exists (link it), **landed**
  once released in a sipx version this workspace can pin.
- Blocked sipx-clstr stories carry the marker in their `note:` field and name this file.
- If a gap turns out to be clstr-specific after all, the row records that decision instead of
  silently disappearing.
- A row can also be wrong. The TLS/WS/WSS row above was written against sipx 0.1.0 and stayed
  `open` through a release that closed it; re-read the kernel before believing a row, and correct
  it when it lies.
