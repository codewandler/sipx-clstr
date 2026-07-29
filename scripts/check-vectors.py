#!/usr/bin/env python3
"""Every spec vector row is covered by a test, or deferred with a reason.

`PX-1` writes its normative behaviour as vector tables. A table that nobody executes is prose, so
this check closes the loop: it reads the row IDs out of the specification, reads the coverage out of
the test suite, and fails when the two disagree.

Coverage is derived from **test function names** rather than from a hand-maintained list —
`fn pb_v_8_a_max_breadth_of_one_…` covers `PB-V-8`. A list would rot; a name cannot, because deleting
the test deletes the claim. Tests that cover a row without wanting the name can say so in a
`// covers: PB-R-4` comment instead.

Two row shapes are accepted, because the specs have two. Most tables number by family —
`PB-V-8`, `RA-R-7` — and one table per spec is small enough to number straight through:
`HF-9`, proved by `fn hf_9_…`. `EX-8` widened the grammar rather than renumbering `hook-framework`
into families, because a row ID is a citation: `HF-1` … `HF-8` are quoted from other specs, from
designs and from stories, and renumbering to satisfy a regex would break every one of them to buy
nothing a spec reader can see. A spec whose table grows past one family's worth of rows should be
split into families when it is written, not after it is cited.

Three ways to fail, and the third is the one that matters:

1. A spec row is neither covered nor deferred.
2. A deferred row has no reason or no story.
3. A deferred row **is** covered — a stale deferral, which is how a coverage report starts lying
   about what it proves.

Also writes `docs/reference/proxy-conformance.md`: generated, never hand-edited, and checked in so a
reader who does not run the suite can still see what is proved. `--check` fails if the committed file
is out of date, the way the story board is checked.

Exit 0 when everything agrees, 1 otherwise.
"""

from __future__ import annotations

import pathlib
import re
import sys
import tomllib

ROOT = pathlib.Path(__file__).resolve().parent.parent
SCOPE = ROOT / "docs" / "reference" / "vector-scope.toml"
REPORT = ROOT / "docs" / "reference" / "conformance.md"

# Every spec that carries a vector table, with the prefix its rows use and the section the table
# lives in — the section is quoted in the generated report, so it cannot drift from this table.
SPECS = {
    "PB": (ROOT / "docs" / "specs" / "proxy-behavior.md", "§12"),
    "EP": (ROOT / "docs" / "specs" / "e2e-probe.md", "§10"),
    "RA": (ROOT / "docs" / "specs" / "registrar-auth.md", "§8"),
    "HF": (ROOT / "docs" / "specs" / "hook-framework.md", "§9"),
}

PREFIXES = "|".join(SPECS)
# The family letter is optional: `PB-V-8` and `HF-9` are both rows. See the module docstring for
# why the grammar widened instead of the tables renumbering.
ROW = re.compile(rf"\b({PREFIXES})-(?:([A-Z])-)?(\d+)\b")
TEST_NAME = re.compile(rf"\bfn\s+({PREFIXES.lower()})_(?:([a-z])_)?(\d+)_")
COVERS = re.compile(rf"//\s*covers:\s*((?:(?:{PREFIXES})-(?:[A-Z]-)?\d+[,\s]*)+)")

FAMILIES = {
    ("PB", "V"): "Proxy — request validation (§4)",
    ("PB", "P"): "Proxy — route preprocessing (§5)",
    ("PB", "F"): "Proxy — request forwarding (§7)",
    ("PB", "R"): "Proxy — response processing (§8)",
    ("PB", "C"): "Proxy — CANCEL and Timer C (§9)",
    ("PB", "S"): "Proxy — stateless mode (§10)",
    ("PB", "A"): "Proxy — transaction affinity (§11)",
    ("EP", "P"): "Probe — a passing run (§10)",
    ("EP", "F"): "Probe — one failure per step (§10)",
    ("EP", "I"): "Probe — probe-side faults (§10)",
    ("EP", "C"): "Probe — cleanup (§10)",
    ("RA", "D"): "Registrar auth — the decision (§3)",
    ("RA", "A"): "Registrar auth — algorithm selection (§4)",
    ("RA", "R"): "Registrar auth — replay and retransmission (§3, §6, §7)",
    ("RA", "T"): "Registrar auth — the tenant boundary (§5)",
    ("HF", ""): "Hook framework — startup graph validation (§9)",
}


def canonical(prefix: str, letter: str, number: str) -> str:
    """`("pb", "v", "08")` → `PB-V-8`; `("hf", "", "09")` → `HF-9`."""
    stem = f"{prefix.upper()}-{letter.upper()}-" if letter else f"{prefix.upper()}-"
    return f"{stem}{int(number)}"


def family_of(row: str) -> tuple[str, str]:
    parts = row.split("-")
    return (parts[0], parts[1]) if len(parts) == 3 else (parts[0], "")


def sort_key(row: str) -> tuple[int, int]:
    family = family_of(row)
    order = list(FAMILIES).index(family) if family in FAMILIES else len(FAMILIES)
    return (order, int(row.split("-")[-1]))


def spec_rows() -> set[str]:
    """Every row every vector-carrying spec declares.

    Read from the spec that *owns* the prefix, so a row mentioned in passing elsewhere — a design doc
    citing `PB-F-1`, this script's own docstring — cannot invent a row nobody has to prove.
    """
    rows: set[str] = set()
    for prefix, (path, _section) in SPECS.items():
        if not path.is_file():
            continue
        text = path.read_text(encoding="utf-8")
        rows.update(
            canonical(found, letter, number)
            for found, letter, number in ROW.findall(text)
            if found == prefix
        )
    return rows


def rust_sources() -> list[pathlib.Path]:
    return sorted(
        path
        for path in (ROOT / "crates").rglob("*.rs")
        if "target" not in path.parts
    )


def covered() -> dict[str, list[str]]:
    """Row ID → the files that prove it."""
    found: dict[str, list[str]] = {}
    for path in rust_sources():
        text = path.read_text(encoding="utf-8")
        rows = {
            canonical(prefix, letter, number)
            for prefix, letter, number in TEST_NAME.findall(text)
        }
        for group in COVERS.findall(text):
            rows.update(canonical(a, b, c) for a, b, c in ROW.findall(group))
        for row in rows:
            found.setdefault(row, []).append(str(path.relative_to(ROOT)))
    return found


def deferred() -> tuple[dict[str, dict], list[str]]:
    problems: list[str] = []
    if not SCOPE.is_file():
        return {}, [f"{SCOPE.relative_to(ROOT)} is missing"]
    data = tomllib.loads(SCOPE.read_text(encoding="utf-8"))
    rows: dict[str, dict] = {}
    for entry in data.get("deferred", []):
        row = entry.get("id")
        if not row:
            problems.append("a deferred entry has no `id`")
            continue
        if not entry.get("reason", "").strip():
            problems.append(f"{row}: deferred with no reason")
        if not entry.get("story", "").strip():
            problems.append(f"{row}: deferred with no story to cover it")
        rows[row] = entry
    return rows, problems


def sources() -> str:
    """The vector tables this report covers, named from `SPECS` so the sentence cannot go stale."""
    parts = [
        f"[{path.stem}](../specs/{path.name}) {section}"
        for path, section in SPECS.values()
        if path.is_file()
    ]
    return ", ".join(parts[:-1]) + f" and {parts[-1]}" if len(parts) > 1 else parts[0]


def render(rows: set[str], proofs: dict[str, list[str]], waived: dict[str, dict]) -> str:
    lines = [
        "# Conformance — every spec vector, and what proves it",
        "",
        "**Generated by `scripts/check-vectors.py`. Do not hand-edit.**",
        "",
        f"One row per vector in {sources()}.",
        "",
        "A row is *proved* when a test in the workspace covers it, and *deferred* when",
        "[vector-scope.toml](vector-scope.toml) says why and names the story that will.",
        "",
        f"**{len(rows) - len(waived)} of {len(rows)} rows proved**; {len(waived)} deferred.",
        "",
    ]
    for family_key, title in FAMILIES.items():
        family = sorted(
            (row for row in rows if family_of(row) == family_key), key=sort_key
        )
        if not family:
            continue
        lines += [f"## {title}", "", "| Row | Status | Proved by / deferred to |", "|---|---|---|"]
        for row in family:
            if row in proofs:
                where = ", ".join(f"`{path}`" for path in sorted(set(proofs[row])))
                lines.append(f"| `{row}` | proved | {where} |")
            else:
                entry = waived.get(row, {})
                story = entry.get("story", "—")
                reason = " ".join(entry.get("reason", "").split())
                lines.append(f"| `{row}` | deferred | `{story}` — {reason} |")
        lines.append("")
    return "\n".join(lines)


def main() -> int:
    check_only = "--check" in sys.argv

    rows = spec_rows()
    if not rows:
        print("vectors: FAIL — no rows found in any spec", file=sys.stderr)
        return 1

    proofs = covered()
    waived, problems = deferred()

    for row in sorted(rows, key=sort_key):
        if row not in proofs and row not in waived:
            problems.append(f"{row}: in the spec, covered by no test, and not deferred")

    # A stale deferral is the failure mode that makes a report lie: it says "not proved yet" about
    # something that is, so nobody notices when the real gap reopens.
    for row in sorted(set(waived) & set(proofs), key=sort_key):
        problems.append(
            f"{row}: deferred in vector-scope.toml but covered by "
            f"{', '.join(sorted(set(proofs[row])))} — remove the deferral"
        )

    for row in sorted(set(proofs) - rows, key=sort_key):
        problems.append(f"{row}: covered by a test but not a row in the spec")

    report = render(rows, proofs, waived)
    if check_only:
        current = REPORT.read_text(encoding="utf-8") if REPORT.is_file() else ""
        if current != report:
            problems.append(
                f"{REPORT.relative_to(ROOT)} is out of date — run scripts/check-vectors.py"
            )
    else:
        REPORT.write_text(report, encoding="utf-8")

    for problem in problems:
        print(problem)
    if problems:
        print(f"\nvectors: FAIL — {len(problems)} problem(s)")
        return 1

    print(
        f"vectors: {len(rows) - len(waived)}/{len(rows)} rows proved, "
        f"{len(waived)} deferred with a reason"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
