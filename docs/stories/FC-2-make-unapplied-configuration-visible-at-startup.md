---
id: FC-2
title: Make unapplied configuration visible at startup, at the depth the keys actually live
pillar: Cluster
status: done
priority: 2
design: docs/designs/fail-closed-config.md
epic: fail-closed-config
areas: [deploy, build]
note: the warning exists, is correct, and reaches nobody — it is logged before the subscriber is installed
---

# Make unapplied configuration visible at startup, at the depth the keys actually live

## Goal

Make the loader's "this build does not apply that" warning actually arrive, and make it able to name
the keys that matter. It exists, its reasoning is right, and it is emitted into a process with no
subscriber installed — so a release shipped four silently-discarded security keys with the detector
already in the tree.

## Acceptance

- [x] The tracing subscriber is installed **before** the configuration document is loaded.
      `from_document` currently runs in `main.rs` well above `tracing_subscriber::…try_init()`, so
      every `tracing` event emitted during load is discarded. Moving the init is the fix; keep the
      refusal path on `eprintln!` so a refused document still reports without a subscriber.
- [x] **Failing-first**: a test (or a scripted assertion in the gate) starts a node with a document
      naming a deferred section and requires the warning on stderr. It fails today — verified by
      hand: a document carrying `cluster.registrar` produced no warning at any `RUST_LOG` level.
- [x] Deferral is tracked at the depth the keys live. `Config::deferred` is a set of *top-level*
      cluster keys, so `tenant[].auth` and `listener[].tls` cannot be named by it even once the
      subscriber works. Whatever replaces it must be able to report a path, not a section name —
      `error::Path` already spells `cluster.tenant[0].auth` for the refusal case and is the obvious
      donor.
- [x] An allow-listed key that nothing consumes is reported by *something* — this story's warning,
      or another story's refusal. The two are exclusive per the epic's rule 1; what this story owes
      is that neither ends up being nobody.
- [x] The node's startup line says whether authentication is on. It currently logs
      `tenant=default store="in-memory"` — the store choice is named because `RG-12` thought it
      worth naming, and by the same argument an operator reading one line should be able to tell an
      open tenant from an authenticated one.
- [x] `cargo test -p sipx-clstr-node` green, and the gate green.

## Progress

- **Done.** Both halves, because the story is right that either alone is worthless.
- **Ordering.** The subscriber is installed in `run_node` *before* the document is read. Refusals stay
  on `eprintln!`, so a document that cannot be read still reports even if no subscriber could be
  installed. The duplicate init inside the runtime block is gone.
- **Depth.** `Config::deferred` — a set of top-level section names — is replaced by
  `Config::unapplied: Vec<Path>`, recorded at each site where the walk recognises a key and does not
  apply it. It now names `cluster.tenant[0].auth` and `cluster.listener[0].tls`, neither of which a
  section-name set could reach. Recorded *where they are recognised* rather than from a hand-kept
  list, so it cannot drift from what the code did.
- **Security keys get their own line.** "Not applied" reads very differently for `observability` than
  for the thing standing between a stranger and the registrar, so an ignored `auth` or `tls` is
  escalated rather than listed. Matched on the path's **leaf key** via a new `Path::leaf`, not a
  suffix on the rendered string: `ends_with(".auth")` would also match a key called `foo.auth`, and it
  reads to a human — and to clippy — as a filename-extension test, which it is not.
- **The startup line says whether authentication is on**: `auth="open"` beside `store="postgres"`.
  Today it is always `open`, which is exactly why it is worth printing.
- **Failing-first, and it had to drive the binary.** No unit test could have caught this: the warning
  *was* produced correctly, and the defect was the order two things happened in `main`.
  `tests/startup_warns.rs` starts the built binary with `RUST_LOG` unset and asserts on its stderr —
  the warning arrives, both nested paths are named, a `SECURITY:` line appears, and the startup line
  carries the auth state. Verified failing beforehand by hand: at `RUST_LOG=trace` the same document
  produced no matching line at all.
- One thing the tests taught: they run in parallel and originally shared a port, so three passed while
  the fourth failed on `Address already in use` — a node that never reached the line under test. A port
  per test.
- Considered for upstream: no. Startup ordering and this loader's reporting contract are the node's own.

### What this deliberately does not do

A warning is not a fix. `tenant[].auth` and `listener[].tls` still load clean and still change nothing;
this story only makes that audible. §3 D6 — "a half-understood security posture is worse than a node
that will not start" — means both want a **refusal**, which is `FC-1` and `FC-3`. What is closed here is
that neither ended up being nobody.

## Notes

- **The mechanism is not missing; it is unreachable.** `startup.rs` warns on a non-empty
  `cluster.deferred` and the comment gives exactly the right rationale — "worth saying out loud at
  startup rather than discovering as behaviour that never happens." Both halves of why it fails are
  mechanical: ordering, and depth.
- **Why this is worth its own story rather than a line in `FC-3`.** It is the reason the other three
  defects survived a release with the detector already written, and it is the only one of the four
  that is cheap. Fixing the ordering without widening the depth would still not name
  `tenant[].auth`; widening without fixing the ordering would still print to nowhere. Both, or
  neither is worth doing.
- **What a reader should not conclude.** A warning is not a fix. §3 D6's "a half-understood security
  posture is worse than a node that will not start" means authentication and transport want a
  *refusal* (`FC-1`, `FC-3`); this story's warning is the right answer only for configuration a
  node's roles genuinely do not consume — §4 R5's projected-away case, which is normal and not a
  defect.
- Consider whether `Config::deferred`'s contract should become "every allow-listed path this build
  did not consume", computed rather than hand-listed. A hand-maintained `DEFERRED_SECTIONS` is a
  list that goes stale exactly when a section is implemented, which is the same class of bug one
  level up.
- Upstream? No. This is the node's own startup ordering and the loader's own reporting contract.
