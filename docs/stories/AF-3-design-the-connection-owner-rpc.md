---
id: AF-3
title: Design the connection-owner RPC
pillar: Cluster
status: in-progress
priority: 2
design: docs/designs/cluster-affinity.md
epic: cluster-affinity
areas: [affinity]
note: 
---

# Design the connection-owner RPC

## Goal
Design the one cross-node hop on the signalling path: delivering a request to the edge that owns the target client's connection.

## Acceptance
- [ ] Delivery semantics are specified: at-most-once, bounded queueing, and an explicit failure taxonomy (owner unreachable ≠ flow dead ≠ flow rejected — the future `430 Flow Failed` mapping).
- [ ] Node-to-node authentication and the transport choice are decided with rationale.
- [ ] The failure taxonomy is exercised as harness scenarios.

## Progress

- **2026-07-31 — written, independently reviewed, `REWORK`, and the rework died on infrastructure.**
  The branch is **`impl/AF-3` at `c76f239`** (2 commits, 5 files, **zero Rust**), worktree preserved at
  `/home/timo/projects/sipx-clstr-AF-3` with a warm build cache. It adds a new normative spec
  [`owner-rpc.md`](../specs/owner-rpc.md) (296 lines) rather than more `affinity-token` sections, and
  the implementor's own reasoning is in that branch's copy of this file — read it there, it is long
  and mostly sound. A rework agent was dispatched with the findings below and was killed by an org
  monthly spend limit before writing anything; the worktree is clean, so **round 1 of 2 is still
  available**. Nothing is merged; `main` never saw this.
- **Review verdict `REWORK`, three blocking findings.** They are recorded here because `AF-7`
  implements against this spec and would inherit every one of them.
  1. **The "no addition to `proxy-behavior` §2's effect set" claim is false** (`owner-rpc.md:63`).
     `OW7` (`:164`) requires serializing to a connection slot+generation, but §2's only delivery
     effect carries `next_hop: Uri` (`proxy-behavior.md:52`), and the one URI-expressible form —
     Request-URI plus RFC 5923 reuse — is forbidden by `OW8` (`:168`). `AN1` (`:183`) makes `100`
     conditional on write completion, and §2's `Input` set (`proxy-behavior.md:40-48`) has no
     write-completed variant while ordered effects (`:60`) cannot express a conditional; deciding it
     in the driver puts a status-code decision there, against AGENTS.md rule 2. §12 — whose whole
     purpose is naming consequences for documents this spec does not own (`:283`) — omits
     `proxy-behavior` §2 (`:286-296`). `AF-5` is landing `Effect::VerifyToken` into that same enum
     with a story attached; this is the same class of delta, asserted away instead.
  2. **Item 3's delta is traceable but unenforceable in both directions.** `AF-7`'s acceptance item 2
     says "a harness scenario" — singular, satisfied outright by one of the thirteen
     (`owner_rpc_outcomes_are_distinct_in_the_trace`, `:254`) — and `AF-7`'s file is untouched by this
     diff, naming neither `owner-rpc.md` nor §10. The only link is a design open-questions bullet
     (`../designs/cluster-affinity.md:281-284`). **And no gate can see §10:** `SPECS` in
     `scripts/check-vectors.py:92-118` has no `owner-rpc` entry, and `EX-12`'s unowned-row guard
     cannot fire because `ANY_ROW_LINE` (`:131`) matches only a first cell shaped `XX-N` while §10's
     is a backticked function name. Demonstrated: `--check` passes at 154/583 with a 296-line
     normative spec invisible to it. **The `cluster-membership` §11 precedent this story leans on
     only half transfers** — §11 (`cluster-membership.md:319-350`) maps its rules onto rows of an
     *already registered* prefix (`CC`), every one in `vector-scope.toml` with a reason and a story,
     so `CF-16`'s sweep finds them; §10 maps onto nothing and enters no ledger. Cheapest closure,
     inside this story's scope: **name `owner-rpc` §10 in `AF-7`'s acceptance** (that file is not
     fenced). The general gate hole is filed separately as [`CF-25`](CF-25-a-new-spec-can-carry-normative-rules-no-gate-enumerates.md).
  3. **`AM6` instructs across an undecided boundary.** `:229` says an in-dialog request crosses the
     hop as its own delivery; the design's largest open question says a mid-dialog request has no
     lookup, "carries no reference and **cannot use the hop**"
     (`../designs/cluster-affinity.md:255-258`), and `RP2` (`:102`) confines the hop to a target from
     a lookup carrying `flow_ref`. Narrow `AM6` to what `RP2` admits, or settle the question.
- **Minors, all with `path:line`.** §12's preamble claims "none is performed by this story" (`:283`)
  while rows 1–2 *were*, plus two edits §12 never lists. **Five dead-letter `AF-3` pointers survive a
  commit titled "Repoint the dead-letter pointers"** (`affinity-token.md:110`, `:793`,
  `cluster-membership.md:93`, `:105`, `:187`), and `affinity-token.md:598` **newly writes a story name
  into a normative spec** as this story closes — `CF-24`'s defect class, created fresh. The peer
  channel's TLS *material* has no schema home (AU4 rests on §7 registering `tls` on a listener, but
  CH4 says the peer listener is not a `listener[]` entry and `cluster-config.md:166-190` has no row);
  identity genuinely needs no new field, so AU2/AU3's reuse of `rpc` checks out. AU6 clears KY9 and
  §10 on the merits but not its own "§8 gains no row" clause — UQ4 is declared exhaustive and already
  carries the loop-cookie key, a non-`keys[]` per-process CSPRNG key. AU8's `403` (`:135`) is not
  performable for a *foreign* peer endpoint, since CR5 (`:148`) confines matching to the owning node
  on the receiving listener — so a Route naming another member's `rpc` is loose-routed, making any
  edge a client-triggered dialler of any member's peer port. The epic's done-criterion
  (`cluster-affinity.md:328`) still claims the M2 mid-dialog assertion the new open question
  qualifies.
- **Three of §10's thirteen scenarios need sim faults that do not exist** — `fault.rs:33-92` offers
  only `KillNode`, `Partition`, `Heal`, `SetLinkPolicy`, `TimerSkew`; reconnect, restart-with-fresh-
  incarnation and backpressure (`owner-rpc.md:246`, `:247`, `:249`, `:250`) are absent, so "the
  harness executes it as it stands" is over-claimed for exactly those three. Filed as
  [`CF-26`](CF-26-the-harness-cannot-injure-a-connection.md).
- **What the review verified as sound — do not re-litigate.** Acceptance items 1 and 2 are met. **The
  carriage claim is byte-wise correct**: `-`/`_` are `mark` hence `unreserved` in RFC 3261 §25.1's
  `user` production, `affinity-token.md:203-206` makes the text form normatively **unpadded** *and*
  requires decode to reject padding, `AT-16` (`:509`) is a negative vector for appended `==`, and the
  arithmetic is padding-free anyway (49 B → 66 chars, 45 B → 60 chars) — §11.2 can never emit `=`.
  **RFC 4320 §4.1 supports `AN1` for non-INVITE**: it forbids a provisional other than `100 Trying`
  to a non-INVITE, which is the uniqueness `AN1` cites. Both open questions are genuinely in the
  design (`:252-269`, `:270-280`), not only here. Provenance clean (7 terms, 0 carve-outs);
  `check-docs.sh` clean (252 files); `check-vectors.py --check` exit 0.
- **The Rust three-quarters of the gate is unverified on this branch by anyone but the author.** I
  told the reviewer to skip `cargo fmt`/`clippy`/`test` to save disk on a zero-Rust diff, so the
  branch's "Full gate green" line has one pair of eyes. Run `scripts/gate.sh` in the preserved
  worktree on resume; its cache is warm.

## Notes
- Design: [cluster-affinity](../designs/cluster-affinity.md).
