#!/usr/bin/env python3
"""Documentation consistency: the half of the gate that is not Rust.

There are two documentation trees and they are published differently. `website/docs/` is the
public site, authored for end users. `docs/` is internal contributor material — the story board,
the roadmap, design records, the normative specs — and **none of it is published**. A broken
relative link in either is cheap to check and expensive to notice late.

Four checks:

1. **Every relative link resolves.** Absolute URLs and bare anchors are out of scope; anything
   with a path is resolved against the file that contains it. This covers both trees.
2. **Every `epic:` slug has `docs/designs/<slug>.md`, and every `design:` path exists.**
3. **No page on the site relative-links out of the published tree.** Docusaurus can only route
   what lives under `website/docs/`, so a link from a site page to `../../docs/specs/foo.md`
   resolves perfectly on disk and is a **broken link on the published site**. The overwhelmingly
   common case is a site page reaching for a spec or a design, which is exactly the material that
   is no longer published; those are reached by absolute GitHub URL instead.

   This check is the inverse of the one it replaces. Until the site was split from `docs/`, the
   rule was "a published doc must not link into an *excluded* one", and the exclude globs were
   read out of `docusaurus.config.js`. That config key is now gone, which would have made the old
   check silently return `[]` — a gate that passes because it stopped looking is worse than no
   gate, so the rule is restated against the new arrangement rather than deleted.
4. **A spec defers upstream work to the ledger, never to a story.** A normative spec outlives every
   story that touches it, so "flagged for `CX-1` to file against sipx" is a promise addressed to
   something that can stop existing — and `CX-1` was already `done` when all three specs that said
   it were written, so the three kernel gaps it was named for were never filed and nobody noticed
   (`CX-6`). Two rules, both mechanical: a paragraph in `docs/specs/` that says *ledger row* must
   link `docs/upstream.md`, and no paragraph there may name a story as the thing that will file
   or raise the row. The ledger is the document to defer to because it is the one that does not
   close.

   Scoped to `docs/specs/` on purpose. Stories and design records are dated by nature — naming the
   story that will do the work is exactly right in a story — while a spec is read years later by
   somebody deciding what the platform is allowed to build.

Two rules about *what* gets read, both learned the hard way:

- **The file set comes from git, not from a directory walk.** See `markdown_files`.
- **Code is not prose.** Fenced blocks and inline spans are stripped before links are looked for,
  so a story can quote a real error message without the checker treating the quote as a defect.

Exit 0 when clean, 1 otherwise. Run from the repository root, or anywhere — paths are resolved
against this file's parent.
"""

from __future__ import annotations

import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
LINK = re.compile(r"\[([^\]]+)\]\(([^)]+)\)")
DOCS = ROOT / "docs"
SPECS = DOCS / "specs"
LEDGER = DOCS / "upstream.md"
SITE_DOCS = ROOT / "website" / "docs"
GITHUB_TREE = "https://github.com/codewandler/sipx-clstr/blob/main"
EXTERNAL = ("http://", "https://", "#", "mailto:")
# The phrase, not the word: "there is no nonce ledger" and "this platform's own ledger" are about
# other things entirely, and a check that fired on them would be argued with rather than obeyed.
LEDGER_ROW = re.compile(r"ledger rows?", re.I)
# The deferral construction, and only it. `for CX-1 to file against sipx`, `flagged for CX-1 to
# raise` — the story ID optionally in backticks, because half the occurrences were and half were
# not. Matched against the raw text rather than the code-stripped prose for that reason.
DEFERRED_TO_STORY = re.compile(r"\bfor\s+`?([A-Z]{1,3}-\d+)`?\s+to\s+(?:file|raise)\b")


def markdown_files() -> list[pathlib.Path]:
    """Every markdown file **this repository tracks**, and nothing else.

    Asked git rather than walked, deliberately. Walking the tree needs a skip-list, and a skip-list
    is a denylist that has to grow an entry every time a tool invents a directory — which is exactly
    how this went wrong: agent worktrees under `.claude/worktrees/` are full checkouts of this
    repository, so the walk found 3109 markdown files beside the 169 real ones. The count stopped
    meaning anything, and worse, **the verdict depended on whether a sibling worktree happened to
    exist**: another agent's in-flight, half-written doc could turn this gate red on a diff that
    never touched it, and a second copy of a file could resolve a link that was broken in the one
    that mattered.

    `git ls-files` also excludes untracked scratch, which matches the provenance gate's rule that
    untracked files are ignored by design.
    """
    try:
        listed = subprocess.run(
            ["git", "-C", str(ROOT), "ls-files", "-z", "--", "*.md"],
            capture_output=True,
            check=True,
        ).stdout
    except (OSError, subprocess.CalledProcessError) as error:
        # Loudly, never silently. A gate that checks nothing because it could not work out what to
        # check is the failure mode this file has already been bitten by once.
        raise SystemExit(
            f"docs: FAIL — cannot list tracked files ({error}).\n"
            "  This check needs the repository's file list from git. Run it inside a git\n"
            "  checkout; it deliberately does not fall back to walking the directory tree."
        ) from error

    names = [name for name in listed.decode("utf-8").split("\0") if name]
    # A tracked path can still be absent from the working tree (a sparse checkout, a deleted-but-
    # unstaged file). Reading one would crash the checks below, so skip what is not there.
    return sorted(path for name in names if (path := ROOT / name).is_file())


def prose(text: str) -> str:
    """`text` with fenced blocks and inline code spans removed.

    A link checker that reads a code sample as a link cannot be used to *write about* link defects:
    quoting a real "broken link ..." error message inside a story file made this gate fail on the
    story that documented the failure. Code is not prose and its brackets are not links.

    Newlines are preserved so nothing downstream shifts; only the code itself goes.
    """
    without_fences = re.sub(
        r"^[ \t]*(`{3,}|~{3,}).*?^[ \t]*\1[ \t]*$",
        lambda m: "\n" * m.group(0).count("\n"),
        text,
        flags=re.S | re.M,
    )
    # Indented code blocks are not stripped: four-space indentation is also how this repository
    # wraps continuation lines in frontmatter and list items, and those do carry real links.
    return re.sub(r"`+[^`\n]*`+", "", without_fences)


def check_links() -> list[str]:
    problems = []
    for md in markdown_files():
        for match in LINK.finditer(prose(md.read_text(encoding="utf-8"))):
            target = match.group(2)
            if target.startswith(("http://", "https://", "#", "mailto:")):
                continue
            path = target.split("#")[0]
            if path and not (md.parent / path).resolve().exists():
                rel = md.relative_to(ROOT)
                problems.append(f"broken link  {rel}: [{match.group(1)}]({target})")
    return problems


def check_stories() -> list[str]:
    problems = []
    stories = ROOT / "docs" / "stories"
    if not stories.is_dir():
        return problems
    for story in sorted(stories.glob("*.md")):
        if story.name in ("README.md", "_TEMPLATE.md"):
            continue
        text = story.read_text(encoding="utf-8")
        # An empty frontmatter field is legitimate — `epic:` with nothing after it means "no
        # epic". `\S+` with `[ \t]` rather than `\s` is what keeps the match from running past
        # the newline and swallowing the next field's name as this field's value.
        front = text.split("---")[1] if text.startswith("---") else ""
        epic = re.search(r"^epic:[ \t]*(\S+)[ \t]*$", front, re.M)
        design = re.search(r"^design:[ \t]*(\S+)[ \t]*$", front, re.M)
        if epic and not (ROOT / "docs" / "designs" / f"{epic.group(1)}.md").exists():
            problems.append(f'{story.name}: epic "{epic.group(1)}" has no design doc')
        if design and not (ROOT / design.group(1)).exists():
            problems.append(f'{story.name}: design path "{design.group(1)}" does not exist')
    return problems


def paragraphs(text: str) -> list[tuple[int, str]]:
    """`(line number, block)` for each blank-line-separated block of `text`.

    A paragraph rather than a line, because the two halves of a deferral wrap: the phrase and the
    link that should accompany it are routinely on different lines of the same sentence.
    """
    blocks: list[tuple[int, str]] = []
    block: list[str] = []
    first = 1
    for number, line in enumerate(text.splitlines(), start=1):
        if line.strip():
            if not block:
                first = number
            block.append(line)
        elif block:
            blocks.append((first, "\n".join(block)))
            block = []
    if block:
        blocks.append((first, "\n".join(block)))
    return blocks


def check_spec_deferrals() -> list[str]:
    """A spec defers upstream work to the ledger, never to a story — see the module docstring."""
    if not SPECS.is_dir():
        return []

    problems = []
    for md in markdown_files():
        if not md.is_relative_to(SPECS):
            continue
        rel = md.relative_to(ROOT)
        raw = md.read_text(encoding="utf-8")

        for line, block in paragraphs(prose(raw)):
            if not LEDGER_ROW.search(block):
                continue
            targets = [
                match.group(2).split("#")[0]
                for match in LINK.finditer(block)
                if not match.group(2).startswith(EXTERNAL)
            ]
            if not any(target and (md.parent / target).resolve() == LEDGER for target in targets):
                problems.append(
                    f"ledger row  {rel}:{line}: names a ledger row and does not link the ledger "
                    f"— point at docs/upstream.md, which is where the row can be acted on"
                )

        # Against the raw text: the story ID is inside a code span half the time, and `prose`
        # removes those.
        for line, block in paragraphs(raw):
            for match in DEFERRED_TO_STORY.finditer(block):
                problems.append(
                    f"dead letter  {rel}:{line}: defers to {match.group(1)} as the story that "
                    f"will file or raise it — name the ledger row instead; a story closes, the "
                    f"ledger does not"
                )
    return problems


def check_site_links() -> list[str]:
    """A site page must not relative-link out of the published tree — see the module docstring."""
    if not SITE_DOCS.is_dir():
        return []

    problems = []
    for md in markdown_files():
        if not md.is_relative_to(SITE_DOCS):
            continue  # Only the published tree carries a routing constraint.
        for match in LINK.finditer(prose(md.read_text(encoding="utf-8"))):
            target = match.group(2)
            if target.startswith(("http://", "https://", "#", "mailto:")):
                continue
            path = target.split("#")[0]
            if not path or path.startswith("/"):
                continue  # Site-root-relative; Docusaurus resolves it against baseUrl.
            resolved = (md.parent / path).resolve()
            if resolved.is_relative_to(SITE_DOCS):
                continue
            if resolved.is_relative_to(DOCS):
                # The common case, and the reason this check exists: reaching for a spec, a
                # design record or the roadmap, none of which the site publishes.
                where = resolved.relative_to(ROOT).as_posix()
                why = f"docs/ is not published — link {GITHUB_TREE}/{where} instead"
            else:
                why = "that path is outside website/docs/, so the site has no page for it"
            problems.append(
                f"site link  {md.relative_to(ROOT)}: [{match.group(1)}]({target}) — {why}"
            )
    return problems


def main() -> int:
    problems = check_links() + check_stories() + check_spec_deferrals() + check_site_links()
    for problem in problems:
        print(problem)
    if problems:
        print(f"\ndocs: FAIL — {len(problems)} problem(s)")
        return 1
    print(f"docs: clean ({len(markdown_files())} markdown files checked)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
