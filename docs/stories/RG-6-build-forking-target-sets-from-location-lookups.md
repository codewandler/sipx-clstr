---
id: RG-6
title: Build forking target sets from location lookups
pillar: Signalling
status: done
priority:
design: docs/designs/registrar-location.md
epic: registrar-location
areas: [registrar, proxy]
note: M1 #7 · the registrar and the proxy meet; found a dropped Path in PX-5
---

# Build forking target sets from location lookups

## Goal
Turn a location lookup into what the proxy forks on: ordered branches carrying the Path route set and, when present, the flow_ref.

## Acceptance
- [x] Lookups return q-value-ordered targets with expired bindings excluded and per-tenant quotas applied.
- [x] The proxy's forking path consumes the target set end-to-end in the M1 register-and-call harness scenario.

## Progress
**The bridge is one function behind an optional feature.** `sipx-clstr-proxy`'s
`registrar-targets` feature adds `targets_from_lookup`, converting the location service's `Target`
into the forwarding core's. Optional and off by default on purpose: the proxy also forwards to
trunks (`RT-*`) and to plain Request-URIs, so it must not *depend* on the registrar — making the
coupling a feature keeps it visible in the manifest and keeps the default build independent of where
targets came from. The feature matrix builds both states.

**Ordering is not redone.** `order_targets` already applied §7's L1–L3 — expired excluded,
descending `q`, ties by recency then contact bytes — and it is a pure function of the set and `now`,
so every node computes the identical order. Re-sorting in the bridge would be a second opinion about
that, and a test pins that the lookup's order survives untouched. Quotas were applied at write time
(§5.5), which is what bounds every lookup by construction rather than by a second check here.

**The scenario RG-6 asks for:** one node running the **real** registrar and the **real** forwarding
core. Two endpoints register, one calls the other, the node looks the callee up, converts, and forks.
Six tests: a call reaches a registered contact; two contacts under one address-of-record fork to
both; a registered `Path` becomes the route set the INVITE carries; an unregistered callee is `480`
(the location service returns an empty set and the *proxy* decides the response); eight seeds replay
byte for byte; sixteen adversarial seeds all complete, bounded below Timer C so a `200` can only have
come from a callee.

### What the integration scenario found that 40 unit vectors did not

**`forward()` never applied the target's route set, so every registered `Path` was silently
dropped.** RFC 3327 §5.3 makes the stored path the route set toward that contact and RFC 3261 §16.6
step 6 says the proxy applies it; `PX-5` implemented every other F-step and missed this one. It was
invisible to the whole `PB-F` table because every vector there uses an empty route set — the bug
lived exactly in the gap between two crates that were each correct alone.

Fixed, and now pinned in three places at the unit level as well: the route set is applied in stored
order; it goes **ahead** of any `Route` that survived preprocessing, because the path is the nearer
part of the journey; and a bare URI is bracketed, since `sip:p;lr` in a `Route` makes `;lr` a
*header* parameter and loses the loose-routing flag entirely.

Two smaller mistakes of my own, both worth recording because both produced confidently wrong
behaviour rather than an error:

- **A far-future `now` for the lookup expires every binding.** I passed `u64::MAX / 2` meaning
  "ignore expiry"; it means the opposite, and cost four failing tests. The node now captures the
  virtual clock on every input.
- **A test phone's trace label doubled as its SIP user**, so a device registered under
  `sip:bob@sip:bob@10.0.0.1@atlanta.example`. Two devices sharing one address-of-record is the whole
  point of forking, so the label and the user are now separate fields.

## Notes
- Design: [registrar-location](../designs/registrar-location.md).
- `flow_ref` is carried by the registrar's `Target` and deliberately *not* by the proxy's: nothing in
  M1 consumes it, and the connection-owner RPC that will (`AF-7`) does not exist yet. Adding a field
  the forwarding core cannot act on would be a promise it does not keep.
- This scenario is the shape of `CX-3`, which does the same thing over real sockets against real
  phones.
