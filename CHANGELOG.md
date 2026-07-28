# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
