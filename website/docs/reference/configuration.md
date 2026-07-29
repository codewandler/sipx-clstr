---
title: "Configuration"
description: "What configures a node today, what does not yet exist, and the cluster configuration schema that replaces it."
---

# Configuration

**There is no configuration file today.** Not an empty one, not an optional one, not one with a
default path. A node is configured by three command-line flags and one environment variable, and
that is the entire surface.

This page is the inventory: what you can set, what is built but you cannot reach, and what the
schema that replaces all of it looks like.

## What configures a node today

| | Where | Default |
|---|---|---|
| The address to bind | `--listen <addr>` | `0.0.0.0:5060` |
| The address peers reach it on | `--advertise <host[:port]>` | the bound address |
| The tenant name | `--tenant <name>` | `default` |
| Log level | `RUST_LOG` | `info` |

That is the complete list. See the [CLI reference](cli.md) for each one, its refusals and its
exit codes.

Everything else a SIP platform normally has a knob for — realms, credentials, expiry windows,
timers, trunks, rate limits, `Max-Forwards`, NAT handling, a metrics endpoint — has no knob here.
Most of it is specified in detail. None of it is reachable from the binary.

## Implemented, and unreachable from the binary

Two of those gaps are not "unwritten". They are working, tested code that the command line has no
way to switch on. This is the part most likely to surprise you, so it is stated plainly.

### Digest authentication

The registrar implements digest authentication and it is proved against the RFCs' own test
vectors: a challenge in a named realm, a nonce with a replay window, and `stale=true` to recover
an expired one in a round trip rather than a re-login. It offers one algorithm rather than a menu,
because a modern algorithm offered beside a weak one invites a downgrade (RFC 8760 §3). A tenant's
policy is either open or requires credentials.

**The binary always chooses open.** The tenant's authentication configuration is a field the
command line never sets, so it is always absent, and absent means an open tenant. There is no
flag, no environment variable and no file that changes it.

So the node you build today accepts any `REGISTER` for any address-of-record from anyone who can
reach the port. `--tenant` does not change this; it names the tenant, and that tenant has no
credentials.

:::danger Do not put this on a public address
An open registrar on the internet is a free relay. Keep it on loopback or a trusted network until
the configuration schema lands.
:::

### The PostgreSQL location store

The durable location store is real: it implements the same store contract as the in-memory one and
runs the same shared conformance suite, with compare-and-swap on a per-address-of-record revision.
It lives behind a cargo feature.

Turning the feature on compiles it in and changes nothing about how the node is configured:

```console
$ cargo build --bin sipx-clstr --features postgres
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 24.50s
$ ./target/debug/sipx-clstr --help
sipx-clstr 0.10.0 — a clustered SIP proxy and registrar

One node registers users and proxies calls between them. The cluster — affinity
tokens, trunks, media control — is specified but not implemented; see the docs.

  run --listen <addr>   run a node: registrar and proxy on one listener
      --tenant <name>            the tenant this node serves (default:
                                 `default`); registrations are not
                                 authenticated — this node is an open
                                 registrar, and bindings are lost on restart
      --advertise <host[:port]>  what peers reach it on, if that is not the
                                 address it binds — a node behind a NAT or on a
                                 private address must say so, or its Via and
                                 Record-Route name somewhere unreachable
  --version             print the version and the kernel it is built against
```

Identical help, and no connection string to give it. The node constructs the in-memory store
unconditionally. **Every registration is lost when the process restarts**, and there is no second
node that kept them.

Both gaps have the same single cause: the configuration surface is three flags, and the schema
that would carry a realm or a connection string has not been implemented yet.

## The schema that replaces this

The replacement is written and normative — the code is what is missing, not the decision. It is
worth reading before you design a deployment around the flags, because the flags are going away.

**[The cluster configuration spec](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/cluster-config.md)
is the contract.** What follows describes its shape so you know what is coming; it is not a
restatement, and where the two differ the spec is right.

**One document describes a whole cluster.** Not one file per node. Every node reads the same
bytes, projects them through the identity it was started with — its node id, its zone, its roles —
and either starts or refuses. Identity comes from outside the document, because the document is
the same everywhere; a section belonging to a role this node does not run is dropped, not
rejected.

That is the property everything else rests on: "which configuration is this cluster running" has
one answer, and a change between two versions is reviewable as a single diff.

**Two encodings, one data model.** YAML or JSON, your choice — the normative artefact is the typed
tree, and the same tree in either spelling must load identically. Keys are `lowerCamelCase`;
enumerated values are `kebab-case`. That convention is mechanical rather than aesthetic: the same
tree is the Kubernetes custom resource.

**Two version numbers, meaning different things.** One says which schema version the document is
written against; a node refuses a version it does not fully implement rather than parsing it
best-effort. The other says which configuration the document *is*, and it only ever goes up —
rolling back means publishing the old content under a higher number.

**Closed world.** An unrecognised key is an error, naming the path and what is recognised there.
Not a warning, not ignored. A quota field with a typo in it is a quota nobody is enforcing, and
nothing anywhere would say so.

**Refusing to start is the only failure mode.** There is no partial application, no degraded mode,
no "carry on with the last good value for that field". A load reports *every* error rather than
the first, ordered so two runs over one document print the same thing — five mistakes cost one
restart, not five.

**No secret is ever written in the document.** Key material, database credentials and TLS private
keys are named by reference and resolved by the node at startup, so the safe shape is the only
shape. The one substitution available is `${NAME}` from the environment, and an undefined name is
an error rather than an empty string.

**Some of it reloads, some of it does not.** Which is which is declared per field in the spec, not
decided by diffing at runtime, so a node and an operator classify a change the same way. Trunks,
tenants, routing and observability reload; listeners and storage backends want a restart. No
reload disturbs an established dialog — a dialog's route set is not recomputed, and everything a
mid-dialog request needs already rides in the message.

**What it will not let you express** is as deliberate as what it will: no includes, no templating,
no expression language, no per-node conditionals, and no regex or selector anywhere. A
configuration that computes cannot be diffed, and being diffable is the point.

### What that means for the flags

`--listen` and `--advertise` survive as concepts, not as flags: the same bind-versus-advertise
split becomes a listener entry in the document, with the same refusals — an unspecified address
or a port of `0` cannot be advertised, and an omitted advertised port means the port bound. That
much of today's behaviour is already the schema's, which is why
[Addressing](../guides/addressing.md) is worth reading now.

`--tenant` does not survive. Tenants become a list, each with its own realm, credential source,
expiry policy and binding quota.

## The Helm chart's values are not the schema

If you have found
[`deploy/helm/values.yaml`](https://github.com/codewandler/sipx-clstr/blob/main/deploy/helm/values.yaml),
its `cluster:` tree looks like the configuration document, and its header says so. **Do not treat
it as the schema.** It was written before the spec and the two have diverged.

The divergences are real, not cosmetic:

- It sets `security.maxForwards` to `10`, where the schema fixes **70**. That value is what a
  proxy inserts when a request arrives without the header (RFC 3261 §16.6 step 3) — not a hop
  budget to tune downward, and `10` quietly shortens every call path that arrives without it.
- Its media block is cluster-wide, where media policy belongs to a trunk, and the codec policy it
  declares is one the media specification refuses to start on.
- Sections sit under names and shapes the schema does not use: `numbering` rather than named
  normalisation profiles bound per trunk, `limit` and `limits` rather than rate limits and timers,
  per-tenant registrar values under `registrar` rather than under a tenant list, a single `shards`
  count rather than a shard map.
- Enumerated values are spelled `snake_case` — `strip_plus`, `destination_number`,
  `location_store` — where the schema fixes `kebab-case`.

Reconciling the chart with the spec is open work. Until it is done, the spec is the schema and
the chart is a work in progress. Nothing runs it in
any case: there is no operator image and no custom resource yet, so the chart renders a resource
nothing serves.

## Where this leaves you

Configure what you can: a bind address, an advertised address, a tenant name, a log level. Assume
the node is an open registrar with volatile state, and place it accordingly. Read the spec if you
are planning a deployment, and do not build tooling on the flags —
[the CLI reference](cli.md) says why.
