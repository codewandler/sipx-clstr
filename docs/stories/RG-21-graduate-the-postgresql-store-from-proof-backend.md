---
id: RG-21
title: Graduate the PostgreSQL store from a serialized cleartext proof backend
pillar: Registrar
status: backlog
priority: 3
design: docs/designs/registrar-location.md
epic: registrar-location
areas: [registrar, location, postgres, deploy]
note: V-14 · blocked by RG-17; then design and replace the NoTls mutexed client with a production backend
---

# Graduate the PostgreSQL store from a serialized cleartext proof backend

## Goal

Design and implement the production PostgreSQL driver promised by the registrar design: bounded,
encrypted, reconnecting and observable under registration storms, while preserving the existing
`LocationStore` CAS contract and sans-IO decision core.

## Acceptance

- [ ] **Design first:** extend [registrar-location](../designs/registrar-location.md) with the selected
      async/pool architecture, TLS trust and identity policy, acquisition/query/transaction deadlines,
      reconnect/backoff behavior, pool and queue bounds, shutdown, and overload mapping. Record
      rejected alternatives and capacity assumptions before implementation.
- [ ] The production path does not serialize all AoRs through one `Mutex<postgres::Client>` and does
      not depend on unbounded `block_in_place` work. Concurrency remains bounded independently from
      the proxy transaction admission limit.
- [ ] PostgreSQL transport uses configured TLS with peer verification; a deployment cannot silently
      downgrade to `NoTls`. Secrets remain references and never appear in config logs or Debug output.
- [ ] Connection acquisition, reads, writes and CAS transactions have explicit deadlines. Broken
      connections are evicted/reconnected with bounded backoff, and failures surface through RG-17's
      fallible store contract rather than as absence.
- [ ] Pool saturation and database unavailability produce bounded, observable registrar behavior;
      metrics cover pool occupancy/wait, operation latency, timeouts, reconnects, conflicts and
      failures without per-message overload logging.
- [ ] **Failing-first operational tests:** concurrent writes to independent AoRs execute concurrently;
      a stalled database cannot grow tasks beyond the configured bound; TLS verification failure,
      connection loss and recovery have deterministic assertions.
- [ ] The unchanged CAS/conformance suite passes against a real PostgreSQL instance, and a documented
      load test validates the design's stated bound. `scripts/gate.sh` is green.

## Progress

- (not started; the design update is the first implementation step)

## Notes

- Validated synthesis finding [**V-14**](../reviews/00-validated-synthesis.md#v-14--postgresql-mode-is-a-serialized-cleartext-proof-backend). V-08's false-success correctness defect is separate and owned by RG-17.
- **Dependency:** `RG-17` lands the fallible authoritative-read contract first. This story consumes
  that contract for timeout, connection and decode failures rather than inventing a backend-only
  error channel.
- **Upstream boundary:** no; database pooling, TLS policy, deadlines and failure mapping are local
  durable-state driver concerns. The SIP kernel remains outside this backend.
