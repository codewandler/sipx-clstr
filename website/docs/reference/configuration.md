---
title: "Configuration"
description: "One cluster-scoped document in YAML, JSON or TOML — what a node reads, what it refuses, and which sections are accepted but not yet applied."
---

# Configuration

A node is configured by **one document, shared by the whole cluster**, plus an identity supplied from
outside it. There are no configuration flags: the schema replaced them rather than being added beside
them, because two configuration surfaces is the thing the schema exists to remove.

## The shape of it

```yaml title="cluster.yaml"
apiVersion: sipx.dev/v1alpha1
version: 1
cluster:
  name: local
  environment: dev
  zones: [a]
  listener:
    - roles: [edge, registrar]
      transport: udp
      bind: 0.0.0.0:5060
      advertise: ${POD_IP}:5060
  membership:
    - node: 1
      name: node-a
      zone: a
      roles: [edge, registrar]
    - node: 2
      name: node-b
      zone: a
      roles: [edge, registrar]
  locationStore:
    backend: postgres
    dsnRef: location-dsn
  tenant:
    - name: default
      id: 1
      domains: [example.test]
```

**YAML, JSON and TOML are all accepted**, and the same document in any of the three produces exactly
the same configuration — a test asserts that, comparing the whole parsed result rather than a field.
The encoding is detected from the content, never from the file name: a document renamed is not a
document changed.

Keys are `lowerCamelCase`; enumerated values are `kebab-case`.

## One document, many nodes

Every node reads the same bytes. Two things make that work:

**`${VAR}` substitution.** The only substitution there is: `${NAME}` where `NAME` matches
`[A-Z_][A-Z0-9_]*`, resolved from the environment. No nesting, no defaults, no arithmetic. An
undefined name is an error naming the variable — never the empty string, which would turn
`advertise: "${POD_IP}:5060"` into an unparsable address and report the wrong problem.

**Identity from outside.** The node id, zone and role set are given on the command line or in the
environment, never in the document. If the document *does* carry a `membership` entry for this node,
it is **cross-checked** against what the node was started with and a mismatch is refused; a node with
no entry still starts, because a node whose pod the operator has not yet published should not
crash-loop.

## What it refuses, and why it only refuses

**Refusing to start is the only failure mode.** No partial application, no degraded mode, no
"continue with the last good value". An operator learns one failure behaviour.

- **An unknown key is an error**, not a warning, and the refusal names the keys that *are* recognised
  at that path. `maxContact` for `maxContacts` silently dropped is a quota nobody is enforcing with
  nothing anywhere saying so.
- **Every** error is reported, ordered by path — five mistakes cost one restart, not five. Two runs
  over one document print byte-identical output.
- **A transport this build cannot serve is refused, never downgraded.** A listener declaring
  `transport: tls` does not silently bind cleartext UDP. It did for one commit, and a deployment
  asking for encrypted signalling got plaintext with nothing saying so.
- **Where an RFC fixes a value, there is no knob.** `maxForwards` is 70 (RFC 3261 §16.6) and is not
  offered as a hop budget to tune downward.
- **Timer C must exceed three minutes** (RFC 3261 §16.6 step 11). It is not `maxCallDuration`;
  conflating them produces a Timer C set to hours in the belief that it protects long calls.

## Secrets are named, not written

No credential appears in the document. `dsnRef`, `secretRef` and `keyRef` name one, and the driver
resolves it from the environment — `dsnRef: location-dsn` reads `LOCATION_DSN`. Resolution is I/O, so
it happens outside the parser, which stays a pure function of bytes, identity and environment.

A reference that does not resolve stops the node. It is never treated as absent.

## The location store

| `backend` | What it means |
|---|---|
| `memory` | Process-local. Bindings die with the node, and no peer can see them. Correct for one node. |
| `postgres` | The shared location service. **Required for more than one node** — this is what makes two nodes one registrar. |

A `postgres` store that cannot be reached **stops the node**. It does not fall back: a registrar that
came up healthy, answered `200` to every `REGISTER`, and served bindings no peer can see would be
worse than one that refused to start, because nothing would say so.

The `postgres` cargo feature has to be compiled in — `cargo build --features postgres`. The container
image includes it.

## Sections accepted but not yet applied

This is the part worth reading before you trust a document. The loader validates ten sections. The
rest of the schema's registry is **recognised** — naming one is legal, and a typo in the name is
still an error — but its contents are not validated and **nothing applies them**.

| Section | State |
|---|---|
| `name`, `environment`, `zones`, `listener`, `membership`, `locationStore`, `tenant`, `security`, `timers` | validated and applied |
| `tenant[].auth` | **accepted and not applied** — the node is an open registrar regardless |
| `profile`, `management`, `keys`, `shardMap`, `registrar`, `normalisation`, `trunk`, `domain`, `destinationSet`, `routeRule`, `ingress`, `rateLimit`, `nat`, `mediaPool`, `observability`, `probe`, `echo` | recognised, not validated, not applied |

A node logs a warning naming the sections it recognised but did not apply, so this is visible at
startup rather than inferred. It is being made fail-closed rather than fail-open: a document declaring
authentication should be refused by a build that cannot enforce it, not accepted silently.

**Until then, treat any of those sections as documentation of intent, not as configuration.**

## The normative schema

This page describes what the loader does. The
[specification](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/cluster-config.md) is
the contract — every rule above has an id there, the refusals quote those ids, and the spec covers the
sections this build does not yet apply.

Do **not** read `deploy/helm/values.yaml` as the schema. It was a real proposal and much of it was
adopted, but where it disagreed with the spec the spec won, and reconciling the chart is open work.
