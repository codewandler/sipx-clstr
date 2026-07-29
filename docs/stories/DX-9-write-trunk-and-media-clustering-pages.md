---
id: DX-9
title: Write the trunk and media clustering pages
pillar: Foundation
status: ready
priority: 5
design: docs/designs/docs-site.md
epic: docs-site
areas: [docs, trunks, media]
note: no RTP in the signalling process ever — a vision non-goal, not an implementation detail
---

# Write the trunk and media clustering pages

## Goal

Give `website/docs/clustering/trunks-and-carriers.md` and `website/docs/clustering/media.md` their
content: the egress side — carrier interconnect, number normalisation, asserted identity — and why
media is controlled rather than carried.

## Acceptance

- [ ] Both pages open with the `:::caution Preview` admonition.
- [ ] `trunks-and-carriers.md` explains a trunk as a peer interconnect carrying its own media
      policy, normalisation profile, identity policy and quirks.
- [ ] It explains number normalisation's binding rule: exactly two directions, **ingress** (one
      profile per ingress scope, applied before target determination, so its result is the routing
      key) and **egress** (one per trunk, applied per branch, so a fork sends each branch its own
      trunk's numbers); profiles never chain; nothing is normalised without a binding.
- [ ] It explains asserted identity and privacy per trunk — that whether a trunk honours a
      caller's privacy request is a **per-trunk policy** that is either performed or declined, not
      a global switch.
- [ ] `media.md` states the non-goal plainly: no RTP in the SIP process, ever. Media is relayed by
      an external process controlled over a network protocol, or it flows directly between
      endpoints as it does today.
- [ ] Both cite governing rule IDs and link their specs by absolute GitHub URL.

## Progress

- (running log)

## Notes

- Specs: `docs/specs/number-normalisation.md`, `docs/specs/asserted-identity.md`,
  `docs/specs/media-relay.md`; designs: `docs/designs/routing-trunks.md`,
  `docs/designs/media-control.md`. Absolute GitHub URLs only.
- `rtpengine` is an allowlisted integration target and may be named as the relay this platform
  controls. It is a target, never a precedent.
- Stories: the `RT-*` and `ME-*` sets.
- Today media flows directly between endpoints because there is no relay — say so, rather than
  implying a relay exists.
