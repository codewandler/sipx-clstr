---
id: RT-1
title: Design the RoutePlan and shared-cache resolver
pillar: Signalling
status: done
priority:
design: docs/designs/routing-trunks.md
epic: routing-trunks
areas: [routing, dns]
note: settled upstream — the resolver is the kernel's; what stays here is the plan
---

# Design the RoutePlan and shared-cache resolver

## Goal
Design the `RoutePlan` the proxy driver consumes and the resolver that produces it at proxy throughput: async, shared, TTL- and negative-caching.

## Acceptance
- [x] The RoutePlan type (attempt list: transport, address, source, priority, weight) and its consumption contract are designed. — [routing-trunks](../designs/routing-trunks.md) → *RT-1: the resolver decision*.
- [x] The resolver design keeps every await off the signalling loop and covers the WS/WSS SRV prefixes the kernel's prefetch misses. — both are the kernel's, via `Prefetched`; the design records why that seam made the decision available.
- [x] The upstream-vs-local decision for the async resolver is made and recorded in the upstream ledger. — **upstream**; [ledger](../upstream.md) row `T-17`, and the local option is in the design's *Alternatives considered* with the reason it lost.

## Progress
**Done 2026-07-29, and the headline decision went the way the story could not know in advance.**

- **The resolver is the kernel's.** `T-17` shipped in `v0.4.0` and closes every requirement this
  epic had: `dns::resolve_uri` for the async path, `DnsResolver` for the shared cache with TTLs
  both ways, `Answer::{Records, Unavailable}` so a nameserver blip is not cached as a permanent
  routing failure, and all five SRV prefixes prefetched — including the `_sip._ws` and
  `_sips._wss` this story was written to chase. Verified by reading `v0.4.0`, not by trusting the
  ledger row.
- **The seam that made it possible is `Prefetched`.** RFC 3263 selection is pure computation over
  records, so the kernel awaits first and hands the answers to a synchronous `resolve`. That is
  why a proxy and a UA can share selection logic without either becoming async, and why there is
  no caching layer to write here.
- **What stays local is the plan.** `RoutePlan` is an ordered list of `Attempt`s, each **wrapping**
  the kernel's `Target` rather than restating it — a second address type in this repo would
  eventually disagree with the kernel about which host a TLS certificate must be valid for, and
  that disagreement is a silent downgrade rather than a visible error. The wrapper adds provenance
  (`Naptr | Srv | AddressRecord | Configured | Literal`) and the trunk context `RT-2` keys its
  breaker and CPS limits on.
- **The consumption contract keeps every await in the driver by construction**: the sans-IO core
  emits the `ResolveTargets` effect it already has, the driver awaits and builds the plan, and the
  plan comes back as one input. Advancing an attempt is an input too, which is what makes `RT-4`'s
  failover vectors ordinary harness scenarios rather than tests that need a nameserver.
- Deferred to `RT-2` with a reason rather than left silent: whether a plan is rebuilt or resumed
  when a trunk's policy version changes mid-transaction. It depends on `AF-1`'s policy-version
  field and cannot be settled before the trunk model exists.

## Notes
- Design: [routing-trunks](../designs/routing-trunks.md) — status is now **accepted**.
- This is the second row this week where the kernel had already solved the problem and the ledger
  had not been re-read. `CF-7` is the counter-example from the same day: a row that said `landed`
  and did not fit. Both directions are why the ledger's rule says to re-read the kernel before
  believing a row.
