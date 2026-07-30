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

A `dsnRef` that does not resolve stops the node. It is never treated as absent. The one reference that
behaves differently is `tenant[].auth.secretRef`, and only because there are no credentials behind it
yet — see [Authentication](#authentication-applied-and-still-not-usable) below.

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

## The tenant, and what it enforces

The tenant is not just a label. Three of its fields reach the registrar:

```yaml
tenant:
  - name: default
    id: 1
    domains: [example.test]
    maxBindingsPerAor: 5
    expiry:
      default: 3600
      min: 60
      max: 86400
```

- **`domains`** are served domains. A `REGISTER` whose address-of-record is in any other domain is
  answered **`403`** — well-formed, understood, declined. Comparison is byte-exact, because folding
  case here would make two domains one. A tenant that declares **no** `domains` serves any.
- **`maxBindingsPerAor`** is a quota, enforced before the store is touched. `0` is refused: a tenant
  that can register nothing is a disabled tenant spelled as a limit.
- **`expiry`** bounds the granted lease. A `min` above the `max` is refused rather than reordered.

Absent keys keep the location service's own defaults rather than restating a different one here.

## Authentication: applied, and still not usable

`tenant[].auth` is **applied** — it is not on the ignored list below. That does not mean you can have
an authenticated registrar, because there is no user-credential store yet. The three states, exactly:

| The document | What the node does |
|---|---|
| no `auth` block | An **open** tenant, deliberately. Startup logs `auth="open"`. |
| `auth` whose `secretRef` **resolves** | **Refuses to start**, exit `2`. Running `required` against an empty credential store would answer `401` to every `REGISTER` while looking protected, which is worse than open because nothing says so. |
| `auth` whose `secretRef` does **not** resolve | Starts, logs `auth="required"`, and challenges every `REGISTER` with a `WWW-Authenticate` in the document's realm. Nobody can answer it — so it registers nobody. |

```yaml
tenant:
  - name: default
    id: 1
    domains: [example.test]
    auth:
      realm: example.test
      secretRef: nonce-secret     # the nonce secret, named — never written here
```

Credentials themselves are **refused** in the document: a `credentials` or `users` key under `auth` is
a load error, because where users come from is not this schema's to invent. An inline `secret` is
refused for the same reason a `dsn` is.

So today the honest reading is: this build has one usable posture, open, and declaring authentication
buys a refusal rather than protection. Keep the node off any address you do not control.

## The admission bound

```yaml
admission:
  maxInFlightTransactions: 1024   # the default
```

A ceiling on server transactions in flight at once. Above it a request is answered `503` with a
`Retry-After` rather than queued forever — a bound that is stated is one an operator can reason about.
Startup logs the effective value.

## Sections accepted but not yet applied

This is the part worth reading before you trust a document. The rest of the schema's registry is
**recognised** — naming one is legal, and a typo in the name is still an error — but its contents are
not validated and **nothing applies them**.

| Section | State |
|---|---|
| `name`, `environment`, `zones`, `listener`, `membership`, `locationStore`, `tenant` (including `auth`), `security`, `admission`, `timers` | validated and applied |
| `profile`, `management`, `keys`, `shardMap`, `registrar`, `normalisation`, `trunk`, `domain`, `destinationSet`, `routeRule`, `ingress`, `rateLimit`, `nat`, `mediaPool`, `observability`, `probe`, `echo` | recognised, not validated, not applied |
| a listener's `tls`, `maxConnections`, `connectionLifetime` | recognised, not applied — the same state, one level down |

A node logs a warning naming the **paths** it recognised but did not apply, and gives the
security-relevant ones a line of their own, so this is visible at startup rather than inferred.

**Treat any of those as documentation of intent, not as configuration.**

## The normative schema

This page describes what the loader does. The
[specification](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/cluster-config.md) is
the contract — every rule above has an id there, the refusals quote those ids, and the spec covers the
sections this build does not yet apply.

Do **not** read `deploy/helm/values.yaml` as the schema. It was a real proposal and much of it was
adopted, but where it disagreed with the spec the spec won, and reconciling the chart is open work.
