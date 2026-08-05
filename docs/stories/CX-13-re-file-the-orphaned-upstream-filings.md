---
id: CX-13
title: Re-file the orphaned upstream filings on the kernel's main
pillar: Platform
status: ready
priority: 1
design:
epic:
areas: [upstream]
note: UPSTREAM — CX-7's two filings sit on an unmerged kernel branch and their IDs were recycled; until they exist on main, PX-12 and FC-1 wait on stories the kernel repo does not have
---

# Re-file the orphaned upstream filings on the kernel's main

## Goal

Put the outgoing-CANCEL and exact-listener-selection asks back into the kernel's actual backlog.
`CX-7` filed both — but the filing commit (`09d5518`) was pushed to `filing/clstr-CX-7-public`
and never merged, and fifteen hours later the kernel's own backlog work recycled `T-28` and
`T-29` for unrelated stories on `main`. The [ledger](../upstream.md) now reads **filing orphaned**
for both rows; this story makes them **filed** again, in a way that cannot silently un-file.

## Acceptance

- [ ] Both stories exist as files on the kernel repository's `main` (merged, not on a filing
      branch), under whatever IDs its pool next yields, with content re-verified against
      `v1.0.0-beta.4`'s surfaces: `impl Handle` exposes no cancel operation
      (`crates/sipx-transport/src/endpoint.rs:489`) and `bind_matching_ports` binds UDP
      unconditionally first (`endpoint.rs:1394,1403`).
- [ ] The two ledger rows in [docs/upstream.md](../upstream.md) move from **filing orphaned** to
      **filed**, each linking the story at its new ID on `main`.
- [ ] `PX-12`'s and `FC-1`'s dependency bullets and notes name the new IDs (or keep naming the
      ledger, which is the durable pointer).
- [ ] The lesson lands in the ledger's rules: a row may claim **filed** only for a story
      reachable from the kernel's `main` — a pushed branch is not filed.

## Progress

- Filed by `CX-12`'s ledger re-read at `v1.0.0-beta.4`, which found the orphaning: the filing
  commit is an ancestor of no tag and no mainline branch, and `git tag --contains` /
  `git branch --contains` prove it.

## Notes

- The original filed texts are still readable at the orphaned commit:
  [T-28 as filed](https://github.com/codewandler/sipx/blob/09d5518dc587dd77db61abd220ad309e00eda688/docs/stories/T-28-cancel-an-outgoing-invite-transaction.md),
  [T-29 as filed](https://github.com/codewandler/sipx/blob/09d5518dc587dd77db61abd220ad309e00eda688/docs/stories/T-29-bind-only-the-selected-cleartext-transports.md).
  Re-filing can start from them; the substance has not changed, only the citations.
- This story touches the sipx repository, like `CX-1` and `CX-6` before it. The work over there
  follows that repo's own conventions; what this story owns here is the ledger state and the
  two consumer stories' pointers.
