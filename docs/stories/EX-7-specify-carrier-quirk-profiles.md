---
id: EX-7
title: Specify carrier quirk profiles
pillar: Platform
status: backlog
priority: 
design: docs/designs/extension-framework.md
epic: extension-framework
areas: [hooks, trunks]
note: 
---

# Specify carrier quirk profiles

## Goal
Give per-peer protocol quirks — header injection and SDP body rewriting — a bounded, declarative vocabulary, so accommodating a carrier is configuration with a test vector rather than a patch.

## Acceptance
- [ ] A quirk profile is data: which peers it applies to, which headers it adds on which methods, and which SDP rewrites it performs.
- [ ] The vocabulary is bounded — it must not become general scripting embedded in config.
- [ ] Profiles attach to a trunk or a domain, and several may apply.
- [ ] Each shipped profile carries a test vector; adding one is a config change plus a vector.
- [ ] Interaction with media policy (e.g. a quirk that also implies SRTP) is specified.

## Progress
- (not started)

## Notes
- babelforce has one live example needing `mediasec` headers on REGISTER and INVITE plus an SDP `a=sendrecv` rewrite. It is currently an inline domain test in the routing script.
- The requirement is the mechanism, not that specific carrier — more will follow.
- Filed from the babelforce-sip-clstr deployment (`~/babelforce/projects/babelforce-sip-clstr`), whose capability inventory records this as `upstream`. Requirement **U-5** in that repo's `docs/upstream.md`; evidence in its `docs/reference/environments.md`.
