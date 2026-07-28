# Design: Proxy engine

**Status:** proposed · **Pillar:** Signalling · **Epic:** `proxy-engine` ·
**Stories:** PX-1 … PX-7

## Why

The forwarding layer the whole platform stands on: RFC 3261 §16 as a sans-IO engine.

The platform is proxy-first: dialogs stay end-to-end between endpoints, and the cluster forwards.
That keeps the state the cluster must hold — and therefore replicate — to a minimum, and it is
what makes the affinity-token model possible at all. The sipx kernel supplies everything below
this layer (lossless messages with byte-exact passthrough, all four RFC 3261 transaction machines
amended by RFC 6026, transports, resolution) but contains **no forwarding path**: no Via push/pop,
no `Max-Forwards` decrement, no Record-Route insertion, no forking, no response aggregation, no
CANCEL propagation. This epic builds that layer — RFC 3261 §16, amended by RFC 5393 for loop
detection — as a sans-IO engine over the kernel.

## Approach

**Two modes, one engine.** Transaction-stateful proxying (§16.2) is the primary mode: one server
transaction, N client transactions, a response context selecting the best response (§16.7).
Stateless forwarding (§16.11) is a strict subset used where per-message cost matters (e.g.
in-dialog requests already carrying a valid affinity token); anything that forks, recurses, or
must generate its own responses is stateful by definition.

**The engine is pure.** Following the kernel's sans-IO discipline, the proxy core is a state
machine: messages, fired timers and verified-token facts go in; effects come out (send on branch,
create client transaction, cancel branch, respond upstream, set/clear timer). Sockets, clocks and
the location service live in the driver. This is what makes the §16 rules testable under the
deterministic harness (`conformance-harness`) with vectors from the spec (`PX-1`).

**Request processing pipeline** (each step a typed hook phase — see `extension-framework`):
validate (§16.3, including the `Max-Forwards` check and RFC 5393 loop/spiral detection via branch
parameter reuse rather than Via counting alone) → preprocess routing information (§16.4: strip
own Route, handle strict-router predecessors) → target determination (location service, trunk
routing, or the Request-URI) → forward to each target (§16.6: copy, decrement `Max-Forwards`,
push Via with a new branch, optionally insert Record-Route carrying the affinity token, apply
Route) → response processing (§16.7: pop own Via, aggregate, pick best final response, forward
provisionals) → CANCEL propagation on a better final response or upstream CANCEL, with `487`
generation for branches cancelled before a final response. Timer C guards INVITE branches that
stop receiving provisionals.

**The proxy driver lives here, not in sipx** — but it is built on the kernel's endpoint, not
beside it. This paragraph originally argued that `sipx_transport`'s API was "UA-shaped: one
transaction, one target" and that a forking proxy therefore needed its own socket loop over
`sipx_sip::transaction::TransactionLayer`. `PX-2` read the released kernel rather than
remembering it and found that wrong in the way that matters: `Handle::send` creates one client
transaction per call, N calls fan out concurrently over one `TransactionLayer`, and `send`
inserts a `Via` only when one is absent — so a proxy that pushes its own keeps control of the
branch, and the branch it chooses is the transaction key. **Decision: build on
`sipx_transport::Handle`**, and file the two things a proxy needs that a UA does not (unmatched
responses surfaced, requests not dropped silently) as kernel changes. Full rationale, the effect
table and the backpressure analysis: [proxy-transaction-driver](proxy-transaction-driver.md).

Only generic primitives are upstreamed — the `Headers` surgery API (`remove_first`, `insert_at`,
`retain`) needed for Via pop/push and Record-Route insertion is a sipx change
([upstream ledger](../upstream.md), story PX-3).

**What the proxy never does:** own dialog state, rewrite bodies (that is `media-control`'s hook),
or terminate calls. A dialog-terminating feature belongs in `services-b2bua`.

## Alternatives considered

- **Extend sipx-transport's `Driver` to 1→N fan-out.** Rejected (with the user): it destabilizes
  a shipped, UA-shaped endpoint loop and couples both roadmaps; the kernel's transaction layer is
  the right reuse boundary, not the UA driver above it.
- **B2BUA-first (terminate every call).** Rejected by the vision: it multiplies replicated state
  (two dialogs, two offer/answer machines, CSeq translation per call) and forfeits end-to-end
  transparency for the common case.
- **Stateless-only edge.** Rejected: forking, authentication challenges, CANCEL and Timer C all
  require transaction state; RFC 3263 §4.4 failover itself recommends becoming stateful after the
  first failed attempt.

## Risks & open questions

- ~~The exact crate boundary against sipx-transport.~~ **Settled by PX-2:** the driver is built on
  `sipx_transport::Handle`; `Pool`, `Resolver`, `resolve`, `Target` and `TransportKind` are all
  public and imported as they are; nothing is extracted; two gaps are filed upstream as sipx
  `T-18` and `T-19`.
- Record-Route token size: RFC 3261 puts no hard limit on URI length, but UDP fragmentation is
  real; the affinity token spec (AF-1) must budget bytes with this epic in the review.
- ~~Whether stateless mode ships in M1 at all.~~ **Decided:** stateless mode is defined in the
  spec (PX-1) but implemented only in M2, when the token path gives it a real consumer; M1 ships
  transaction-stateful forwarding only. The roadmap's M1 entry says the same.

## Acceptance / done

The union of PX-1 … PX-7: a normative `docs/specs/proxy-behavior.md` with vector tables; a
sans-IO proxy core passing those vectors under the deterministic harness, including forking,
CANCEL, Timer C, loop detection and best-response selection; and an M1 node through which two
sipx CLI phones register and call each other with media direct.
