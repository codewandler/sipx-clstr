---
id: CF-10
title: check-docs must only see the repository, not whatever sits under it
pillar: Foundation
status: done
priority: 1
epic: conformance-harness
areas: [ci, build]
note: the gate's verdict currently depends on whether an agent worktree happens to exist
---

# check-docs must only see the repository, not whatever sits under it

## Goal
Make `scripts/check-docs.sh` check **the repository** rather than every `*.md` under the working
directory. Today it walks untracked directories, so its verdict depends on transient state that has
nothing to do with the commit being gated — which is the same class of defect as a check that is
green locally and red in CI.

## Acceptance
- [x] The set of files checked is derived from what git tracks, not from a filesystem walk.
      `scripts/check-docs.py:44` is `ROOT.rglob("*.md")`.
- [x] The reported file count is stable for a given commit. Today it has ranged from **122** to
      **249** across runs of the same gate on the same tree, purely because worktrees came and went.
- [x] A broken link in an untracked directory does not fail the gate; a broken link in a tracked
      file still does. Both directions get a check.
- [x] Fenced code blocks and inline code spans are not scanned for links. A link checker that reads
      a code sample as a link cannot be used to write about link defects — quoting a real error
      message in a story file currently fails the gate, which is how this item was found.
- [x] The same question is asked of the other gate scripts — `check-provenance.sh`,
      `check-vectors.py` — and each either already scopes itself to tracked files or is fixed to.
      `check-provenance` scanning an untracked directory would be the more serious version of this
      bug, since it is the script enforcing non-negotiable #1.

## Progress

- **Done.** Two defects in one script, both closed.
- **The file set now comes from `git ls-files`, not `rglob`.** The walk needed a skip-list, and a
  skip-list is a denylist that grows an entry every time a tool invents a directory — which is how
  this happened. Measured before the change: **1331** markdown files seen against **169** tracked,
  because `.claude/worktrees/agent-*/` holds full checkouts contributing 3109 `*.md` files.
- **Failing-first, both directions.** A fabricated `.claude/worktrees/fake-agent/docs/poison.md`
  with a broken relative link turned the gate red before the change (`docs: FAIL — 1 problem(s)`)
  and is ignored after (`docs: clean (169 …)`). Then, to prove it had not simply gone blind, all
  three checks were re-provoked in tracked files and each still fails: a broken relative link in
  `docs/vision.md`, a site page relative-linking into `docs/specs/`, and a story naming an epic
  with no design doc.
- **Code blocks are no longer scanned for links.** This story could not previously quote the error
  message that produced it without failing the gate on itself. Fenced blocks and inline spans are
  stripped first. Verified both ways: a broken link inside a fence and inside backticks is ignored,
  while a broken link in prose in the same file still fails.
- Indented blocks are deliberately *not* stripped — four-space indentation is also how this
  repository wraps continuation lines in list items and frontmatter, and those carry real links.
- **The sibling gates were audited and both are already correct**, for different reasons.
  `check-provenance.sh` uses `git grep`, which only sees tracked files; its non-git fallback walks
  the tree, but that path only runs where there is no git repository and therefore no worktrees.
  `check-vectors.py` scopes its walk to `crates/`, which agent worktrees fall outside. Neither
  needed changing, which is worth recording so the question is not re-asked.
- Failure is loud: if `git ls-files` cannot run, the script exits with a message saying so rather
  than falling back to a walk. A gate that checks nothing because it could not decide what to check
  is the exact failure this file has already shipped once.
- Considered for upstream: no. This is this repository's own gate.

## Notes
- **Found during wave 4.** `scripts/gate.sh` went red on the integration branch with:
  ```
  broken link  .claude/worktrees/agent-a128a802637351ec5/docs/designs/routing-trunks.md:
               a relative link to ../specs/asserted-identity.md
  ```
  (The target is written out rather than quoted as a link, because the checker does **not** skip
  fenced code blocks — quoting the original error verbatim in this file made the gate fail a second
  time, on this story. That is a second defect in the same script and part of this story's scope:
  a link checker that reads code samples as links cannot be used to document link defects.)
  That path is a **live agent worktree**, mid-story: the link was correct-in-progress, pointing at a
  spec the agent had not written yet. Nothing in the commit under test was wrong, and the tracked
  tree was clean — the gate simply saw work that was not part of the repository.
- The failure is latent rather than new. Worktrees existed during earlier waves and the check
  scanned them too; it only surfaced when one of them contained a forward reference. That is the
  worst shape for this kind of bug: it fires on unrelated timing.
- The reverse risk is the one that matters more: a broken link in a tracked file could be **masked**
  by a fixture or scratch copy elsewhere in the tree resolving the same relative path. This check
  exists because a release deploy failed on a link the gate had passed, so a way for it to pass
  wrongly is worth closing.
- `.claude/` is untracked and has no `.gitignore` entry, but adding one would not fix this:
  `Path.rglob` does not consult git at all. The fix is in how the file set is chosen.
