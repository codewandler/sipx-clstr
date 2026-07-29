# Design: Registrar & location service

**Status:** proposed · **Pillar:** Signalling · **Epic:** `registrar-location` ·
**Stories:** RG-1 … RG-6

## Why

The one place the platform is allowed durable state — so its updates must serialize.

A registrar is the one place the platform is *allowed* durable state, and the correctness of the
whole cluster rests on it: RFC 3261 §10 REGISTER processing writes AoR→Contact bindings that
every later call reads. The sipx kernel implements only the client side (registration as a lease,
digest response generation). This epic builds the server side: authenticated REGISTER processing
and a strongly consistent location service whose lookups yield a routable forking target set —
Contact plus Path plus flow reference — not a bare URI.

## Approach

**Keyed by tenant + canonical AoR.** AoR canonicalization is normative (RG-1 specifies it) because
the canonical form feeds both the storage key and the rendezvous hash; two spellings of one AoR
must never land on two shards. A binding carries: contact URI, Call-ID, CSeq, expiry, Path vector
(RFC 3327), received address, optional instance-id/reg-id (RFC 5626, M3), optional flow_ref
(see `cluster-affinity`), optional push metadata (RFC 8599, M3), the authenticated principal,
and a revision number.

**Per-AoR serialization via CAS.** REGISTER processing is read-modify-write (add, replace, remove,
wildcard-remove contacts; compare Call-ID/CSeq; apply expiry rules; return the complete active
set). The `LocationStore` trait therefore exposes a versioned compare-and-swap contract — apply a
`RegisterCommand` against a revision, get the new binding set or a conflict — so per-AoR updates
serialize regardless of backend, and REGISTER processing is idempotent on retry. Caches may
accelerate reads; they are never the source of correctness.

**Two backends to start.** An in-memory store for the deterministic harness, and **PostgreSQL**
as the first production backend (decided with the user): serializable per-AoR transactions,
boring operability in the 3-zone reference deployment, LISTEN/NOTIFY as the change stream for
cache invalidation. The trait leaves room for a consensus KV or a sharded replicated state
machine later, when scale justifies it.

**Sharding by rendezvous hash** over `tenant_id || canonical_aor` keeps all bindings for one AoR
in one consistency domain and needs no rebalancing metadata service in v1 (membership is
config-first, per `cluster-affinity`).

**Authentication.** REGISTER is challenged with digest (RFC 3261 §22, modern algorithms per
RFC 8760). The hash formulas exist client-side in sipx; the server side — nonce minting, a replay
window, challenge emission, credential verification — is generic protocol machinery and is
upstreamed as primitives ([upstream ledger](../upstream.md), RG-2), with the credential store and
policy living here.

**Lookup for routing.** Resolving an AoR returns the active bindings ordered for forking
(q-values respected, expired bindings gone), each with enough routing material for the proxy to
construct a branch: the Path route set toward the UA and the flow_ref when the binding rides a
client-initiated connection.

## Alternatives considered

- **Last-write-wins replicated cache as the store.** Rejected: REGISTER semantics (wildcard
  removal, CSeq comparison, multi-binding replacement) are read-modify-write; without per-AoR
  serialization, concurrent refreshes corrupt the binding set in ways endpoints only notice as
  missed calls.
- **Custom consensus/replicated state machine in v1.** Rejected: an existing serializable
  database gives the needed semantics now; the trait keeps the door open.
- **Registrar embedded in every edge with gossip reconciliation.** Rejected: violates "every
  resource has one owner" and makes the binding set eventually consistent exactly where
  correctness is defined by serialization.
- **Narrowing what digest binds, so a captured `Authorization` cannot be reattached to a REGISTER
  with a different `Contact`.** Rejected (`RG-9`), and the reasoning is normative in
  [registrar-auth](../specs/registrar-auth.md) §7.3: `qop=auth-int` covers the body and not the
  header fields a binding is made of; one-time nonces would work and would break `RA-R-1`, an
  ordinary UDP retransmission; and folding request state into the nonce is kernel machinery that
  would also destroy the property that any edge can answer any other edge's challenge. The exposure
  is accepted, bounded by the nonce lifetime, and mitigated by carrying signalling over TLS.

## Risks & open questions

- PostgreSQL write throughput per shard under registration storms (mass re-REGISTER after an
  outage): needs a load model in RG-4 and possibly write coalescing for pure refreshes.
- The change-stream contract: LISTEN/NOTIFY delivery is best-effort; cache invalidation must
  tolerate missed notifications (TTL-bounded staleness on the read path).
- `Min-Expires`/423 policy and per-tenant binding quotas: policy shape decided in RG-1 (named in
  its acceptance).
- Digest nonce-store scope across edges: per-node nonces lean on transaction affinity for the
  challenge/response round-trip; a shared store reintroduces coupling. Decided in RG-2.

## Acceptance / done

The union of RG-1 … RG-6: a normative `docs/specs/location-service.md`; server-side digest
challenge and verification; REGISTER processing passing its vectors on the in-memory store under
the harness; the PostgreSQL backend passing the same contract tests; rendezvous sharding; and
proxy target sets built from lookups — completing the M1 register-and-call loop.
