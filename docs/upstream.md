# Upstream dependency ledger — what sipx-clstr needs from sipx

sipx-clstr builds on the [sipx](https://github.com/codewandler/sipx) kernel and, per the working agreement
([AGENTS.md](https://github.com/codewandler/sipx-clstr/blob/main/AGENTS.md) → *Upstream first*), never shadow-implements protocol logic that
belongs there. This ledger tracks every kernel gap this platform depends on: what it is, which
sipx story covers it (filed by `CX-1`), and which sipx-clstr stories it blocks. Two repos sharing
a boundary is this project's biggest coordination risk; this table keeps it auditable.

**State of play — sipx `v0.4.0` cleared this ledger.** Every filed row landed in one release:
`S-15`, `S-16`, `T-14`, `T-16`, `T-17`, `T-18`, `T-19` and `X-20` are in the tag this workspace
pins. The only row that is not `landed` is `X-15`, which was filed as an offer and which `CF-1`
had already decided stays local.

One row reopened on contact with the code: `X-14` shipped, but it generalized `TimerQueue` over its
key rather than its clock, so the harness still cannot drive it from virtual time. `CF-7` is
blocked on a follow-up that is **not filed yet**. This is the third time a row here has been
believed rather than read — the TLS row stayed `open` through the release that closed it, and this
one read `landed` for a day before anyone tried to use it. Re-read the kernel before trusting a
row.

**Decided (not upstream):** the proxy transaction driver — one server transaction fanning out to
N client transactions — is built **here**, directly over `sipx-sip`'s sans-IO `TransactionLayer`.
The sipx transport driver stays UA-shaped; only generic primitives move upstream.

| Gap | What sipx has today | sipx story | Blocks here | Status |
|---|---|---|---|---|
| Header surgery API | `Headers` exposes `push`/`push_front`/`remove_all` but no `remove_first`, `insert_at` or `retain`; sipx-transport privately rebuilds the collection to rewrite the top Via (`nat.rs:149`) | [S-15](https://github.com/codewandler/sipx/blob/main/docs/stories/S-15-header-editing-operations.md) | PX-3, PX-4 | **landed in 0.4.0** — `Headers::remove_first` and `retain` (`sipx-sip/src/message.rs:387,408`) |
| `Path` and `Service-Route` headers | Not in the `HeaderName` enum; fall through to `Other`. Typed accessors and compact handling missing | [T-14](https://github.com/codewandler/sipx/blob/main/docs/stories/T-14-register-a-path-header.md) (Path, pre-existing), [T-16](https://github.com/codewandler/sipx/blob/main/docs/stories/T-16-service-route-header.md) (Service-Route) | AF-5, RG-1, M3 Path work | **landed in 0.4.0** — both typed in `HeaderName` (`sipx-sip/src/headers/address.rs:264,271`) |
| Server-side digest primitives | Hash formulas and challenge parsing exist client-side (`sipx-ua/src/auth.rs`); nonce minting, replay window, challenge emission and verification are absent | [S-16](https://github.com/codewandler/sipx/blob/main/docs/stories/S-16-server-side-digest.md) | RG-2 | **landed in 0.4.0** — `sipx-ua::challenge::{Authenticator, Presented, Verdict}`, consumed by `sipx-clstr-registrar::auth` |
| Digest primitives reachable without an async runtime | `sipx-ua` depends on `tokio` and `sipx-transport` unconditionally, though only `agent`, `flows` and `error` need either. A sans-IO registrar cannot take S-16's authenticator without linking a runtime into its decision core — and its alternative is to write digest a second time, which is what S-16 exists to prevent | [X-20](https://github.com/codewandler/sipx/blob/main/docs/stories/X-20-digest-without-a-runtime.md) | RG-2 | **landed in 0.4.0** — a default-on `runtime` feature; the workspace pins `default-features = false`, and `sipx-clstr-registrar/tests/sans_io.rs` asserts the resulting lockfile against it rather than trusting the manifest comment |
| Async / shared-cache resolver option | `Resolver` trait is sync with per-URI prefetch; fine for a UA, insufficient at proxy throughput; `_sip._ws`/`_sips._wss` SRV prefixes not prefetched | [T-17](https://github.com/codewandler/sipx/blob/main/docs/stories/T-17-resolution-at-proxy-throughput.md) | RT-1 | **landed in 0.4.0** — settled *upstream*: `sipx-transport/src/resolve.rs`. RT-1's design is now against that resolver rather than about where it should live |
| Deterministic timer queue + seeded loopback link (`sipx-testkit`) | `sipx-transport`'s `TimerQueue` reads `Instant::now()` inside `set` and is keyed to `(TransactionKey, Timer)`; the testkit's crate doc promises a loopback transport and ships none — its modules are `certs`, `load`, `rfc4475`, `soak`. CF-1's asks: generalize the queue (generic key, `now` passed in) and add a seeded in-process byte link with loss/duplication/latency knobs | [X-14](https://github.com/codewandler/sipx/blob/main/docs/stories/X-14-testkit-timer-queue-and-loopback-link.md) | CF-4, CF-5, CF-7 | **landed in 0.4.0, and does not close the gap** — both pieces are keyed to `tokio::time::Instant`, which has no deterministic constructor, so neither is drivable from virtual time. See the row below |
| Timer queue drivable from a virtual clock | `X-14` generalized `TimerQueue` over its **key** and passes `now` in, but the instant type is still `tokio::time::Instant` (`timers.rs:19`), whose only constructors are `now()` and `from_std` — a virtual clock cannot build an epoch. `TimerQueue` also stores keys without payloads, where the harness needs deliveries, timers and breaks in one totally ordered queue. `testkit::Link` is two-party, has no stream class, and draws faults from its own generator rather than the harness's | — **not filed yet** | CF-7 | **open** — needs `TimerQueue` generic over its instant type, and a decision on whether an N-party link belongs upstream at all |
| Multi-node simulation runtime (virtual clock, simulated network + LB stickiness, fault schedules, scenario runner, load model) | Nothing — and per CF-1, nothing should: no kernel `Clock` trait either, since sans-IO layers take fired timers by contract and the clock is the harness scheduler's loop variable | — | CF-5 | decided: stays here (CF-1) |
| Requirement-grain conformance registry | Per-RFC grain only: `docs/rfc/registry.toml` → `rfc-report.py` → `docs/compliance.md`, gate-enforced, claims checked against parser tables and cited files | [X-15](https://github.com/codewandler/sipx/blob/main/docs/stories/X-15-requirement-grain-registry.md) (an offer; declining is a valid close) | CF-2, CF-6, EX-2 | decided: local extension of the kernel schema, kernel rows inherited by reference (CF-1) |
| Unmatched **responses** are dropped | `endpoint.rs`'s `Dispatch::Unmatched` arm forwards only `Message::Request` to the application; a response matching no client transaction is logged and discarded. RFC 3261 §16.7 step 1 requires a stateful proxy with no response context to forward it statelessly — which it cannot do without seeing it | [T-18](https://github.com/codewandler/sipx/blob/main/docs/stories/T-18-surface-unmatched-responses.md) | PX-5 (M2 assertions; not M1 — one node always finds its context) | **landed in 0.4.0** — the M2 assertion is now writable |
| Incoming requests are dropped silently under backpressure | `let _ = self.incoming.try_send(…)`: on a full channel the request is gone, nothing logged, no counter. A dropped 2xx ACK is a **call that never ends** — the ACK is the only thing that concludes it and nothing retransmits after Timer H | [T-19](https://github.com/codewandler/sipx/blob/main/docs/stories/T-19-stop-dropping-incoming-requests-silently.md) | RT-3 (overload); M2 node-loss assertions | **landed in 0.4.0** — closed as a kernel defect on its own terms, which is how it was filed |
| Released TLS/WS/WSS transports | Shipped in sipx **0.2.0** — TLS with a certificate policy (`T-6`, `T-7`), SIP over WebSocket (`T-8`), WSS (`T-9`), with interop runs against independent implementations (`T-10`). The `sipx-transport` features `tls`, `ws` and `wss` are on by default | — | M2 TLS edges (DP-2), M3 WSS clients | landed in 0.2.0 |

Rules of the ledger:

- A row is **open** until the sipx story is filed, **filed** once it exists (link it), **implemented
  upstream, unreleased** once the kernel code exists but no tag this workspace can pin carries it,
  and **landed** once released in a sipx version this workspace can pin.
- *Unreleased* is a real state and not a formality. Code on someone's `main` is not a dependency:
  `Cargo.toml` pins a **tag** from the GitHub URL precisely so "which kernel is this claim true of?"
  has an answer, and a story that consumes unreleased kernel code is still blocked. The remedy is a
  release, not a `[patch]` — patching to a local checkout makes the build unreproducible and hides
  the dependency from exactly the ledger that exists to track it.
- Blocked sipx-clstr stories carry the marker in their `note:` field and name this file.
- If a gap turns out to be clstr-specific after all, the row records that decision instead of
  silently disappearing.
- A row can also be wrong. The TLS/WS/WSS row above was written against sipx 0.1.0 and stayed
  `open` through a release that closed it; re-read the kernel before believing a row, and correct
  it when it lies.
