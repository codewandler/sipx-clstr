# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.0] — 2026-07-28

### Added

- **A front door.** `README.md` explains the project to a person arriving cold: the problem (a
  five-node SIP proxy has behaviour no RFC describes), the answer (no shared call state — a signed
  token in the message carries what routing needs), and where this actually is. The status is
  stated in the first screen rather than discovered on the third: four specs written, no Rust yet.
- **A logo** (`docs/assets/logo.svg`) — the sipx crab, kept identical for family resemblance,
  shrunk inside a three-node mesh. At favicon size what survives is an orange body in a dark
  triangle, which is the distinction worth keeping: sipx is a phone, sipx-clstr is a cluster of
  them.
- **A published documentation site** at
  [codewandler.github.io/sipx-clstr](https://codewandler.github.io/sipx-clstr/) — Docusaurus
  reading `docs/` directly rather than copying it, so there is one set of words. Curated sidebar
  (the story board and the archive stay working material), offline search, mermaid diagrams, and a
  palette taken from the logo. It deploys on published releases only, so the public site follows a
  tag rather than the last hour's work.
- **CI for the documentation gate** (`.github/workflows/docs.yml`), making executable what
  AGENTS.md describes: relative links resolve, every `epic:` slug has a design doc, every `design:`
  path exists. It joins a build job when `CX-2` lands the Cargo workspace rather than being
  replaced by one.
- **MIT and Apache-2.0 licenses**, matching sipx.

### Changed

- **AGENTS.md gained a map** — where each kind of document lives, which are generated, and the
  state of play in one line — plus the publishing rule and two additions to the gate.
- **The downstream boundary is deployment-agnostic.** The gap stories filed from a consuming
  deployment carried that deployment's name through their notes and prose; a platform repo that
  names one consumer invites requirements shaped for that consumer. Traceability is kept by citing
  the ledger entry rather than the repo.

### Fixed

- **Links that only worked for the author.** `../../sipx` was a path to a sibling checkout;
  `../AGENTS.md`, `../CHANGELOG.md` and the board pointed outside what gets published. All are now
  absolute URLs that resolve in the repository and on the site alike.
- Contact-set notations in the location-service spec are code-spanned, matching the convention
  already used by their neighbours in the same table.

## [0.2.0] — 2026-07-28

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
- **The operator epic starts moving and the backlog deepens.** The Helm chart scaffold under
  `deploy/helm/` (KO-2, in progress), the active-drain story (KO-9), and fifteen new stories
  across routing (egress allowlist, number normalisation, asserted identity, source-IP
  admission, scoped routes), deployment (split listeners, CDRs, capture), extensions (async
  external routing hook, carrier quirk profiles), media (per-trunk codec/SRTP policy), and the
  operator (naming contract, anti-affinity, OCI chart).

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
