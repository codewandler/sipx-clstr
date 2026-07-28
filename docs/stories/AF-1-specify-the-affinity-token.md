---
id: AF-1
title: Specify the affinity token
pillar: Cluster
status: ready
priority: 3
design: docs/designs/cluster-affinity.md
epic: cluster-affinity
areas: [affinity]
note: 
---

# Specify the affinity token

## Goal
Write `docs/specs/affinity-token.md`: the byte-level format and security rules of the signed token that lets any edge route a mid-dialog request with zero global lookups.

## Acceptance
- [ ] Fields specified with byte layout: format version, key id, tenant, home shard, edge affinity, media node, policy version, expiry, nonce, authentication tag — with mint → encode → parse → verify round-trip vectors.
- [ ] Encoding into Record-Route/Route and Path URIs is specified with a size budget (target: well under 200 bytes as a URI parameter), justified against UDP MTU limits.
- [ ] Key rotation via key id and overlapping validity; distribution is config-first.
- [ ] Verification-failure behavior is normative: a mid-dialog request with an unverifiable token is hard-rejected; there is no fallback routing.
- [ ] The encryption policy names which fields are confidential; every token is authenticated.

## Progress
- (not started)

## Notes
- Design: [cluster-affinity](../designs/cluster-affinity.md).
