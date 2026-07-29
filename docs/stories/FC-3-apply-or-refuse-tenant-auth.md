---
id: FC-3
title: Apply or refuse tenant[].auth, so a document that asks for authentication cannot yield an open registrar
pillar: Cluster
status: ready
priority: 3
design: docs/designs/fail-closed-config.md
epic: fail-closed-config
areas: [registrar, security, deploy]
note: DP-10 deliberately declined to fold this in — it is the security-behaviour change that wants its own test
---

# Apply or refuse `tenant[].auth`, so a document that asks for authentication cannot yield an open registrar

## Goal

Close the gap `RG-2` left and `DP-10` deliberately declined to close: digest authentication is
implemented, proved against the RFCs' own vectors, and unreachable. A document declaring
`tenant[].auth` loads clean and the node runs `TenantAuth::open`, so the operator who configured
authentication gets an open registrar and nothing says so.

## Acceptance

- [ ] `tenant[].auth` is projected and consumed, or the document is refused when it is present.
      `TenantSpec` carries `name`, `id`, `domains` and nothing else; `startup.rs` copies only
      `tenant.name`, so `NodeConfig.auth` is always `None`. Either end that, or refuse — the epic's
      rule 1 forbids the current middle.
- [ ] **Failing-first**: a test loads a document declaring `auth.required: true` with a realm and a
      credential, starts the registrar, sends a REGISTER with no `Authorization`, and requires
      `401` with a `WWW-Authenticate` challenge. It fails today — verified by hand against `HEAD`,
      which answers **`200 OK`**.
- [ ] The realm, the algorithm and whether authentication is required at all come from the document,
      per [cluster-config](../specs/cluster-config.md) §5 S2, which puts all three under `tenant[]`
      and says why a node-wide spelling would be a second, coarser policy.
- [ ] The nonce secret arrives **by reference** (§5 S6, §8 V9), not as a literal. S6 is explicit that
      it is cluster configuration rather than a per-node accident, because edges sharing a secret can
      answer each other's challenges and a self-generated one invalidates every outstanding nonce on
      restart. `AuthConfig`'s per-node literal is the shape this replaces.
- [ ] How a document names credentials without inlining them is decided and written down. §8 V9
      forbids inline secrets and there is no `credentialSource` resolution at all today, so this
      story either adds the field or records the decision to defer it — and if it defers, a document
      naming credentials inline is refused rather than accepted.
- [ ] A tenant with no `auth` block still runs open, exactly as today, and says so on the startup
      line (`FC-2`). This story makes authentication *reachable*; it does not change the default.
- [ ] Nonce lifetime and algorithm get schema fields, or the story records why not.
      `with_lifetime`/`with_algorithm` exist and are exercised only by tests, so the effective values
      are always the kernel defaults — correct values, reached by accident. §7.3 names a short nonce
      lifetime as one of its two operator mitigations and cluster-config has no field for it.
- [ ] `cargo test -p sipx-clstr-node -p sipx-clstr-registrar` green; the `RA` vector rows still pass.

## Progress

- (running log)

## Notes

- **Measured against `HEAD`.** A document declaring `required: true`, `realm: example.test`,
  `algorithm: SHA-256`, a `secretRef` and an `alice`/`hunter2` credential loaded with **no error and
  no warning**, and the node answered a REGISTER carrying no `Authorization` header with `200 OK`.
- **The loader advertises the key it drops.** A typo'd `authh` is refused with
  `expected one of: name, id, domains, auth, expiry, maxBindingsPerAor`. An operator who mistypes is
  corrected *toward* a key that does nothing, which is a strong signal that accepted keys are
  applied.
- **This was foreseen and deferred on purpose.** `DP-10`'s notes: "`AuthConfig` has the same shape of
  gap as the store did … Wiring `tenant[].auth` is the obvious companion to this story and is
  deliberately **not** folded into it — it is a security-behaviour change and deserves its own
  failing-first test and its own line in the changelog." That judgement stands; this is that story.
  What changed since is the failure mode: before `DP-8`/`DP-10` there was no way to *ask* for
  authentication, and now there is a way to ask that is accepted and ignored.
- **Out of scope, and why.** `CredentialStore::password() -> Option<String>` makes recoverable
  plaintext the trait's contract — the kernel recomputes the digest from the cleartext, so an
  HA1-only store cannot satisfy it, and `InMemoryCredentials` holds `Vec<(String,String,String)>`
  with `derive(Debug)` and no zeroization. Changing that is a trait change and an upstream
  conversation; this story wires what exists. Worth noting that this crate already applied the
  opposite discipline to DSNs on purpose — `StoreChoice::describe()` exists so a log line cannot leak
  a resolved credential "into the one artefact most likely to be copied into an issue" — and never
  extended it to passwords or the nonce key.
- **Do not enable authentication by default in this story.** The default is a separate decision with
  its own migration and its own line in the changelog; conflating "reachable" with "on" would make
  one story change behaviour for every existing deployment.
- Related: `RG-15` makes the outcome observable (there is currently no log line on a `401`, a `403`
  or a success), and `CX-5` files the nonce-uniqueness defect that would bite the moment this lands.
  Neither blocks this story, and `CX-5` should be read before believing a green auth test at scale.
