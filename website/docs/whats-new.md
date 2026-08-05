---
title: "What's new"
description: "Where sipx-clstr stands, release by release, and what is still missing."
---

# What's new

The current release is **0.13.0**.

## Where this actually is

**Two nodes sharing one registrar run.** They register users, forward calls between them — including
calls that cross from one node to the other — and let media flow directly between the endpoints. That
much is real, proved over real sockets by a **same-kernel, separate-process integration test** — the
phone on the other end is the `sipx` CLI, built from the kernel checkout this repository pins — and
you can have it working in about ten minutes, ending with a call you can hear: see
[Getting started](getting-started.md).

The next cluster layer now exists below the deployment seam: the affinity-token library passes every
byte vector, and a deterministic call round-trips its `Record-Route`/`Route` token through two edges
with zero cross-node dialog lookups. It is not deployable yet: configuration cannot load the keys,
there is no connection-owner delivery hop, and there is still no registrar shard, trunk, media relay
or operator.

Two things will bite you:

- **It is an open registrar.** Digest authentication is implemented, vector-proved and now *applied*
  from `tenant[].auth` — and there is still no way to configure a node that authenticates, because
  there is no user-credential store. A document asking for it either stops the node or challenges
  everyone into a `401` nobody can answer; a document not asking accepts any `REGISTER` for any
  address-of-record in a served domain. Do not put it on a public address.
- **One address in front of both nodes does not work.** Each node writes its own address into
  `Record-Route`, so in-dialog requests have to come back to the node that forwarded them. A single
  Service or VIP will send a `BYE` to whichever node the balancer picks, and that node knows nothing
  about the dialog. Affinity tokens are what fix this.

[Does this fit?](guides/does-this-fit.md) is the long version of this paragraph.

## What is still missing

Named, so nobody has to infer it from what the release notes happen to mention:

| | State |
|---|---|
| One address in front of the cluster | Route-token round trip proved; key loading and owner delivery not shipped |
| Flow ownership and the connection-owner RPC | specified, not shipped |
| Registrar sharding | specified, not shipped |
| Carrier trunks, number normalisation, asserted identity | specified, not shipped |
| Media relay control | specified, not shipped |
| Kubernetes operator, Helm, autoscaling | designed |
| A user-credential store, without which authentication is applied but cannot protect anything | specified, not shipped |
| Seventeen of the schema's sections — recognised, contents not validated, applied not at all | partly shipped |

**The measuring instrument now measures everything, and it now shows what it measures.** Six of the
specs in this repository used to carry vector tables the checker had no registration for — roughly
340 normative rows that nothing executed, and a fabricated row in one of those families passed the
gate untouched. Fifteen prefixes across thirteen specifications are now registered, and every file
under `docs/specs/` is either registered or named as a deliberate exclusion with the story that will
execute it: **169 of 598 rows proved, 19 covered for shape only, 410 deferred**, each deferral
naming what is specifically missing and the story that closes it.

Read that number for what it is. A row proves what a **decision core emits**, never that a socket
carried it — matched `CANCEL` and Timer C are produced as effects and dropped by the driver
(`PX-12`), and outbound target resolution returns a UDP target and refuses a hostname (`RT-12`).
Those two gaps close when their stories land, not when their rows do.

Two of those numbers went the "wrong" way on purpose. *Shape only* is a category `CF-12` created for
a row whose test runs but never compares the value the row states — a row that used to count as
proved and no longer does. And the report itself was printing only 138 of the rows it counted, so
seven whole families were in the denominator and in no table; `CF-17` made the sections cover the
count. A falling percentage here is the report becoming honest rather than the platform regressing.
[Conformance](reference/conformance.md) has the live numbers.

## Releases

Each entry leads with what changed for someone using this. The full detail — findings, rejected
alternatives, the reasoning behind each decision — is in
[CHANGELOG.md](https://github.com/codewandler/sipx-clstr/blob/main/CHANGELOG.md).

### 0.13.0 — affinity on the route, fail-closed at the edge

**M2's central rule is executable now, but not yet deployable.** Edge A mints the affinity pair into
`Record-Route`; a mid-dialog request reaches edge B, which has never seen the dialog, verifies the
pair and forwards with the cross-node dialog-lookup counter at zero. The token library proves every
byte vector, including rotation and tamper rejection. Membership, key distribution, shard mapping
and the connection-owner RPC are specified beside it; loading those keys and performing that owner
hop remain open.

The running node also became stricter in places an operator can observe:

- `ACK` and other in-dialog requests follow their dialog route instead of being looked up as
  registrations.
- Declared roles reach runtime dispatch, so a proxy-only node no longer accepts `REGISTER`.
- Unsupported `cluster.security` controls stop startup instead of appearing applied, and the default
  chart no longer declares controls this build cannot enforce.
- One REGISTER cannot submit an unbounded contact-operation set before reconciliation.
- The gate proves a completed call releases all transactions at the RFC-derived `128·T1` ceiling;
  the old, shorter threshold had labelled a legal lifetime a leak and caused a correct routing fix
  to be reverted once.

Two required kernel capabilities are filed, not released: proxy-usable outgoing CANCEL and exact
TCP-only listener selection. This release does not copy either into the platform; it keeps the real
driver fail-closed until a tagged sipx version carries them.

### 0.12.0 — the gate, pointed at itself

**Mostly a release about trust in the numbers on this site, not about new capability.** A third of
what changed was a check that could not see the thing it was believed to check — the conformance
report printed 138 of the 533 rows it counted, thirty-one normative rows sat in a file the gate never
opened, "this proof runs in CI" was decided by a substring match that a commented-out line would have
satisfied, and two checkers read prose *about* a directive as the directive. None of it was found by
a failure; all of it was found by reading each checker and asking what it would have to miss for the
report to stay green.

What you get from that: the end-to-end call — the evidence behind the claim that a call completes
with audio — now runs on every push against the kernel's own client, instead of being run by hand and
cited afterwards. And a row this site calls *proved* is now a row whose test compares the value the
row states.

Three changes you can actually use:

- **A node bounds how much work it holds at once.** `cluster.admission.maxInFlightTransactions`
  (default 1024) is taken on the accept loop, and over the bound a node answers `503` with
  `Retry-After` rather than accepting without limit. `REGISTER` and `ACK` are deliberately outside the
  bound: a registration storm *is* the overload, and shedding refreshes turns a spike into an outage.
- **The pinned sipx kernel moved three releases forward to `v0.10.0`**, which brought the declared
  Rust floor **down** to 1.91.
- **The `SipxCluster` custom resource is specified** — as one definition with the configuration
  schema rather than a second copy of it, with a check that fails when the two drift. There is still
  no operator: the resource is specified, not served.

Still true, and still the two things that will bite you: it is an open registrar, and one address in
front of both nodes does not work. Newly stated: the digest replay window costs `O(n)` per
authenticated request, and the challenge nonce is not unique per challenge. Both are in the kernel,
in a file that is byte-identical at every released kernel tag, so neither is fixable from here.

### 0.11.0 — two nodes, one registrar, and a call you can hear

**This is the release where "cluster" stops being a design document.** Two nodes sharing one
PostgreSQL location service: a user who registers through one node can be called through the other,
with audio. It is scripted both ways — `scripts/two-node-call.sh` for two local processes and
`scripts/k8s-two-node-call.sh` for two pods on Kubernetes — and both scripts print what they do not
prove.

- **A node is configured by a document**, not by flags. YAML, JSON or TOML, all producing the same
  configuration; the encoding is detected from the content. The old `--listen`/`--advertise`/`--tenant`
  flags are **gone**, replaced rather than extended.
- **Every mistake in a document is reported at once**, ordered by path, so five mistakes cost one
  restart instead of five. An unknown key is an error naming the keys that are recognised.
- **It refuses rather than improvises.** A location store it cannot reach stops the node instead of
  falling back to memory. A `transport: tls` listener is refused instead of quietly binding cleartext.
  A secret reference that does not resolve stops the node instead of being ignored.
- **[Getting started](getting-started.md) now ends with dialling in and hearing a tone** — through a
  node that never saw the answering phone register.
- Fixed: the node used to print `listening on` and *then* exit if its store was unreachable, so a
  script waiting for that line proceeded against a dying node.

**What still does not work:** one address in front of both nodes. Each node record-routes its own
address, so in-dialog requests must come back to it. Affinity tokens are what fix that and they are
not implemented. At this release authentication was still accepted by the document and not applied;
that has since changed — see [Where this actually is](#where-this-actually-is) for the current state.

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
- **A [configuration page](reference/configuration.md)** that stated plainly what the binary of the
  day did *not* read — at 0.10.0 there was no configuration file at all, only three flags — and what
  the schema replacing them would look like. 0.11.0 shipped that schema, and the page now describes
  the document the node actually reads.
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
