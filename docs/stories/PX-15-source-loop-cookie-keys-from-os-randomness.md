---
id: PX-15
title: Source per-process loop-cookie keys from operating-system randomness
pillar: Signalling
status: ready
priority: 2
design: docs/designs/proxy-engine.md
epic: proxy-engine
areas: [proxy, node, security]
note: V-15 — the HMAC key is predictable text derived entirely from process startup time
---

# Source per-process loop-cookie keys from operating-system randomness

## Goal

Give RFC 5393 loop cookies the unforgeable per-process key their design assumes. Startup time is
public metadata, not key material; the driver must inject cryptographic randomness or refuse to run.

## Acceptance

- [ ] `driver::cookie_key` obtains at least 256 bits from the operating system CSPRNG at startup and
      constructs `CookieKey` from those bytes. `SystemTime`, epoch text, process ids, node ids, and
      deterministic fallback values contribute no key material.
- [ ] Randomness failure is a named startup error before a listener binds. There is no all-zero,
      timestamp, fixed, or “best effort” fallback.
- [ ] Randomness remains at the driver boundary. The proxy decision core continues to receive an
      injected `CookieKey`, so vectors and deterministic simulations use explicit test keys without
      reading a clock or entropy source.
- [ ] The key remains redacted from `Debug`, errors, and startup logs. A test captures stdout/stderr
      from successful and failed startup and proves no key bytes or reversible encoding appears.
- [ ] **Failing-first test:** key construction is exercised with an injected deterministic entropy
      source and proves the bytes, not wall time, determine the cookie. A source check/test rejects
      reintroduction of `SystemTime` in production key generation; it fails on `86e6b10` at
      `crates/sipx-clstr-node/src/driver.rs:771-782`.
- [ ] Two independently started nodes receive different per-process keys under a controlled entropy
      fixture, while retransmissions within one process remain deterministic and existing cookie
      vectors stay green.
- [ ] `scripts/gate.sh` is green.

## Progress

- (not started)

## Notes

- Source: validated synthesis **V-15**. HMAC-SHA256 and its truncation are not the defect; the driver
  supplies `sipx-clstr/<Unix epoch nanoseconds>` as the entire HMAC key.
- Dependencies: none. This story deliberately delivers only safe per-process generation. `AF-6`
  remains the later owner of cluster distribution and rotation and should consume the same
  `CookieKey` injection point rather than replacing it.
- Considered for upstream: **no for key sourcing.** The existing HMAC/cookie primitive is generic and
  already lives at the kernel-facing protocol seam; obtaining and lifecycle-managing this
  deployment's secret is driver orchestration here.
