---
id: RT-11
title: Decide whether a trunk may unconditionally strip a standard header, against the caller's request
pillar: Signalling
status: backlog
priority:
design: docs/designs/routing-trunks.md
epic: routing-trunks
areas: [routing, trunks, privacy]
note: RT-7 §14 records this as a known gap, not a deferral — and it is a product question first
---

# Decide whether a trunk may unconditionally strip a standard header, against the caller's request

## Goal
Answer, and record, whether this platform offers an operator a way to guarantee that a given carrier
**never** receives a `P-Asserted-Identity` — including when the caller sent `Privacy: none`.

Today nothing does. [asserted-identity](../specs/asserted-identity.md) `A19` explains why no field in
that spec can be it: RFC 3323 §4.2 says twice that an intermediary `MUST NOT` remove or alter a
`Privacy` header whose priv-value is `none`, and RFC 3325 §7 makes the identity travel under it. So
`A22` emits, whatever the trunk's policy says, and `AI-T-8` pins that it does.

**This is a decision story before it is an implementation story.** The honest reason it is open is
not that the work is unbuilt — it is that building it means offering an operator a switch that
knowingly violates a `MUST NOT` in order to honour a carrier contract over a caller's explicit
request. That trade is the deployment's to make and the platform's to make visible; it is not
obviously one we should offer at all.

## Acceptance
- [ ] The question is **decided and written down**, either way. "Not yet built" is not an outcome;
      "we decline to offer this, and here is the argument" is.
- [ ] If offered: a per-trunk unconditional egress removal for **standard** SIP headers, separate
      from RT-5's allowlist rather than an extension of it. RT-5 is default-deny over the
      platform's application-header prefix
      ([RT-5](RT-5-implement-per-trunk-egress-header-allowlist.md) acceptance 1), so its field set
      and this one are disjoint — they compose without a precedence rule, and that should stay true.
- [ ] If offered: the RFC 3323 §4.2 deviation is recorded the way `A23`'s §7 deviation is — named,
      argued, and scoped to the one direction it runs in. A deviation from a `MUST NOT` that is not
      written down is the failure mode `AGENTS.md` non-negotiable #1 exists to prevent.
- [ ] If offered: it is written where a reviewer reading the config sees an *unconditional header
      removal*, not inferred from a privacy default. That placement is the whole point — `A19` says
      so, and it is why this is not a field on `PrivacyEgress`.
- [ ] Either way, [asserted-identity](../specs/asserted-identity.md) §14's last row is updated to
      name the outcome instead of naming nobody.

## Progress
- (not started)

## Notes
- **Filed 2026-07-29 at wave 4's integration**, from `RT-7` §14. `RT-7` recorded the gap rather than
  filing it, correctly — the board and story IDs are the coordinator's, not an implementor's.
- The gap was found the hard way. `RT-7`'s first rework closed the original finding by claiming
  RT-5's allowlist provided this guarantee; review established that RT-5 is prefix-scoped and cannot
  touch `P-Asserted-Identity`, so the claim was false in three places (`A19`, `A33`, `AI-A-16`). The
  second rework replaced it with the true statement — nothing provides it — which is what turned an
  incorrect cross-reference into this story.
- Worth resisting the obvious shortcut: widening RT-5's prefix to cover standard headers would close
  this on paper and would be wrong. RT-5's default-deny is safe *because* its field set is bounded;
  a default-deny over all standard SIP headers would strip `Via` and `Route` and break routing.
