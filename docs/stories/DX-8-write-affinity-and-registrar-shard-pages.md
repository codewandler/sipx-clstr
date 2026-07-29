---
id: DX-8
title: Write the affinity-token and registrar-shard clustering pages
pillar: Foundation
status: ready
priority: 4
design: docs/designs/docs-site.md
epic: docs-site
areas: [docs, affinity]
note: preview pages cite rule IDs — "not shipped" must never read as "not decided"
---

# Write the affinity-token and registrar-shard clustering pages

## Goal

Give `website/docs/clustering/affinity-and-flows.md` and
`website/docs/clustering/registrar-shards.md` their content: what the signed token carries, how
connection ownership works, and who owns an address-of-record.

## Acceptance

- [ ] Both pages open with the `:::caution Preview` admonition — specified and normative, not
      implemented.
- [ ] `affinity-and-flows.md` explains what the token holds (tenant, shard, media node, policy
      version, expiry), where it rides (`Record-Route`, `Route`, `Path`), that it is signed and
      opaque to clients, and what domain separation is for.
- [ ] It explains `flow_ref` — which edge owns a client's connection, and why reaching a NAT'd
      client is an RPC to that edge rather than a lookup.
- [ ] `registrar-shards.md` explains rendezvous hashing to one owning shard per AoR, the
      compare-and-swap contract on the location store, and what happens to a re-`REGISTER` that
      lands elsewhere.
- [ ] Both cite the governing rule IDs and link their specs by absolute GitHub URL.
- [ ] Neither restates a normative rule as if this page were the contract.

## Progress

- (running log)

## Notes

- Specs: `docs/specs/affinity-token.md`, `docs/specs/location-service.md`; design:
  `docs/designs/cluster-affinity.md`. Absolute GitHub URLs only.
- Stories that build this: the `AF-*` set.
- Mermaid is available and renders; a topology chart earns its place here.
- Read the spec before writing. Do not infer the token's fields from the page you are replacing.
