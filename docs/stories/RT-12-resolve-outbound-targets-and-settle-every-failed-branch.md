---
id: RT-12
title: Resolve outbound targets and settle every failed branch
pillar: Signalling
status: ready
priority: 2
design: docs/designs/routing-trunks.md
epic: routing-trunks
areas: [routing, dns, proxy, node]
note: V-12 · outbound selection is UDP-only; hostname/transport failures are logged without a BranchTransportError and can leave no final response
---

# Resolve outbound targets and settle every failed branch

## Goal

Use the kernel's resolved transport targets for proxy egress and feed every resolution,
materialization or send failure back into the owning sans-IO context, so no valid URI is downgraded to
UDP or leaves a branch pending without an event.

## Acceptance

- [ ] The driver consumes sipx's released resolver/`Target` API from RT-1/T-17 for hostname and
      literal next hops. It does not parse a Contact into a local address-only target type.
- [ ] URI transport parameters, scheme security and resolver results select the actual transport;
      `Via` names that chosen transport and the listener's advertised sent-by. TCP-only contacts are
      never attempted over UDP, and secure schemes never downgrade.
- [ ] DNS unavailable/no-record, unsupported transport, target materialization failure, pool/send
      error and an ended response stream each become the specified explicit branch failure input.
      The context selects another attempt/branch or, when every attempt failed before an answer,
      returns proxy-behavior R8's `500 Server Internal Error`; it never just logs and returns
      unfinished. An authoritative empty AoR remains the distinct `480` case.
- [ ] Resolution and connection IO stay in the driver. The proxy engine receives values and failure
      inputs and remains deterministic under the harness.
- [ ] **Failing-first real-socket cases:** a hostname Contact resolves and receives a request; a
      TCP-only Contact receives TCP and a matching TCP Via; an unresolvable sole target yields the
      policy final instead of silence. All fail on `86e6b10`.
- [ ] Multi-address/multi-transport ordering is controlled by the kernel resolver and tested without
      reimplementing RFC 3263 selection locally.
- [ ] `scripts/gate.sh` is green.

## Progress

- (not started)

## Notes

- Validated synthesis finding [**V-12**](../reviews/00-validated-synthesis.md#v-12--outbound-transport-selection-is-udp-only-and-target-failures-are-not-settled).
- PX-13 consumes direct dialog next hops; this story generalizes how any next-hop URI becomes one or
  more transport attempts and guarantees failure settlement. Neither story may build a second
  resolver.
- **Upstream boundary:** URI resolution and transport selection are protocol-generic and use sipx's
  RT-1/T-17 capability; feeding a driver failure into this platform's response context is local.
