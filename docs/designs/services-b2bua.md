# Design: Optional session services

**Status:** proposed for M4 · **Pillar:** Services · **Epic:** `services-b2bua` ·
**Stories:** `BS-1` … `BS-3` · **Spec:** [session-service](../specs/session-service.md)

## Why

Some features must terminate one dialog and create another. A proxy cannot provide that behavior
without ceasing to be a proxy, so the dialog-terminating path is a separate opt-in service consuming
the platform and the released sipx call layer. This keeps ordinary calls stateless or
transaction-stateful at the proxy while giving applications a home for two-leg coupling and a
conference focus.

## Approach

The normative behavior is in [`session-service.md`](../specs/session-service.md). The service has a
sans-IO session engine and a driver:

- the engine owns per-leg dialog and offer/answer state and emits effects;
- the driver uses platform routing, affinity and configuration and the released kernel call
  primitives;
- anchored media is controlled through `MediaRelay`; no service process handles RTP; and
- conference state is external-relay state correlated by the session owner.

`BS-1` reviews and accepts the spec against the kernel release boundary. `BS-2` implements the
two-leg engine/service. `BS-3` adds the conference focus and its three-party proof.

## Boundary decision

Considered for upstream: the session product, routing policy, affinity, relay selection and
deployment stay here because their subject is the clustered service. The generic two-dialog
coupling, call events, offer/answer behavior and endpoint bridge/conference API stay in sipx and are
tracked there. This repository never forks those primitives.

## Alternatives considered

- **A proxy mode.** Rejected: it would put two-dialog state on every call path and make proxy
  behavior depend on service policy.
- **Embedded media.** Rejected: the platform controls an external media cluster and never puts RTP
  in a signalling process.
- **Queues and IVR in M4.** Deferred: they consume the same service after its ownership, failure and
  conference paths are proved; they are not needed to establish the service boundary.

## Risks

- Owner loss ends established sessions in M4. That is stated in the HA contract; session-state
  replication is a later explicit capability, never implied.
- Early-media and glare behavior can diverge per leg. One state machine per leg and the `SS-*`
  vectors prevent a single shared offer state from hiding that divergence.
- External relays differ in conference vocabulary. `MediaRelay` owns the portable contract; adapter
  quirks remain outside the session engine.

## Acceptance / done

The union of `BS-1` … `BS-3`: every `SS-*` vector passes under deterministic time, a real two-leg
bridged call and three-party conference pass through the reference deployment, and disabling the
service leaves the proxy path and artifact dependency graph unchanged.
