---
title: "What's new"
description: "Where sipx-clstr stands, release by release, and what is still missing."
---

# What's new

The current release is **0.10.0**.

## Where this actually is

**One node runs.** It registers users, forwards calls between them, and lets their media flow
directly. That much is real, tested against independent client software, and you can have it
working in about five minutes — see [Getting started](getting-started.md).

Everything else on this site is specification. The cluster does not exist yet: there is no second
node, no affinity token in a `Record-Route`, no registrar shard, no trunk, no media relay, no
operator.

Two things about the shipped binary will bite you, and both are consequences of the same missing
piece — the configuration surface is three command-line flags:

- **It is an open registrar.** Digest authentication is implemented and vector-proved, and there
  is no way to switch it on from the CLI. The binary accepts any `REGISTER` for any
  address-of-record from anyone who can reach the port. Do not put it on a public address.
- **Bindings are in memory only.** A restart loses every registration, and there is no second node
  that kept them. The PostgreSQL location store is real, tested code sitting behind a cargo
  feature that the binary does not reach.

[Does this fit?](guides/does-this-fit.md) is the long version of this paragraph.

## What is still missing

Named, so nobody has to infer it from what the release notes happen to mention:

| | State |
|---|---|
| A second node cooperating with the first | specified, not shipped |
| Affinity tokens and flow ownership on the wire | specified, not shipped |
| Registrar sharding | specified, not shipped |
| Carrier trunks, number normalisation, asserted identity | specified, not shipped |
| Media relay control | specified, not shipped |
| Kubernetes operator, Helm, autoscaling | designed |
| A configuration file — the schema exists, nothing reads it | specified, not shipped |
| Digest auth reachable from the CLI, durable bindings in the binary | implemented, not wired |

One more gap that is easy to miss because it is about the measuring instrument rather than the
product: **most of the platform's normative vector rows are not under the conformance gate yet.**
Six of the specifications carry tables the checker has no registration for, so those rows are
prose that nothing executes. [Conformance](reference/conformance.md) says which, and why it
matters.

## Releases

Each entry leads with what changed for someone using this. The full detail — findings, rejected
alternatives, the reasoning behind each decision — is in
[CHANGELOG.md](https://github.com/codewandler/sipx-clstr/blob/main/CHANGELOG.md).

### 0.10.0 — documentation you can actually start from

**This site.** Until this release the published documentation was the project's own internal
material — the roadmap, thirteen design records, ten specifications, a generated coverage report —
and the landing page still said that nothing forwards a SIP message, which stopped being true five
releases ago. There was no install page, no quickstart, no configuration guide and no command
reference.

- **[Getting started](getting-started.md) reaches a forwarded call** with a Rust toolchain and
  Python, and nothing else. No PBX, no account, no softphone to build.
- **A [command reference](reference/cli.md)** whose every flag, message and exit code was produced
  by running the binary rather than read off the source.
- **A [configuration page](reference/configuration.md)** that states plainly there is no
  configuration file yet, and what the schema replacing these flags will look like.
- **[Migration concept maps](migrate/from-kamailio.md)** for people arriving from an existing
  deployment, including what does not carry over.
- **Clustering and operations are documented as unshipped**, marked in the navigation and again on
  every page, so the gap between the design and the software is visible rather than inferred.
- Also fixed: `--help` claimed no roles were implemented, four releases after they were, and never
  mentioned `--tenant`.

### 0.9.0 — three specifications, and a check on whether they say what they mean

**Nothing in this release is executable.** It is three normative specs, two of which were returned
for rework because they claimed more than they delivered.

- **If you will terminate carrier trunks, the asserted-identity and privacy contract is now
  readable before you have to configure it.** Trust, assertion and privacy are per-trunk policy,
  and each privacy level is either performed in full or **declined with a reason** — the first
  draft advertised a level and performed a third of it.
- **There is now one configuration schema instead of four disagreeing dialects.** It was
  reconciled against the Helm chart and the node rather than invented beside them, which turned up
  a chart default the platform would refuse to boot on. Reading it tells you what a config file
  will look like when one is finally accepted.
- **The default `Max-Forwards` becomes 70.** RFC 3261 §16.6 makes it the value inserted when a
  request carries none, not a hop budget — the old `10` silently shortened every path that arrived
  without the header.
- **The conformance gate learned to see the hook-framework rows**, taking the report to *77 of 98
  rows proved, 21 deferred* at this release. The honest caveat shipped with it: the two specs this
  release added are not under the gate at all. Live numbers:
  [Conformance](reference/conformance.md).

### 0.8.0 — the Rust you need to build this

- **You need Rust 1.94.** The declared minimum was 1.88 and it never worked — on 1.88 the
  workspace did not compile at all, so nothing that previously built stops building. The floor was
  established by bisecting rather than by reasoning, and the gate now builds on it so the number
  and the truth cannot drift apart again.
- **One more registrar-auth row moved from asserted to proved**, closing a claim that shipped
  unproved in 0.7.0.

### 0.7.0 — the address a node tells the world about

- **A node advertises the address you give it, not the address it bound.** Until this release a
  container binding `0.0.0.0` told every peer to answer `0.0.0.0`. `Via`, `Record-Route` and
  `Contact` now carry the advertised address, chosen per listener.
- **A node that would advertise an unspecified address refuses to start**, exiting 2 and naming
  the address. This is a deliberate break in the CLI's default path: the failure it replaces was
  silent and only visible from the far end.
- **The platform runs outside a test** — a `Dockerfile` and a scripted demo stand a node up, two
  users register, and a call is forwarded between them. See
  [Docker and k3d](guides/docker-and-k3d.md).
- Specifications for carrier quirk profiles, number normalisation, and per-trunk codec and SRTP
  policy — with every defect independent review found in them fixed before release rather than
  shipped documented.

### 0.6.0 — a retransmitted `REGISTER` stops failing

- **A `REGISTER` retransmitted after a lost `200` is answered `200` again, not `500`.** Over UDP
  this happened every day. The idempotency rule had been comparing absolute deadlines, so it held
  only for a retry arriving in the same nanosecond as the original; it now compares the granted
  duration.
- Specifications for flow references and connection ownership, and for the media relay and its
  external control protocol — including the decision that SDP stays opaque bytes end to end.

### 0.5.1 — the documentation site could not deploy

- **The site is published again.** It had been failing since 0.4.0 on links that resolved on disk
  and 404'd once published, and the gate has been taught the difference.

### 0.5.0 — M1 complete

- **Digest authentication is wired into the registrar's decision path**, so a challenged request
  never becomes something the store could write. Reachable from the node driver's configuration —
  and still not from the CLI, which is why the binary is an open registrar today.

### 0.4.0 and earlier

The foundations, in one line each: a real call between two independent phones through one node
with audio proved rather than assumed (0.4.0); the proxy forwarding core with forking, `CANCEL`
and Timer C; the location service with its compare-and-swap store, in memory and on PostgreSQL;
the deterministic simulation harness with virtual time and seeded faults; the end-to-end probe;
and before any of it, the normative specifications the rest is derived from.

Full history: [CHANGELOG.md](https://github.com/codewandler/sipx-clstr/blob/main/CHANGELOG.md).

## How releases work

The site you are reading is deployed from a **published release**, not from the last push, so what
it says matches a tagged version rather than whatever landed an hour ago. Versions are semantic,
and while the platform is pre-1.0 a minor bump can move behaviour — 0.7.0's refusal to start on an
unspecified advertised address is the kind of change to expect.
