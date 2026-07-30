---
id: RG-19
title: Render the complete REGISTER outcome on the wire
pillar: Registrar
status: ready
priority: 1
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

- [ ] One outcome-to-response renderer is exhaustive over `Outcome`/`Rejection`; the node does not
      reconstruct registrar semantics with status-only match arms.
- [ ] A successful response lists every active Contact with granted `expires` and stored q, echoes
      every stored Path value in order, and includes `Supported: path` when Path is in use.
- [ ] `BadExtension` renders every offender in `Unsupported`; `IntervalTooBrief` renders
      `Min-Expires`; `ExtensionRequired("path")` identifies `path` in the response required by the
      settled spec.
- [ ] Header construction failure cannot silently omit one required fact and still send the success
      or rejection. It produces a controlled internal failure with no panic.
- [ ] **Failing-first wire tests:** exercise LS-R-11, LS-R-17, LS-R-18 and LS-R-20 through the real
      node renderer and compare status plus ordered headers. The core vectors already pass on
      `86e6b10`; every required extra header is absent on its wire path.
- [ ] Multiple Contacts, Path values and option tags retain their specified order and parse back to
      the same facts through sipx typed headers.
- [ ] `scripts/gate.sh` is green.

## Progress

- (not started)

## Notes

- Validated synthesis finding [**V-10**](../reviews/00-validated-synthesis.md#v-10--the-wire-response-drops-registrar-facts-required-by-the-contract).
- Keep `Accepted` and `Rejection` as the decision boundary. A renderer may serialize their facts; it
  must not re-decide expiry, Path policy or supported extensions.
- **Upstream boundary:** SIP header syntax/building belongs to sipx; mapping this platform's
  registrar outcome onto those typed headers is local driver work.
