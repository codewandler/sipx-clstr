---
id: RG-7
title: Support migrating an existing credential store
pillar: Signalling
status: backlog
priority: 
design: docs/designs/registrar-location.md
epic: registrar-location
areas: [registrar, auth]
note: gates a downstream registrar cutover
---

# Support migrating an existing credential store

## Goal
Let the registrar authenticate against credentials that already exist in a deployment's current registrar, so cutover does not require re-provisioning every endpoint.

## Acceptance
- [ ] The credential backend is pluggable behind the location service's auth interface.
- [ ] Digest authentication works against pre-existing hashed credentials without knowing the plaintext.
- [ ] A migration path is documented for at least one common existing schema.
- [ ] A failing-first test authenticates an endpoint using an imported credential.

## Progress
- (not started)

## Notes
- A deployment's agent credentials live in the existing registrar's own database. Re-provisioning every agent endpoint is not an acceptable cutover step.
- Note: the *proxy's* credential table is empty in every environment of one deployment — the requirement is about the registrar's store, not the proxy's.
- Filed from a downstream deployment of this platform, whose capability inventory records this as `upstream` (its ledger entry **U-11**). The evidence sits in that deployment's own reference material.
