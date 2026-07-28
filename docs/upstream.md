# Upstream dependency ledger — what sipx-clstr needs from sipx

sipx-clstr builds on the [sipx](../../sipx) kernel and, per the working agreement
([AGENTS.md](../AGENTS.md) → *Upstream first*), never shadow-implements protocol logic that
belongs there. This ledger tracks every kernel gap this platform depends on: what it is, which
sipx story covers it (filed by `CX-1`), and which sipx-clstr stories it blocks. Two repos sharing
a boundary is this project's biggest coordination risk; this table keeps it auditable.

**Decided (not upstream):** the proxy transaction driver — one server transaction fanning out to
N client transactions — is built **here**, directly over `sipx-sip`'s sans-IO `TransactionLayer`.
The sipx transport driver stays UA-shaped; only generic primitives move upstream.

| Gap | What sipx has today | sipx story | Blocks here | Status |
|---|---|---|---|---|
| Header surgery API | `Headers` exposes `push`/`push_front`/`remove_all` but no `remove_first`, `insert_at` or `retain`; sipx-transport privately rebuilds the collection to rewrite the top Via (`nat.rs`) | _to file (CX-1)_ | PX-3, PX-4 | open |
| `Path` and `Service-Route` headers | Not in the `HeaderName` enum; fall through to `Other`. Typed accessors and compact handling missing | _to file (CX-1)_ | AF-5, RG-1, M3 Path work | open |
| Server-side digest primitives | Hash formulas and challenge parsing exist client-side (`sipx-ua/src/auth.rs`); nonce minting, replay window, challenge emission and verification are absent | _to file (CX-1)_ | RG-2 | open |
| Async / shared-cache resolver option | `Resolver` trait is sync with per-URI prefetch; fine for a UA, insufficient at proxy throughput; `_sip._ws`/`_sips._wss` SRV prefixes not prefetched | _to file (CX-1)_ | RT-1 | open |
| Virtual-time / loopback harness split | No `Clock` trait, no in-memory transport, no multi-node harness anywhere; `sipx-testkit` holds the RFC 4475 corpus and a fixture CA only | _to file (CX-1)_ | CF-1, CF-4 | open |

Rules of the ledger:

- A row is **open** until the sipx story is filed, **filed** once it exists (link it), **landed**
  once released in a sipx version this workspace can pin.
- Blocked sipx-clstr stories carry the marker in their `note:` field and name this file.
- If a gap turns out to be clstr-specific after all, the row records that decision instead of
  silently disappearing.
