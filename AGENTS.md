# sipx-clstr — working agreement

A clustered, proxy-first SIP platform in Rust, built on the [sipx](../sipx) protocol kernel. Read
[docs/vision.md](docs/vision.md) once; it is the tie-breaker when a design choice is unclear.

## Non-negotiables

1. **Provenance, with the integration carve-out.** SIP-stack prior art is never referenced as
   design rationale — "another SIP server does it this way" is not an argument, anywhere: code,
   comments, docs, stories, designs, commit messages. Rationale cites RFCs, sipx specs, or our own
   specs in `docs/specs/`. Named **integration and interop targets** are the carve-out: systems
   this platform talks to or is tested against (rtpengine, Kubernetes, PostgreSQL, SIPp, interop
   peers) may be named anywhere in the repo — specs, designs, configs, test harnesses — as
   targets, never as behavioral precedent.
2. **Decision logic is sans-IO.** Proxy forwarding decisions, location-service semantics, token
   mint/verify, and route planning are pure functions or state machines: time enters as a
   fired-timer input, bytes enter as data, randomness enters as an injected source. Sockets,
   clocks, databases and RPC live in drivers. If a piece of protocol or cluster logic cannot run
   under the deterministic harness, its design is wrong.
3. **No panics on network input, no `unsafe`.** When the workspace exists (`CX-2`),
   `unsafe_code` is forbidden workspace-wide; `unwrap`, `expect`, `panic` and raw indexing are
   lint warnings treated as errors in library code; test modules opt out per-module.
4. **Spec before code.** Non-trivial subsystems get a spec in `docs/specs/` first: normative RFC
   references, types, state tables, timers, and byte-level test vectors. Tests are derived from
   the spec's vectors.
5. **State rides the message.** Everything a mid-dialog message needs in order to be routed
   travels inside the message as signed tokens (Record-Route, Route, Path, flow tokens). A design
   that requires a global dialog lookup on the signalling hot path is wrong by definition here,
   not merely slow. Durable state (registrations, configuration, credentials) lives in owned
   stores off the hot path.
6. **Upstream first — always ask "does this belong in the kernel?"** Whenever you design,
   plan a story, or implement — before writing, not after — explicitly consider whether the
   thing at hand is protocol-generic (header syntax, parsing, transaction and dialog semantics,
   resolver capabilities, auth primitives, testkit machinery) or platform orchestration.
   Protocol-generic work is a sipx change, tracked in [docs/upstream.md](docs/upstream.md),
   even when building it here would be faster today; sipx-clstr never forks or
   shadow-implements kernel logic. Record the answer either way: a design doc or story that
   touches the boundary says where the work lands and why (one line — "considered for
   upstream: no, cluster-specific because …" — is enough). What stays here is orchestration:
   proxying, location, affinity, media control, deployment.

   The boundary continues downstream: **`babelforce-sip-clstr` is reference-only.** The company
   consumer repo never contains implementation — it pins sipx-clstr releases and holds its own
   configuration and deployment; its changes are handled from that repo. If work over there
   starts to look like implementation, it belongs here (or in the kernel).
7. **Never commit without an explicit instruction from the user.**

## The gate

The Cargo workspace does not exist yet (story `CX-2` creates it as the first act of M1). Until
then, the gate before marking any story done is documentation consistency:

- Every story's frontmatter is complete and the board regenerated (`/track:board`).
- Every `epic:` slug has a matching `docs/designs/<slug>.md`; every `design:` path exists.
- New specs carry normative references and test-vector tables, not prose alone.

Once `CX-2` lands, the gate becomes the sipx-style command set (fmt, clippy `-D warnings`, tests
with `--all-features`, a provenance check, and a feature-matrix check) and this section is updated
by that story.

<!-- BEGIN track:agents -->
## Start here (every session) — track backlog

This project tracks work with the **track** framework: every unit of work is a markdown story in
`docs/stories/`, and the board (`docs/stories/README.md`) is generated from story frontmatter.

1. **Orient** — read the latest user request, then run `git status --short --branch`. Treat
   uncommitted changes as user-owned unless you made them.
2. **What to work on** — if the user named work, do that. Otherwise open the
   [board](docs/stories/README.md) and take the top `ready` story by `priority` (lower = higher).
   `/track:next` reports it; `/track:next <area>` filters by optional story `areas`.
3. **The contract** — read the story's `## Goal` and `## Acceptance`; Acceptance defines "done". Read
   any linked `design:`.
4. **Do the work** — set the story `in-progress`; non-trivial design goes in `docs/designs/` first;
   implement; satisfy Acceptance with a **failing-first test**; keep the project's gate green.
5. **On done** — `/track:done <ID>`: set `status: done`, add a CHANGELOG entry, regenerate the board.
6. **New or unscoped work?** Create a story first (`/track:story`) so the next agent inherits the
   context.

The board's status lists are generated — after any change to a story's `status`/`priority`/`title`/
`epic`/`note`, run `/track:board`. Use optional `areas: [subsystem]` tags for query-only subsystem
selection without changing board rows. Story frontmatter is the single source of truth.
<!-- END track:agents -->
