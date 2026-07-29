---
id: DX-9
title: Write the trunk and media clustering pages
pillar: Foundation
status: in-progress
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

- [x] Both pages open with the `:::caution Preview` admonition.
- [x] `trunks-and-carriers.md` explains a trunk as a peer interconnect carrying its own media
      policy, normalisation profile, identity policy and quirks.
- [x] It explains number normalisation's binding rule: exactly two directions, **ingress** (one
      profile per ingress scope, applied before target determination, so its result is the routing
      key) and **egress** (one per trunk, applied per branch, so a fork sends each branch its own
      trunk's numbers); profiles never chain; nothing is normalised without a binding.
- [x] It explains asserted identity and privacy per trunk — that whether a trunk honours a
      caller's privacy request is a **per-trunk policy** that is either performed or declined, not
      a global switch.
- [x] `media.md` states the non-goal plainly: no RTP in the SIP process, ever. Media is relayed by
      an external process controlled over a network protocol, or it flows directly between
      endpoints as it does today.
- [x] Both cite governing rule IDs and link their specs by absolute GitHub URL.

## Progress

- Both pages written; the placeholder bodies are gone and the frontmatter is unchanged.
- `trunks-and-carriers.md`: a trunk as a peer interconnect (table of what it carries — media
  policy `MP1`–`MP4`, normalisation `N23`, identity `A1`/`A14`/`A24`, quirks, operational state);
  the two binding directions with a mermaid figure showing ingress → target determination → fork →
  per-branch egress; `N24` (never chained, and why `add_prefix` being non-idempotent is what makes
  that enforceable); `N25`'s four exclusions with their RFC clauses; `N27`/`N9`/`N28` (no default,
  no built-in profile); then the identity half — `A1`/`A2` trust per peer, `A19` synthesis and
  forwarding as separate axes, `A24`'s six-cell gate, and privacy as `perform`/`decline` with no
  middle value (`A33`, `A35`, `A29`–`A31`, `A22`, `A23`, `A27`), including `A19`'s admission that
  no configuration guarantees a peer never receives a PAI.
- `media.md`: the non-goal first — no RTP in the SIP process, ever, and the only two ways media
  flows; then "today there is no relay, so media goes direct", stated so it cannot read as a
  degraded mode; the five-method port (`O1`–`O4`, `U1`, `U3`, `Q2`, `I1`–`I5`); the NG byte
  contract with the `ping` datagram quoted (`F1`–`F5`, `C1`–`C4`, `E1`–`E5`, `D6`–`D11`, §8, §9
  `X1`); and rtpengine named as the integration target under the AGENTS.md carve-out, never as
  precedent, with drift handled by contract (`V1`–`V5`).
- Gate run in the worktree: `python3 scripts/check-docs.py` → `docs: clean (166 markdown files
  checked)`; `scripts/check-provenance.sh` → `provenance: clean (7 terms checked, 0 carved out as
  integration targets)`; `npm run build` in `website/` → `[SUCCESS] Generated static files in
  "build".` No Rust changed, so `scripts/gate.sh` was not run.
- The site build is what proves the MDX and the internal links (`onBrokenMarkdownLinks: throw`);
  every reference to `docs/` is an absolute GitHub URL, which `check-docs.py`'s third check is
  about.
- Considered for upstream: no. These are two pages of this platform's own end-user documentation —
  they describe orchestration (trunks, media control) that AGENTS.md rule 6 already places on this
  side of the boundary, and the sipx kernel has no site to carry them.
- Not done here, deliberately: the sidebar label still reads "Clustering (preview)" and is fenced
  to `DX-2`; no other page was touched, so the `intro.md` capability matrix rows for **Trunks** and
  **Media control** were left as they are (they already say `specified, not shipped`).

## Notes

- Specs: `docs/specs/number-normalisation.md`, `docs/specs/asserted-identity.md`,
  `docs/specs/media-relay.md`; designs: `docs/designs/routing-trunks.md`,
  `docs/designs/media-control.md`. Absolute GitHub URLs only.
- `rtpengine` is an allowlisted integration target and may be named as the relay this platform
  controls. It is a target, never a precedent.
- Stories: the `RT-*` and `ME-*` sets.
- Today media flows directly between endpoints because there is no relay — say so, rather than
  implying a relay exists.
