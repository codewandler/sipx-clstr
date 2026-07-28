---
id: RG-3
title: Implement REGISTER processing on the in-memory store
pillar: Signalling
status: backlog
priority: 
design: docs/designs/registrar-location.md
epic: registrar-location
areas: [registrar, location]
note: blocked by RG-1, CF-5
---

# Implement REGISTER processing on the in-memory store

## Goal
Implement RG-1's REGISTER semantics against the in-memory `LocationStore`, proving the CAS contract before any database is involved.

## Acceptance
- [ ] RG-1's vectors pass: add/replace/remove, wildcard deregistration, Call-ID/CSeq rules, `Min-Expires`/423, expiry, complete-set responses.
- [ ] Retried REGISTERs are idempotent; concurrent updates to one AoR serialize via CAS conflict-and-retry under the harness.

## Progress
- (not started)

## Notes
- Design: [registrar-location](../designs/registrar-location.md).
