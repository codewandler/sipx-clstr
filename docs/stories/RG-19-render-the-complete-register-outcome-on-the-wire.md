---
id: RG-19
title: Render the complete REGISTER outcome on the wire
pillar: Registrar
status: done
design: docs/designs/registrar-location.md
epic: registrar-location
areas: [registrar, node]
note: V-10 · the core preserves q, Path, Supported, Unsupported and Min-Expires facts that the node silently drops
---

# Render the complete REGISTER outcome on the wire

## Goal

Make the SIP response emitted by the node carry every fact the sans-IO registrar outcome and
location-service contract require, without moving registration policy into the driver.

## Acceptance

- [x] One outcome-to-response renderer is exhaustive over `Outcome`/`Rejection`; the node does not
      reconstruct registrar semantics with status-only match arms.
- [x] A successful response lists every active Contact with granted `expires` and stored q, echoes
      every stored Path value in order, and includes `Supported: path` when Path is in use.
- [x] `BadExtension` renders every offender in `Unsupported`; `IntervalTooBrief` renders
      `Min-Expires`; `ExtensionRequired("path")` identifies `path` in the response required by the
      settled spec.
- [x] Header construction failure cannot silently omit one required fact and still send the success
      or rejection. It produces a controlled internal failure with no panic.
- [x] **Failing-first wire tests:** exercise LS-R-11, LS-R-17, LS-R-18 and LS-R-20 through the real
      node renderer and compare status plus ordered headers. The core vectors already pass on
      `86e6b10`; every required extra header is absent on its wire path.
- [x] Multiple Contacts, Path values and option tags retain their specified order and parse back to
      the same facts through sipx typed headers.
- [x] `scripts/gate.sh` is green.

## Progress

- Considered for upstream before implementation: **no — local driver mapping.** The pinned kernel
  already owns and exposes typed SIP header syntax/building, including `Expires`; this story maps
  this platform's registrar `Outcome`/`Rejection` facts onto those kernel types and defines the
  controlled driver failure when a complete response cannot be built. No protocol-generic syntax,
  parser, transaction, or dialog behavior is added, so no upstream-ledger row is needed.
- Failing first: the four real-UDP node tests all failed against the old renderer. LS-R-11,
  LS-R-18 and LS-R-20 received no registrar fact headers; LS-R-17 received Contacts without q and
  no Path or `Supported: path`.
- One exhaustive renderer now serializes every accepted Contact, q, granted lifetime and Path fact
  in outcome order, and every typed rejection remedy. A required-header build failure discards the
  partial response and becomes a controlled bare `500`.
- The wire tests parse Contact, Path and option-tag values back through sipx's typed headers. Their
  status-and-header assertions moved LS-R-11, LS-R-18 and LS-R-20 from shape-only to proved; the
  generated ledger is now 218/619 proved, 16 shape-only and 385 deferred.
- Verification: the focused node suite and strict node Clippy pass; `scripts/gate.sh` is green,
  including all-features tests, reduced-feature builds, Rust 1.91 MSRV, provenance, vectors, docs
  and site checks.

## Notes

- Validated synthesis finding [**V-10**](../reviews/00-validated-synthesis.md#v-10--the-wire-response-drops-registrar-facts-required-by-the-contract).
- Keep `Accepted` and `Rejection` as the decision boundary. A renderer may serialize their facts; it
  must not re-decide expiry, Path policy or supported extensions.
- **Upstream boundary:** SIP header syntax/building belongs to sipx; mapping this platform's
  registrar outcome onto those typed headers is local driver work.
