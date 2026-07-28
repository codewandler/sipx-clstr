# sipx-clstr docs

Start here to find anything inside the repository. These are the internal contributor docs: vision,
roadmap, story status, design records, specs and notes. Work is tracked with the **track**
framework — see [AGENTS.md](https://github.com/codewandler/sipx-clstr/blob/main/AGENTS.md) → **"Start here"** for the working loop.

## Map

| If you want… | Read |
|---|---|
| Why the project exists; the principles | [vision.md](vision.md) |
| The architecture at a glance (charts) | [architecture.md](architecture.md) |
| Status + what's next; the epics | [roadmap.md](roadmap.md) |
| **What to work on right now** | [stories/README.md](https://github.com/codewandler/sipx-clstr/blob/main/docs/stories/README.md) — the backlog/status board |
| The detail of a specific story | `stories/<ID>-<slug>.md` |
| Design records for non-trivial work | [designs/](designs/) |
| Normative specs (written before code) | [specs/](specs/) |
| What we need from the sipx kernel | [upstream.md](upstream.md) — the upstream dependency ledger |
| Finished / superseded material | [archive/](archive/) |
| Released history | [CHANGELOG.md](https://github.com/codewandler/sipx-clstr/blob/main/CHANGELOG.md) |

**Specs vs designs.** A **spec** (`docs/specs/`) is the normative contract for a subsystem —
RFC references, types, state tables, timers, byte-level test vectors — written before the code,
with tests derived from its vectors. A **design** (`docs/designs/`) is a decision record: why this
shape, what was rejected, what's still open. An epic normally has both.

## Working here

Every contributor — human or agent — starts at [AGENTS.md](https://github.com/codewandler/sipx-clstr/blob/main/AGENTS.md) → **"Start here"**: open the
[board](https://github.com/codewandler/sipx-clstr/blob/main/docs/stories/README.md), take the top `ready` story by priority, follow the loop, keep the gate
green. New or unscoped work? Create a story first (`/track:story`) so the next agent inherits the
context. After any change to a story's status/priority/title/epic/note, run `/track:board`. Optional
story `areas` are query-only subsystem tags for selection, e.g. `/track:next proxy`.
