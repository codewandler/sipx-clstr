---
id: DX-11
title: Write the observability and high-availability operate pages
pillar: Foundation
status: in-progress
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

- [x] Both pages open with the `:::caution Preview` admonition, and `observability.md` states what
      exists today: `RUST_LOG` to stderr, and nothing else — no metrics endpoint, no management
      port.
- [x] `observability.md` describes the designed surface: metrics, log format, call detail, and the
      synthetic end-to-end probe.
- [x] It explains the probe's shape — a role that dials the **public border** like a customer,
      calls an `echo` endpoint in a test tenant, and returns `Pass` / `Fail{step, cause}` /
      `Inconclusive{fault}` — and why it is deliberately drawn *outside* the cluster border.
- [x] It explains why `Inconclusive` is a distinct verdict rather than a failure.
- [x] `high-availability.md` states the guarantee precisely: **service HA** — new calls and
      registrations succeed after a node loss — and that established calls surviving the loss of
      their signalling node is an explicit, later, opt-in that is *never silently promised*.
- [x] It explains why: the cluster holds no shared call state, so a lost node loses its
      connections, and connection ownership cannot be handed over.
- [x] Both link their specs and designs by absolute GitHub URL.

## Progress

- Both pages written; the placeholder bodies are gone and the frontmatter is untouched, including
  the double-quoted `description:` values that keep a bare `: ` from breaking the YAML parse.
- `observability.md` leads with what exists today — `RUST_LOG` to stderr with ANSI off, and a table
  whose other six rows are `specified, not shipped` or `designed`, naming the metrics endpoint, the
  management port and CDR explicitly because those are the three people assume are there. Then the
  designed surface (invariant metrics, `logFormat`, CDR field set as a billing contract, HEP
  capture per transport), then the probe: the four-step plan with its timeouts, the marker, the
  three verdicts, the blast-radius rules and the trigger API.
- The two arguments the story exists for are given their own sections rather than a clause:
  *why it is drawn outside the border* (an internal address may not be a target; `echo` is refused
  beside any proxy role, at load rather than at runtime) and *why `Inconclusive` is not a failure*
  (never counted in the success ratio, alerted separately, and the residual case is
  `Inconclusive{fault: Internal}` rather than an invented platform fault).
- `high-availability.md` states service HA in one sentence, then derives the line from the two
  facts that decide it: routing information survives because it was never on the lost node, and a
  connection cannot be handed over because the descriptor was in that process. The recovery table
  deliberately carries no time bounds — DP-4 owes those, one harness scenario per row — and says
  so instead of inventing them.
- The "today there is none" section is blunt: one node, and losing it loses every in-memory
  registration; the PostgreSQL store is behind a cargo feature and changes nothing; two copies of
  the binary are not a cluster.
- Gate: `python3 scripts/check-docs.py` → `docs: clean (166 markdown files checked)`;
  `npm run build` → `[SUCCESS] Generated static files in "build"`.
- The site build first failed at exit 13 ("unsettled top-level await") with corrupted webpack pack
  files. Cause is environmental, not this diff: worktrees sharing one `website/node_modules` share
  its `.cache`, and concurrent builds corrupt each other's packs. Building against a mirror of
  `node_modules` with a private `.cache` is green on the same tree.
- Considered for upstream: no. These pages describe this platform's own operational contract — its
  role set, its verdict taxonomy, its HA statement — none of which the protocol kernel has an
  opinion about.

## Notes

- Spec: `docs/specs/e2e-probe.md`; designs: `docs/designs/e2e-tester.md`,
  `docs/designs/deployment.md`. Absolute GitHub URLs only.
- The non-goal wording in `docs/vision.md` is deliberate and worth matching closely — this is the
  claim most likely to be over-read by a reader evaluating the platform.
- `echo` is refused in combination with any proxy role.
