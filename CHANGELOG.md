# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **The M0 load-bearing specs land** — written concurrently and cross-reconciled. The proxy
  behavior spec (PX-1): RFC 3261 §16 amended by RFC 5393 as a sans-IO engine with 42 vectors.
  The location service spec (RG-1): a canonical AoR byte form, the per-AoR CAS contract, the
  PostgreSQL and in-memory mappings, forking-ordered lookups. The affinity token spec (AF-1):
  byte-level layout with direction and a 64 B module-facts region, AEAD by default, worst-case
  157 B against the 200 B URI-parameter budget, stateless replay semantics — and a settled
  no-mid-dialog-refresh rule (route sets are fixed at establishment) now reflected in the
  media-control reselection risk and KO-9. The hook framework spec (EX-1): thirteen phases
  aligned to the proxy pipeline, closed per-phase effect sets, a manifest whose state-key
  domain makes dialog-keyed module state unrepresentable. The harness design (CF-1): a
  discrete-event clock, links-with-policies with faults as scheduled overrides, scenarios as
  code over declarative schedule values, a per-component sipx-testkit upstream split, and the
  conformance registry as a requirement-grain local extension of the kernel's per-RFC registry,
  kernel rows inherited by reference. (PX-1, RG-1, AF-1, EX-1, CF-1)

## [0.1.0] — 2026-07-28

### Added

- **Design scaffold (M0).** The track backlog framework, the vision and roadmap, the upstream
  dependency ledger, eleven epic design docs (proxy engine, registrar & location, cluster
  affinity, routing & trunks, media control, extension framework, conformance harness,
  deployment, the deferred B2BUA services placeholder, the end-to-end call probe, and the
  Kubernetes operator), architecture charts, and a 60-story backlog: a six-story ready queue for
  the M0 specs plus the implementation backlog toward M1 and M2 (M3/M4 stories are seeded when
  their milestones near). No code yet — the Cargo workspace arrives with `CX-2` as the first act
  of M1.

### Fixed

- **Review findings resolved across the design layer.** The ICE stance no longer lets anchored
  calls negotiate around the relay; the affinity token gained its missing direction field and
  honest, stateless replay semantics; transaction affinity is now an explicit dataplane
  requirement instead of an unstated assumption; previously unowned work got owning stories
  (CF-5 harness implementation, CF-6 conformance-registry seeding, AF-7 connection ownership,
  ME-5 media-anchoring module); M1's stateless-mode promise, the sipx transport-milestone
  attribution, an RFC 3263 §4.3→§4.4 miscitation, and the impossible "answered-then-cancelled →
  487" wording were corrected; blockers and upstream markers moved into board-visible `note:`
  fields, and the charts were rewired to match the designs.
