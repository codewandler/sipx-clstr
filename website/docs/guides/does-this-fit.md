---
title: "Does this fit?"
description: "The honest answer — what sipx-clstr is for, what it deliberately will never do, and how far along it actually is today."
---

# Does this fit?

Read this before you build anything on it. The short version: **the design is a clustered SIP proxy
and registrar; the implementation today is two nodes sharing one registrar, addressed individually.**

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
| Configuration by one cluster-scoped document; served domains, binding quota, expiry bounds and an in-flight bound all enforced from it | **today** |
| Durable, shared (PostgreSQL) location store — selected from the document | **today**, with the non-default `postgres` cargo feature compiled in; a node asking for the store without it refuses to start |
| Two nodes sharing one registrar: register through one, be called through the other | **today** — scripted as two local processes and as two pods on Kubernetes |
| Digest authentication | implemented, proved and applied from the document — but **no user-credential store**, so a document asking for it is refused or challenges nobody |
| — | — |
| One address in front of the nodes | specified, not shipped |
| Affinity tokens, flow ownership | specified, not shipped |
| Trunks, number normalisation, asserted identity | specified, not shipped |
| Media relay control | specified, not shipped |
| Kubernetes operator, Helm, autoscaling | designed |
| Queues, IVR, conference | not planned for the core |

The two rows to read twice are the last two. The location store **is** reachable now — name it in the
document and build with `--features postgres` — so registrations can be durable and shared. Digest
authentication is reachable in the same sense and still buys you nothing: with no user credentials, a
document declaring `tenant[].auth` either stops the node or challenges every `REGISTER` into a `401`
nobody can answer. So the node you can run today is an **open registrar**, and the only thing
protecting it is the network it is on. Do not expose it.

## Decided, but not performed

There is a second line inside "today", and it is the one that will surprise you. The decision logic
in this platform is sans-IO — pure state machines that emit *effects* — and the driver that turns an
effect into a packet is a separate piece of code. Where a core decides something the driver does not
yet perform, the vector rows go green and **nothing reaches the wire**. A conformance row proves what
the engine emits; it never proves that a socket carried it.

Four of those, named with the work that closes each:

| Decided by the core | What the released node actually does | Closes with |
|---|---|---|
| Matching a `CANCEL` to its forked branches, and arming Timer C | The `CancelBranch` effect is logged and dropped; `SetTimer` reaches no clock, so a Timer C is armed with the right value and never fires. A branch that goes quiet after a provisional is reaped by the kernel's Timer B or not at all | `PX-12` |
| Selecting an outbound target and its transport | Outbound resolution returns a UDP target and refuses a hostname outright — no RFC 3263, no NAPTR/SRV, no transport choice | `RT-12` |
| The full role-to-capability matrix | Roles *do* reach dispatch: a node derives its capability set from them and answers `405` with `Allow` for a method they do not wire, and refuses to start on a role it has no runtime for. The refusal shape (`503`/`481`), the counted `ACK` drop and an `echo` runtime are not built, and the matrix has no real-binary proof yet | `DP-13` |
| The probe establishing and tearing down a real dialog | The probe still builds address-of-record-shaped `ACK` and `BYE`, so a passing run proves a second lookup rather than a dialog route. The *node's* in-dialog path is not affected: it follows the dialog's `Route` set and remote target | `ET-7` |

None of these moves because a vector row closes. Each moves when its story's real-socket acceptance
passes, and the gate refuses to let this site say otherwise before then.

## Where the claims come from

Correctness is expressed as numbered vectors inside the normative specs — for example the
forwarding rules carry a `PB-*` table, the location service an `LS-*` table. A generated report
says how many are proved by a test, and names the work that will close every one that is not.

That report is the honest capability statement for this project, and it is deliberately allowed
to say "no". See [Conformance](../reference/conformance.md), and the specs themselves in
[docs/specs](https://github.com/codewandler/sipx-clstr/tree/main/docs/specs).
