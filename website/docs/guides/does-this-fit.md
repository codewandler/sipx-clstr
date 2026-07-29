---
title: "Does this fit?"
description: "The honest answer — what sipx-clstr is for, what it deliberately will never do, and how far along it actually is today."
---

# Does this fit?

Read this before you build anything on it. The short version: **the design is a clustered SIP
proxy and registrar; the implementation today is one node.**

## It fits if you want

- A **proxy-first** signalling layer: it forwards requests and never terminates dialogs.
- A registrar whose bindings are held in a strongly consistent store, so a re-`REGISTER` cannot
  silently land somewhere that does not know about the last one.
- A platform where **no shared call state** lives in the cluster — the information needed to
  route the next request rides in the message, signed.
- Media that flows directly between endpoints, or through a relay controlled over a network
  protocol — never through the signalling process.
- Correctness you can check rather than trust: numbered test vectors per normative rule, and a
  generated report of which are proved. See [Conformance](../reference/conformance.md).

## It does not fit if you want

- **A PBX.** No queues, no IVR, no conference, no voicemail, no dialplan. Those belong to an
  optional B2BUA service built *with* the platform rather than inside it, and that service is not
  scheduled yet.
- **A routing scripting language.** Routing policy is composed from typed modules with declared
  dependencies. There will not be a config DSL — that is a stated non-goal, not an omission.
- **RTP handled by the signalling process.** Never, by design. Media is relayed by an external
  process under this platform's control, or not at all.
- **Established calls that survive losing their node**, at least not in v1. The guarantee is
  *service* HA: new calls and registrations succeed after a node loss. Calls in progress
  surviving the loss of their signalling node is an explicit later opt-in, and the project's
  vision says plainly that it is "never silently promised".
- **An RFC-complete implementation.** Coverage is selected by profile and tracked per normative
  requirement; "not implemented" is a recorded status rather than a hidden gap.

## The state of it, precisely

Everything below the line is specification, not software.

| | Status |
|---|---|
| One node: proxy + registrar, UDP and TCP | **today** |
| Registrations, forwarding between registered users, direct media | **today** |
| Digest authentication | implemented and proved, **not reachable from the CLI** |
| Durable (PostgreSQL) location store | implemented and tested, **not reachable from the binary** |
| — | — |
| More than one node cooperating | specified, not shipped |
| Affinity tokens, flow ownership | specified, not shipped |
| Trunks, number normalisation, asserted identity | specified, not shipped |
| Media relay control | specified, not shipped |
| Kubernetes operator, Helm, autoscaling | designed |
| Queues, IVR, conference | not planned for the core |

The two "not reachable" rows are the ones that surprise people. The code exists and passes tests;
what is missing is the configuration surface that would let you switch it on. Until that lands,
the binary is an **open registrar with in-memory bindings**. Do not expose it.

## Where the claims come from

Correctness is expressed as numbered vectors inside the normative specs — for example the
forwarding rules carry a `PB-*` table, the location service an `LS-*` table. A generated report
says how many are proved by a test, and names the work that will close every one that is not.

That report is the honest capability statement for this project, and it is deliberately allowed
to say "no". See [Conformance](../reference/conformance.md), and the specs themselves in
[docs/specs](https://github.com/codewandler/sipx-clstr/tree/main/docs/specs).
