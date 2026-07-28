# Design: B2BUA services

**Status:** deferred — not scheduled · **Pillar:** Services · **Epic:** `services-b2bua` ·
**Stories:** _none yet_

## Why

The deferred home for features that must terminate dialogs — opt-in per feature, never default.

Some features structurally require terminating one dialog and creating another: call queues, IVR,
conference focus, application-generated call legs, strict topology separation, protocol
normalization that cannot preserve dialogs. The platform is proxy-first precisely so that this
state-heavy machinery (two dialogs, two offer/answer machines, CSeq translation, leg correlation,
transfer and failure translation per leg) is **opt-in per feature**, never the default path. This
placeholder exists so the layers below — hook phases, media control, affinity tokens — are
designed with a dialog-terminating consumer in mind, and it is deliberately not scheduled:
building services on an unproven platform is how a platform acquires workarounds it can never
remove.

## Approach

_Not designed. The shape, when it comes: a separate session service consuming the platform (and
the sipx dialog/call layer) as libraries — two `Dialog`s per session, an offer/answer state
machine per leg, media anchored via `MediaRelay`, sessions owned by one node with replication as
the explicit call-survival-HA feature the vision defers. Queues, IVR and conference are
applications on that service, not core features._

## Alternatives considered

- _Not applicable yet. The standing decision it records: B2BUA is a separate service, not a mode
  of the proxy._

## Risks & open questions

- Whether session state replication (call-survival HA) is designed into the service from its
  first story or added later — the vision only forbids *promising* it silently, not building it.
- Where the boundary between "routing feature" (proxy + hooks) and "session feature" (B2BUA)
  falls for borderline cases like early-media announcements.

## Acceptance / done

_Undefined. Revisit when M3 nears completion._
