---
id: RT-6
title: Specify declarative number normalisation
pillar: Signalling
status: backlog
priority: 
design: docs/designs/routing-trunks.md
epic: routing-trunks
areas: [routing, numbering]
note: 
---

# Specify declarative number normalisation

## Goal
Express number normalisation — stripping and prefixing on From, To, Request-URI and P-Asserted-Identity — as declarative, testable configuration instead of regex embedded in routing logic.

## Acceptance
- [ ] Normalisation rules are data: which transformations, applied to which fields, in which order.
- [ ] A digit-count guard with a fallback field is expressible (e.g. a Request-URI user outside a digit range falls back to the To user).
- [ ] E.164 policy for egress — forcing a leading `+` — is a declared rule, not a code path.
- [ ] Test vectors cover each rule and their composition.

## Progress
- (not started)

## Notes
- One deployment strips a leading `+` and leading zeros from four fields, then falls back to the To user when the Request-URI user is not 3..20 digits.
- Today these are regexes inside route blocks, so they cannot be reviewed or tested independently.
- Filed from a downstream deployment of this platform, whose capability inventory records this as `upstream` (its ledger entry **U-6**). The evidence sits in that deployment's own reference material.
