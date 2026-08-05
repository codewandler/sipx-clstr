#!/usr/bin/env python3
"""Documentation consistency: the half of the gate that is not Rust.

There are two documentation trees and they are published differently. `website/docs/` is the
public site, authored for end users. `docs/` is internal contributor material — the story board,
the roadmap, design records, the normative specs — and **none of it is published**. A broken
relative link in either is cheap to check and expensive to notice late.

Five checks:

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

5. **Every table holds together as a table** (`CF-23`). A Markdown table cannot survive a blank
   line: `RG-25` inserted its explanatory paragraph between the two data rows of location-service
   §5.7, `AfterRegistrarUpdate` stopped being a row and started being literal pipe text — while
   the sentence above it still said the spec "names exactly the two hook phases", and this check's
   predecessor reported the tree clean. In a repository whose rules are cited by ID out of table
   rows, a row that has silently stopped being a row is a normative rule that has silently stopped
   being readable. Three defects, all one class:

   - an **orphaned row** — a `|` line at the start of a block that no separator row follows;
   - a **split table** — header and separator with a blank line between them;
   - a **ragged row** — a row whose cell count differs from its header's. GFM pads a short row
     with empty cells and *silently drops* a long row's extras, so an unescaped `|` in a cell —
     even inside a code span, where only `\\|` is literal — truncates the sentence it sits in on
     the rendered page with nothing red anywhere.

   Every failure names the file, the line, and the row — "malformed table" would send the reader
   off to re-derive exactly the diagnosis the check already made. The self-test replays the
   `RG-25` shape (and each sibling defect) against fixtures on every run, because a structure
   check that has quietly stopped parsing tables is this file's own §3 failure mode again.

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


def without_fences(text: str) -> str:
    """`text` with fenced code blocks removed, newlines preserved so line numbers hold.

    Split out of `prose` for the table check, which needs exactly half of what `prose` does:
    a fenced block quoting a broken table must not be a defect, but inline code spans have to
    survive, because a `|` inside one still splits a table cell (GFM escapes pipes with `\\|`,
    including inside code spans) and stripping the span would hide exactly the truncation the
    check exists to catch.
    """
    return re.sub(
        r"^[ \t]*(`{3,}|~{3,}).*?^[ \t]*\1[ \t]*$",
        lambda m: "\n" * m.group(0).count("\n"),
        text,
        flags=re.S | re.M,
    )


def prose(text: str) -> str:
    """`text` with fenced blocks and inline code spans removed.

    A link checker that reads a code sample as a link cannot be used to *write about* link defects:
    quoting a real "broken link ..." error message inside a story file made this gate fail on the
    story that documented the failure. Code is not prose and its brackets are not links.

    Newlines are preserved so nothing downstream shifts; only the code itself goes.
    """
    # Indented code blocks are not stripped: four-space indentation is also how this repository
    # wraps continuation lines in frontmatter and list items, and those do carry real links.
    return re.sub(r"`+[^`\n]*`+", "", without_fences(text))


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


# ── table structure (CF-23) ──────────────────────────────────────────────────────────────────────

# A GFM delimiter cell: optional colons around one or more dashes. Matched per cell rather than as
# one whole-line pattern, so `|---|:---:|` and `| --- | --- |` both read as separator rows.
SEPARATOR_CELL = re.compile(r":?-+:?")
# `\|` is the one escape GFM honors inside a table — including inside a code span. Swapped for a
# sentinel before splitting so it counts as cell content, and swapped back for quoting.
ESCAPED_PIPE = "\x00"


def table_cells(line: str) -> list[str]:
    """The line's cells, split the way GFM splits them.

    An unescaped `|` always delimits — a code span does not protect it. That is how e2e-probe §7's
    A7 row grew a third cell out of a code span's pipe and lost the second half of its sentence on
    the rendered site: GFM drops the cells a row has beyond its header's width.
    """
    stripped = line.strip().replace("\\|", ESCAPED_PIPE)
    if stripped.startswith("|"):
        stripped = stripped[1:]
    if stripped.endswith("|"):
        stripped = stripped[:-1]
    return [cell.replace(ESCAPED_PIPE, "\\|").strip() for cell in stripped.split("|")]


def is_separator_row(line: str) -> bool:
    # `[""]` — what a bare `|` yields — fails the fullmatch, so a line of pipes is not a separator.
    return all(SEPARATOR_CELL.fullmatch(cell) for cell in table_cells(line))


def shown(line: str) -> str:
    """The row, short enough to keep the problem on one line."""
    row = line.strip()
    return row if len(row) <= 60 else row[:57] + "..."


def table_problems(text: str) -> list[tuple[int, str, str]]:
    """`(line number, label, problem)` for every table-structure defect in `text`.

    Reads the fence-stripped text, so a fenced block may quote a broken table — but keeps inline
    code spans, because a pipe inside one still splits a cell (see `without_fences`). Line numbers
    are 1-based positions in the original file; `without_fences` preserves them.
    """
    problems: list[tuple[int, str, str]] = []
    lines = without_fences(text).splitlines()
    total = len(lines)
    # The top of the file starts a block exactly the way a blank line does.
    previous_blank = True
    i = 0
    while i < total:
        stripped = lines[i].strip()
        if not stripped.startswith("|"):
            previous_blank = not stripped
            i += 1
            continue

        if i + 1 < total and not is_separator_row(lines[i]) and is_separator_row(lines[i + 1]):
            # A well-formed table start — header, separator directly under it. Checked wherever it
            # stands (a table directly under a heading renders fine); everything below is held to
            # the header's width.
            width = len(table_cells(lines[i]))
            if (separator := len(table_cells(lines[i + 1]))) != width:
                problems.append((
                    i + 2,
                    "ragged table",
                    f"separator has {separator} column(s), its header has {width} — GFM does not "
                    f"recognize the table at all, so every row renders as literal pipe text",
                ))
            j = i + 2
            while j < total and lines[j].strip().startswith("|"):
                if (count := len(table_cells(lines[j]))) != width:
                    fate = (
                        "silently drops the extra cells"
                        if count > width
                        else "pads the row with empty cells"
                    )
                    problems.append((
                        j + 1,
                        "ragged row",
                        f"{count} cell(s) against a {width}-column header — GFM {fate}; an "
                        f"unescaped `|` splits even inside a code span, `\\|` is the escape "
                        f"— {shown(lines[j])}",
                    ))
                j += 1
            i = j
            previous_blank = False
            continue

        if not previous_blank:
            # A pipe line glued to the bottom of a non-table block is a lazy continuation of that
            # block. The orphan rule is about rows a blank line cut loose, and flagging prose that
            # merely mentions a pipe would get this check argued with rather than obeyed.
            i += 1
            continue

        if is_separator_row(lines[i]):
            problems.append((
                i + 1,
                "headless table",
                f"separator row with no header directly above it — GFM renders the whole block "
                f"as literal pipe text — {shown(lines[i])}",
            ))
        else:
            # A pipe line opening a block, with no separator under it. A separator waiting across
            # the blank line means a header was torn from its table; anything else is the RG-25
            # defect itself — a row that has silently stopped being a row.
            after_gap = i + 1
            while after_gap < total and not lines[after_gap].strip():
                after_gap += 1
            if after_gap > i + 1 and after_gap < total and is_separator_row(lines[after_gap]):
                problems.append((
                    i + 1,
                    "split table",
                    f"header and its separator (line {after_gap + 1}) are separated by a blank "
                    f"line — GFM reads the header as a paragraph of literal pipe text "
                    f"— {shown(lines[i])}",
                ))
                i = after_gap
            else:
                problems.append((
                    i + 1,
                    "orphaned row",
                    f"a blank line cut this row off from its table, so it renders as literal "
                    f"pipe text, not as a row — {shown(lines[i])}",
                ))
        # One problem per defect: the rest of the pipe block is the same tear, already diagnosed.
        while i < total and lines[i].strip().startswith("|"):
            i += 1
        previous_blank = False
    return problems


def check_tables() -> list[str]:
    """Every table holds together as a table — see the module docstring, item 5."""
    problems = []
    for md in markdown_files():
        rel = md.relative_to(ROOT)
        for line, label, problem in table_problems(md.read_text(encoding="utf-8")):
            problems.append(f"{label}  {rel}:{line}: {problem}")
    return problems


# ── the self-test (CF-23) ────────────────────────────────────────────────────────────────────────
#
# `RG-25`'s defect, replayed on every run: the first fixture is location-service §5.7 as the
# pre-fix tree had it, an explanatory paragraph between the two hook-phase rows. The branch that
# carried the real thing was amended away (`02a0228` is unreachable), so the shape is pinned here
# instead. The siblings are the rest of the class, and the clean fixture holds the parser to what
# it must NOT flag — a structure check with a false positive gets argued with rather than obeyed.

ORPHANED_ROW_FIXTURE = """\
### 5.7 Extension points

The registrar path exposes exactly the two hook phases:

| Phase (hook-framework) | Anchor here |
|---|---|
| `BeforeRegistrarUpdate` | After S6 — and before S6.1–S10 |

**[sipx-clstr]** An explanatory paragraph, inserted exactly where RG-25 put its own.

| `AfterRegistrarUpdate` | After S10 — and before the response is sent |
"""

SPLIT_TABLE_FIXTURE = """\
| # | Rule |

|---|---|
| Q1 | The rule the ID cites |
"""

HEADLESS_TABLE_FIXTURE = """\
Prose above.

|---|---|
| Q1 | The rule the ID cites |
"""

RAGGED_ROW_FIXTURE = """\
| # | Rule |
|---|---|
| A7 | carries `trigger: scheduled | api` so the provenance is visible |
| A8 | intact |
| A9 |
"""

RAGGED_SEPARATOR_FIXTURE = """\
| a | b |
|---|---|---|
| 1 | 2 |
"""

CLEAN_FIXTURE = """\
Prose before.

| Field | Form |
|---|---|
| `algorithm` | `chacha20-poly1305` \\| `hmac-sha256-96` |

A fenced block may quote anything, broken tables included:

```text
| orphaned | row |

| not | a | table |
```

### A heading directly above a table
| a | b |
|---|---|
| 1 | 2 |
"""


def self_test() -> list[str]:
    failures: list[str] = []

    def check_that(claim: str, held: bool) -> None:
        if not held:
            failures.append(claim)

    found = table_problems(ORPHANED_ROW_FIXTURE)
    check_that(
        f"RG-25's shape is exactly one orphaned row at line 11, found {found}",
        [(line, label) for line, label, _ in found] == [(11, "orphaned row")],
    )
    check_that(
        "the message names the row that was orphaned, not merely 'malformed table'",
        bool(found) and "AfterRegistrarUpdate" in found[0][2],
    )

    found = table_problems(SPLIT_TABLE_FIXTURE)
    check_that(
        f"a header torn from its separator is one split table, found {found}",
        [(line, label) for line, label, _ in found] == [(1, "split table")],
    )

    found = table_problems(HEADLESS_TABLE_FIXTURE)
    check_that(
        f"a separator with no header is one headless table, found {found}",
        [(line, label) for line, label, _ in found] == [(3, "headless table")],
    )

    found = table_problems(RAGGED_ROW_FIXTURE)
    check_that(
        f"an unescaped code-span pipe and a short row are two ragged rows, found {found}",
        [(line, label) for line, label, _ in found]
        == [(3, "ragged row"), (5, "ragged row")],
    )
    check_that(
        "the ragged-row message quotes the row it is about",
        len(found) == 2 and "scheduled" in found[0][2] and "A9" in found[1][2],
    )

    found = table_problems(RAGGED_SEPARATOR_FIXTURE)
    check_that(
        f"a separator wider than its header is one ragged table, found {found}",
        [(line, label) for line, label, _ in found] == [(2, "ragged table")],
    )

    found = table_problems(CLEAN_FIXTURE)
    check_that(
        f"escaped pipes, fenced quotes and heading-adjacent tables are clean, found {found}",
        found == [],
    )
    return failures


def main() -> int:
    if failures := self_test():
        for failure in failures:
            print(f"self-test: {failure}")
        print("\ndocs: FAIL — the check cannot detect what it was built to detect")
        return 1
    if "--self-test" in sys.argv:
        print("docs: self-test passed — every table defect the fixtures replay is still refused")
        return 0

    problems = (
        check_links() + check_stories() + check_spec_deferrals() + check_site_links() + check_tables()
    )
    for problem in problems:
        print(problem)
    if problems:
        print(f"\ndocs: FAIL — {len(problems)} problem(s)")
        return 1
    print(f"docs: clean ({len(markdown_files())} markdown files checked)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
