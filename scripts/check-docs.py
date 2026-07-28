#!/usr/bin/env python3
"""Documentation consistency: the half of the gate that is not Rust.

`docs/` is published — the website is a view of these exact files — so a broken relative link
here is a broken page on the site, and a story whose `epic:` names a design that does not exist
is a board row pointing at nothing. Both are cheap to check and expensive to notice late.

Two checks:

1. **Every relative link resolves.** Absolute URLs and bare anchors are out of scope; anything
   with a path is resolved against the file that contains it.
2. **Every `epic:` slug has `docs/designs/<slug>.md`, and every `design:` path exists.**

Exit 0 when clean, 1 otherwise. Run from the repository root, or anywhere — paths are resolved
against this file's parent.
"""

from __future__ import annotations

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
SKIP_DIRS = {".git", "node_modules", "build", "target", ".docusaurus"}
LINK = re.compile(r"\[([^\]]+)\]\(([^)]+)\)")


def markdown_files() -> list[pathlib.Path]:
    return sorted(
        md
        for md in ROOT.rglob("*.md")
        if not SKIP_DIRS.intersection(md.relative_to(ROOT).parts)
    )


def check_links() -> list[str]:
    problems = []
    for md in markdown_files():
        for match in LINK.finditer(md.read_text(encoding="utf-8")):
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


def main() -> int:
    problems = check_links() + check_stories()
    for problem in problems:
        print(problem)
    if problems:
        print(f"\ndocs: FAIL — {len(problems)} problem(s)")
        return 1
    print(f"docs: clean ({len(markdown_files())} markdown files checked)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
