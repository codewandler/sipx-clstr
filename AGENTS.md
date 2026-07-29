# sipx-clstr — working agreement

A clustered, proxy-first SIP platform in Rust, built on the [sipx](https://github.com/codewandler/sipx)
protocol kernel. Read [docs/vision.md](docs/vision.md) once; it is the tie-breaker when a design
choice is unclear. [README.md](README.md) is the same project explained for humans arriving cold.

## Where things are

| Path | What it holds | Generated? |
|---|---|---|
| `docs/vision.md` | why the project exists, the seven principles that break ties | hand-written, stable |
| `docs/roadmap.md` | status, milestones, the epic narratives | hand-written |
| `docs/specs/` | normative specifications with test-vector tables — the contract code is written against | hand-written |
| `docs/designs/` | one design record per epic and per non-trivial story | hand-written |
| `docs/stories/` | one file per unit of work; `README.md` there is the **board** | board is generated |
| `docs/upstream.md` | the ledger of what belongs in the sipx kernel rather than here | hand-written |
| `docs/architecture.md` | the charts: request paths, roles, deployment control plane | hand-written |
| `crates/` | the Rust workspace — `-proxy` and `-registrar` are sans-IO, `-node` is the driver layer, `-sim` the harness, `-probe` the e2e-tester | hand-written |
| `scripts/` | the gate: `gate.sh` and the checks it runs | hand-written |
| `deploy/helm/` | the chart, its templates and the default deployment set (`KO-2`) | hand-written |
| `website/` | the published documentation site (Docusaurus), deployed on release | built by CI |
| `CHANGELOG.md` | closed stories roll up here | hand-written |

**The state of play, in one line:** M1 is **complete** — all fourteen stories `done`, every exit
criterion proved, cut as `0.5.0` — and sipx `v0.4.0` cleared the whole
[upstream ledger](docs/upstream.md), so nothing here waits on the kernel. The gate is green, and
M1 ships one known defect: `RG-8`, a retransmitted REGISTER answered `500`, `ready` at priority 1.
Check the board before assuming any of that is current.

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
3. **No panics on network input, no `unsafe`.** `unsafe_code` is forbidden workspace-wide;
   `unwrap`, `expect`, `panic` and raw indexing are lint warnings, and CI builds with
   `-D warnings`, so in library code they are errors. Test modules opt out per-module. A proxy
   that panics on a message takes every call on the node with it.
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

   The boundary continues downstream: **a consuming deployment is reference-only.** A deployment
   repo never contains implementation — it pins sipx-clstr releases and holds its own configuration
   and values; its changes are handled from that repo. If work over there starts to look like
   implementation — a chart, a template, an image, protocol code — it belongs here (or in the
   kernel). Deployment repos are not named here: this platform is not any one company's, and a
   requirement that only makes sense for a single deployment is a requirement in the wrong place.
7. **Never commit without an explicit instruction from the user.**

## The gate

Before marking any story done:

```sh
scripts/gate.sh
```

which is:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
scripts/check-features.sh      # each crate with its optional features off, not just --all-features
scripts/check-provenance.sh    # needs SIPX_DENYLIST or ~/notes/sipx-research/denylist.txt
scripts/check-docs.sh          # links resolve; every epic has a design; every design path exists
```

If it is green locally and red in CI, that difference is a bug in `scripts/gate.sh` — fix it there
rather than working around it in the workflow.

**`check-provenance.sh` carries the carve-out from non-negotiable #1.** The denylist is supplied
from outside the repository, deliberately; the *allowlist* — named integration and interop targets
that may appear anywhere — is `scripts/provenance-allow.txt`, in the repository, with a reason per
line. Adding a line is a deliberate act, and "we looked at it" is not a reason.

**`check-features.sh` is not garnish.** `--all-features` hides a whole class of breakage: a crate
that does not compile with an optional feature turned *off* is invisible until a consumer turns it
off, and by then it is in a release.

Documentation consistency is part of the gate rather than a separate practice, because `docs/` is
published:

- Every story's frontmatter is complete and the board regenerated (`/track:board`).
- New specs carry normative references and test-vector tables, not prose alone.
- Anything added under `docs/` that should be readable on the site is reachable from
  `website/sidebars.js`; the site build fails on a broken link rather than shipping one.

## Publishing

`docs/` is the source of truth and the website is a view of it: `website/docs/` symlinks or imports
the same files rather than copying them, so there is one set of words. The site deploys to
**[codewandler.github.io/sipx-clstr](https://codewandler.github.io/sipx-clstr/)** on every published
release — not on every push to `main`, so what the public reads matches a tagged version rather than
whatever landed an hour ago. `workflow_dispatch` is the recovery hatch.

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
