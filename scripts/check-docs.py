#!/usr/bin/env python3
"""Documentation consistency: the half of the gate that is not Rust.

There are two documentation trees and they are published differently. `website/docs/` is the
public site, authored for end users. `docs/` is internal contributor material — the story board,
the roadmap, design records, the normative specs — and **none of it is published**. A broken
relative link in either is cheap to check and expensive to notice late.

Three checks:

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
SITE_DOCS = ROOT / "website" / "docs"
GITHUB_TREE = "https://github.com/codewandler/sipx-clstr/blob/main"


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
    problems = check_links() + check_stories() + check_site_links()
    for problem in problems:
        print(problem)
    if problems:
        print(f"\ndocs: FAIL — {len(problems)} problem(s)")
        return 1
    print(f"docs: clean ({len(markdown_files())} markdown files checked)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
