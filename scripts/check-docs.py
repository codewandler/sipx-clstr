#!/usr/bin/env python3
"""Documentation consistency: the half of the gate that is not Rust.

`docs/` is published — the website is a view of these exact files — so a broken relative link
here is a broken page on the site, and a story whose `epic:` names a design that does not exist
is a board row pointing at nothing. Both are cheap to check and expensive to notice late.

Three checks:

1. **Every relative link resolves.** Absolute URLs and bare anchors are out of scope; anything
   with a path is resolved against the file that contains it.
2. **Every `epic:` slug has `docs/designs/<slug>.md`, and every `design:` path exists.**
3. **No published doc relative-links into one the site excludes.** The site is a *view* of
   `docs/`, but a curated one: `docusaurus.config.js` excludes `stories/**` and friends, so a link
   that resolves perfectly on disk is a **broken link on the published site** — and Docusaurus
   fails the build on those, which is how the `v0.4.0` site deploy died while this gate stayed
   green. Published docs reach a story through its absolute GitHub URL instead.

   The exclude patterns are read *from the site config* rather than restated here, because a check
   that keeps its own copy of the list is a check that eventually disagrees with the thing it is
   checking.

Exit 0 when clean, 1 otherwise. Run from the repository root, or anywhere — paths are resolved
against this file's parent.
"""

from __future__ import annotations

import fnmatch
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
SKIP_DIRS = {".git", "node_modules", "build", "target", ".docusaurus"}
LINK = re.compile(r"\[([^\]]+)\]\(([^)]+)\)")
DOCS = ROOT / "docs"
SITE_CONFIG = ROOT / "website" / "docusaurus.config.js"


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


def site_excludes() -> list[str]:
    """The docs plugin's `exclude` globs, read from the site config.

    Returns `[]` if the config or the key is missing, which makes the check below inert rather
    than wrong — a repo with no website has no published/excluded distinction to enforce.
    """
    if not SITE_CONFIG.is_file():
        return []
    match = re.search(
        r"exclude:\s*\[(.*?)\]", SITE_CONFIG.read_text(encoding="utf-8"), re.S
    )
    if not match:
        return []
    return re.findall(r"['\"]([^'\"]+)['\"]", match.group(1))


def is_excluded(rel_to_docs: str, patterns: list[str]) -> bool:
    # Docusaurus matches these against the doc path relative to the docs root, and `stories/**`
    # is meant to catch `stories/RG-8-….md`. fnmatch's `*` crosses separators, so `stories/**`
    # already does; the extra `stories/*` form is here for patterns written without the doubling.
    return any(
        fnmatch.fnmatch(rel_to_docs, pattern)
        or fnmatch.fnmatch(rel_to_docs, pattern.replace("**", "*"))
        for pattern in patterns
    )


def check_site_links() -> list[str]:
    """A published doc must not relative-link into an excluded one — see the module docstring."""
    patterns = site_excludes()
    if not patterns or not DOCS.is_dir():
        return []

    problems = []
    for md in markdown_files():
        if not md.is_relative_to(DOCS):
            continue  # README.md, AGENTS.md and friends are read on GitHub, not on the site.
        source = md.relative_to(DOCS).as_posix()
        if is_excluded(source, patterns):
            continue  # An excluded page linking to another excluded page is a GitHub-only path.
        for match in LINK.finditer(md.read_text(encoding="utf-8")):
            target = match.group(2)
            if target.startswith(("http://", "https://", "#", "mailto:")):
                continue
            path = target.split("#")[0]
            if not path:
                continue
            resolved = (md.parent / path).resolve()
            if not resolved.is_relative_to(DOCS):
                continue  # Out of the docs tree entirely; the site never had a page for it.
            if is_excluded(resolved.relative_to(DOCS).as_posix(), patterns):
                problems.append(
                    f"site link  {md.relative_to(ROOT)}: [{match.group(1)}]({target}) "
                    f"— the site excludes that page; link its GitHub URL instead"
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
