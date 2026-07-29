---
id: DX-11
title: Write the observability and high-availability operate pages
pillar: Foundation
status: ready
priority: 7
design: docs/designs/docs-site.md
epic: docs-site
areas: [docs, observability, deploy]
note: service HA is the guarantee; call survival is never silently promised
---

# Write the observability and high-availability operate pages

## Goal

Give `website/docs/operate/observability.md` and `website/docs/operate/high-availability.md` their
content: what this platform is designed to expose about itself, and exactly what it does and does
not promise when a node is lost.

## Acceptance

- [ ] Both pages open with the `:::caution Preview` admonition, and `observability.md` states what
      exists today: `RUST_LOG` to stderr, and nothing else — no metrics endpoint, no management
      port.
- [ ] `observability.md` describes the designed surface: metrics, log format, call detail, and the
      synthetic end-to-end probe.
- [ ] It explains the probe's shape — a role that dials the **public border** like a customer,
      calls an `echo` endpoint in a test tenant, and returns `Pass` / `Fail{step, cause}` /
      `Inconclusive{fault}` — and why it is deliberately drawn *outside* the cluster border.
- [ ] It explains why `Inconclusive` is a distinct verdict rather than a failure.
- [ ] `high-availability.md` states the guarantee precisely: **service HA** — new calls and
      registrations succeed after a node loss — and that established calls surviving the loss of
      their signalling node is an explicit, later, opt-in that is *never silently promised*.
- [ ] It explains why: the cluster holds no shared call state, so a lost node loses its
      connections, and connection ownership cannot be handed over.
- [ ] Both link their specs and designs by absolute GitHub URL.

## Progress

- (running log)

## Notes

- Spec: `docs/specs/e2e-probe.md`; designs: `docs/designs/e2e-tester.md`,
  `docs/designs/deployment.md`. Absolute GitHub URLs only.
- The non-goal wording in `docs/vision.md` is deliberate and worth matching closely — this is the
  claim most likely to be over-read by a reader evaluating the platform.
- `echo` is refused in combination with any proxy role.
