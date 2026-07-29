---
id: FC-1
title: Refuse a listener transport the node cannot serve, instead of silently serving cleartext
pillar: Cluster
status: in-progress
priority: 1
design: docs/designs/fail-closed-config.md
epic: fail-closed-config
areas: [deploy, security, transport]
note: the downgrade is closed (CC-V10); the tls sub-block and the published exposure row are left
---

# Refuse a listener transport the node cannot serve, instead of silently serving cleartext

## Goal

Make a declared transport either real or a load error. Today a listener declaring `transport: tls`
is served as a cleartext UDP socket, silently, so the one configuration an operator writes in order
to get confidentiality is the one that removes it.

## Acceptance

- [x] `startup.rs`'s transport mapping stops falling through. The current
      `match declared.transport.as_str() { "tcp" => Tcp, _ => Udp }` becomes a closed match over the
      spelling set the schema defines, and anything the node cannot serve is a load error naming the
      value and the recognised set — the shape §4 R1 already uses for roles.
- [x] A document declaring `transport: tls` **refuses to start** with a message that says a
      certificate cannot yet be supplied, rather than binding UDP. `NodeError::NoCleartextListener`
      already exists for the TLS-only node and is currently unreachable from a document; this story
      is what makes it reachable, or replaces it with a load-time refusal, and records which.
- [ ] `listener[].tls` stops being an allow-listed key nothing descends into. Either `certRef`/
      `keyRef` are validated and resolved (§8 V9, by reference), or naming the block is itself the
      refusal — not both, and not neither.
- [x] **Failing-first**: a test loads a document with `transport: tls` and requires a refusal. It
      fails today, because the document loads clean and the node comes up on UDP. A second case
      pins `transport: ws`/`wss`/`http` to the same refusal, since all three reach `_ => Udp` now.
- [x] `transport: udp` and `transport: tcp` behave exactly as they do today — this story adds a
      refusal, it does not change what already works. The existing `DP-5` bind/advertise rules and
      §5 P6's duplicate-transport refusal are untouched.
- [ ] The published exposure guidance stops advertising a transport the node cannot serve.
      `website/docs/operate/deploy.md`'s table has a `TLS 5061 | public` row; either it goes, or it
      carries the status the rest of the site's unshipped surface carries. Coordinate with `DX-13`
      rather than editing the page twice.

## Progress

- **The downgrade is closed** — landed in `2a7aeeb` ("Close a fail-open on TLS, and repair the M1
  proof I broke"), which cites this story. `check_transport` in the loader accepts only `udp`/`tcp`;
  `tls`/`ws`/`wss` are refused under a new rule id **`CC-V10`** with a message that says outright it
  "will not silently substitute cleartext for a transport that was asked for", and anything else is
  refused naming the closed set. The `startup.rs` match is explicit too, so adding a transport to the
  loader without wiring it is a startup error rather than a substitution. Two tests pin both cases.
- **Re-verified against the rebuilt binary, since the defect was reported from a running node rather
  than from the source.** The original repro no longer reproduces: the same `transport: tls` document
  that previously bound plaintext UDP and answered `200 OK` is now refused with
  `cluster.listener[0].transport [CC-V10]`. The `tls` spelling is named rather than treated as
  unknown, which was the distinction the story asked for.
- **A nit worth deciding, not a defect.** The refusal cascades: the rejected listener leaves the node
  with none, so the output also carries
  `cluster.listener [CC-P4]: found 0 listeners for this node's roles`. That is §8 V1 working as
  designed — every problem, not the first — but the second line is a consequence of the first rather
  than an independent mistake, and an operator reading two errors for one typo may go looking for two
  fixes. Either suppress the derived error or leave it and say why.
- **Left in this story.** `listener[].tls` is still an allow-listed key nothing descends into, so a
  `tls: {certRef, keyRef}` block is accepted and ignored — now only ever alongside a `udp`/`tcp`
  listener, where it is meaningless, so the severity has dropped from "removes confidentiality" to
  "accepted and discarded". It still violates the epic's rule 1 and is the remaining code item.
  `connectionLifetime` and `maxConnections` are in exactly the same state on the same allow-list;
  `KO-14`'s notes record them, and whichever story plumbs or refuses them should do all three at once.
- **Also left**: `website/docs/operate/deploy.md`'s `TLS 5061 | public` exposure row, which now
  advertises a transport the loader actively refuses. `DX-13` owns the edit so the page is touched
  once.

## Notes

- **Measured against `HEAD`, not inferred.** A document with `transport: tls`,
  `bind: 127.0.0.1:5062`, `advertise: 127.0.0.1:5062` and a `tls:` block started clean, printed
  `listening on 127.0.0.1:5062`, and answered an unauthenticated **plaintext UDP** REGISTER with
  `200 OK`, writing a binding. No error, no warning, at any log level.
- The type system is not the problem. `TransportKind::Tls` is fully supported below this seam —
  `listen.rs` admits it and emits `;transport=tls` in a Record-Route — and `driver.rs` already
  raises `NoCleartextListener` for a TLS-only node. The two-line string fallthrough in `startup.rs`
  is what makes that guard dead code on the document path, so the fix is small and the blast radius
  is one function.
- **Why this outranks the rest of the epic.** It is the only finding where the published
  documentation actively directs an operator toward the false guarantee: `deploy.md`'s exposure
  table lists TLS 5061 as public, and `reference/cli.md` says a TLS listener "cannot be declared
  here" — which is now false in its operative clause. It can be declared; it is accepted and
  discarded.
- **Why it is a security inversion rather than a gap.**
  [registrar-auth](../specs/registrar-auth.md) §7.3 makes TLS its primary normative mitigation, and
  §7.2/RA-R-7 computes what a cleartext hop costs: a captured `Authorization` replayed with
  `Contact: *`, `Expires: 0` and a fresh `Call-ID` de-registers every binding on the
  address-of-record. Accepting the configuration that asks for the mitigation while serving the risk
  is worse than having no TLS field at all.
- Upstream? No. The kernel owns transports; which spellings *this* schema admits, and what a node
  does with one it cannot serve, is this platform's configuration semantics.
- Decide explicitly whether `ws`/`wss` refuse or are simply absent from the schema. M3 owns
  WebSocket reachability, and a key that will exist later is not a reason to accept it now.
