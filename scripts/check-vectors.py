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

A name is not an assertion, though, and `CF-12` is about the gap that leaves. `PB-F-1` read "Timer C
set 180 s" and its test asserted `[Kind::ResolveTargets, Kind::Forward, Kind::SetTimer]` — effect
*kinds*, no duration anywhere. A row claiming 180 s and code arming 180 s coexisted for the
project's whole life without ever being compared, and when `PX-10` found the value the row had been
silently wrong about, the "proof" would not have noticed either way. So coverage answers *does a
test claim this row*, and a second question is asked beside it: **does that test compare what the
row states?**

The rule is deliberately narrow, because a wide one would catch nothing worth catching. Plenty of
rows are about ordering, shape or refusal, where insisting on a quantity would be meaningless — so
the check only has an opinion where the row itself states a number:

- the **claim** is the row's `Expect` cell, minus its citations. `RFC 3261`, `§16.6 step 11`, a rule
  name like `A6` or `F11`, another row's ID — those are numbers a row points at, not ones it pins.
- the **proof** is the compared arguments of the assertions in the test's body: argument 1 of
  `assert!`, arguments 1 and 2 of `assert_eq!`/`assert_ne!`/`assert_matches!`. The signature line,
  the comments and each assertion's *message* argument are dropped on purpose — otherwise quoting
  the row in a comment, or naming the value in a panic message, would buy a proof, and the check
  would measure how the test is written about rather than what it compares.
- a row whose stated value nothing compares is **shape only**: covered, running, and not proving
  the quantity it exists to pin. It is not counted as proved.

Ways to fail, and the last two are the ones that matter:

1. A spec row is neither covered nor deferred.
2. A deferred row has no reason or no story.
3. A deferred row **is** covered — a stale deferral, which is how a coverage report starts lying
   about what it proves.
4. A covered row states a value that no assertion in its proof compares, and `vector-scope.toml`
   does not already record it as shape only. This is what stops a new row joining the class.
5. A row recorded as shape only that **is** now asserted, or is not covered at all — the same stale
   entry rule as a deferral, and what makes that list a ratchet that can only shrink.
6. A table row *shaped* like a vector whose prefix no spec owns. Reading only the owning spec is
   what stops a design inventing rows, and its cost is silence the other way: `EX-12` found 31
   `QP-*` rows in a design record, looking exactly like vectors, read by nothing — a fabricated
   `QP-Z-999` among them passed `--check` too. Registration is not something to remember now.

Also writes `docs/reference/conformance.md`: generated, never hand-edited, and checked in so a
reader who does not run the suite can still see what is proved. `--check` fails if the committed file
is out of date, the way the story board is checked.

`--self-test` replays the `PB-F-1` case as it stood at `7d21a22`, from the spec row and the test
body of the day, and asserts this script refuses it. It runs on every invocation: the check that a
detector still detects is worth more than the detector.

Exit 0 when everything agrees, 1 otherwise.
"""

from __future__ import annotations

import pathlib
import re
import sys
import tomllib
import typing

ROOT = pathlib.Path(__file__).resolve().parent.parent
SCOPE = ROOT / "docs" / "reference" / "vector-scope.toml"
REPORT = ROOT / "docs" / "reference" / "conformance.md"

# Every spec that carries a vector table, with the prefix its rows use and the section the table
# lives in — the section is quoted in the generated report, so it cannot drift from this table.
# Every spec that carries a vector table, and the section its table lives in.
#
# A prefix is owned by exactly one spec, and `spec_rows` reads rows only from the owner — so a design
# doc citing `PB-F-1`, or this script's own docstring, cannot invent a row somebody has to prove. The
# convention that makes this safe: a spec's *rules* are named bare (`V1`, `D3`) and only its *vectors*
# carry the prefix (`CC-V-1`). A rule citation like `CC-V2` has no second hyphen and does not match.
#
# Six of these were unregistered until `CF-8`, which meant ~394 normative rows were prose nothing
# executed. Demonstrated rather than inferred: a fabricated `LS-R-999` passed the gate untouched.
SPECS = {
    "PB": (ROOT / "docs" / "specs" / "proxy-behavior.md", "§12"),
    "EP": (ROOT / "docs" / "specs" / "e2e-probe.md", "§10"),
    "RA": (ROOT / "docs" / "specs" / "registrar-auth.md", "§8"),
    "HF": (ROOT / "docs" / "specs" / "hook-framework.md", "§9"),
    "LS": (ROOT / "docs" / "specs" / "location-service.md", "§9"),
    "MR": (ROOT / "docs" / "specs" / "media-relay.md", "§12"),
    "NN": (ROOT / "docs" / "specs" / "number-normalisation.md", "§11"),
    "AT": (ROOT / "docs" / "specs" / "affinity-token.md", "§10"),
    "FR": (ROOT / "docs" / "specs" / "affinity-token.md", "§10"),
    "CC": (ROOT / "docs" / "specs" / "cluster-config.md", "§12"),
    "AI": (ROOT / "docs" / "specs" / "asserted-identity.md", "§13"),
    # `QP` is `hook-framework`'s second table, and the second prefix that spec owns. Registered by
    # `EX-12`, which had to move the rows out of `docs/designs/extension-framework.md` first: a
    # design record is not normative, `spec_rows` reads only the owner, and a prefix aimed at
    # `docs/designs/` would have made a design normative by accident. Same demonstration as `CF-8`'s
    # — a fabricated `QP-Z-999` row passed `--check` until the rows had a spec to live in.
    "QP": (ROOT / "docs" / "specs" / "hook-framework.md", "§9.1"),
}

PREFIXES = "|".join(SPECS)
# The family letter is optional: `PB-V-8` and `HF-9` are both rows. See the module docstring for
# why the grammar widened instead of the tables renumbering.
ROW = re.compile(rf"\b({PREFIXES})-(?:([A-Z])-)?(\d+)\b")
COVERS = re.compile(rf"//\s*covers:\s*((?:(?:{PREFIXES})-(?:[A-Z]-)?\d+[,\s]*)+)")

# A vector-table line whose *first* cell is a row ID — as opposed to a line that merely cites one,
# which is why the ID has to be anchored at the start rather than found anywhere.
CLAIM_LINE = re.compile(rf"^\|\s*`?({PREFIXES})-(?:([A-Z])-)?(\d+)`?\s*\|(.*)$")

# `CLAIM_LINE` with the prefix left open. A table row in that shape is a vector by every visual
# convention this repo has, so one whose prefix `SPECS` does not own is a row no gate can read —
# which is the whole `EX-12` defect, and the reason it survived two stories: `QP` rows looked
# exactly like `HF` rows and were checked by nothing. Anchored at the start for `CLAIM_LINE`'s
# reason: a line that merely cites a row is not a row.
ANY_ROW_LINE = re.compile(r"^\|\s*`?([A-Z]{2})-(?:[A-Z]-)?\d+`?\s*\|")
# Where a vector table may legitimately live. Designs are scanned too, and that is the point.
TABLE_TREES = ("docs/specs", "docs/designs")

# A citation is not a claim. Every one of these is a number the row *points at* — a document, a
# section, a rule, another row, a digest width — rather than a quantity it pins, and leaving them in
# would flood the check with values no test could sensibly compare.
CITATIONS = (
    re.compile(rf"\b({PREFIXES})-(?:[A-Z]-)?\d+\b"),
    re.compile(r"\bRFC\s*\d+\b", re.IGNORECASE),
    re.compile(r"§\s*[\dA-Za-z]+(?:\.[\dA-Za-z]+)*"),
    re.compile(r"\bstep\s+\d+\b", re.IGNORECASE),
    re.compile(r"\b[A-Z]\d+\b"),
    re.compile(r"\bSHA-\d+(?:-\d+)?\b", re.IGNORECASE),
    re.compile(r"\bMD5\b", re.IGNORECASE),
    re.compile(r"\bIPv\d\b", re.IGNORECASE),
    re.compile(r"\bbase\s*\d+\b", re.IGNORECASE),
)
# Not preceded by a word character, a dot, a hyphen or a slash: the tail of `10.0.0.1`, of `SIP/2.0`
# and of a hyphenated identifier are parts of a name, not values in their own right. The unit is
# captured because a spec states a timer in seconds and the code holds it in milliseconds — see
# `EQUIVALENTS`, without which `CC-V-12`'s "240 s" would miss its own `timer_c_ms == 240_000`.
STATED = re.compile(
    r"(?<![\w.\-/])(\d+(?:[_.]\d+)*)(?:\s*(ms|seconds?|minutes?|bytes?|s|B|%)\b)?"
)
# Unit → the multipliers that spell the same quantity. Conversion is arithmetic, not interpretation,
# so widening the check this way costs no honesty: `240 s` and `240_000` really are one value.
EQUIVALENTS = {
    "": (1,),
    "s": (1, 1_000),
    "second": (1, 1_000),
    "seconds": (1, 1_000),
    "minute": (1, 60, 60_000),
    "minutes": (1, 60, 60_000),
    "ms": (1,),
    "B": (1,),
    "byte": (1,),
    "bytes": (1,),
    "%": (1,),
}

FN_LINE = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+(\w+)")
NAMED_FOR = re.compile(rf"^({PREFIXES.lower()})_(?:([a-z])_)?(\d+)_")
LINE_COMMENT = re.compile(r"//.*$", re.MULTILINE)
ASSERTION = re.compile(r"\b(?:debug_)?assert(_eq|_ne|_matches)?!\s*\(")
# How many leading arguments each assertion *compares*. Everything after them is the failure
# message, which is prose about the test rather than part of it.
COMPARED_ARGS = {None: 1, "_eq": 2, "_ne": 2, "_matches": 2}

PROVED, SHAPE_ONLY, UNREADABLE = "proved", "shape only", "unreadable"

# Row family → the section title the report gives it. `render` iterates this, so a family absent
# here is a family the report cannot show — which was `CF-17`: `CF-8` registered seven prefixes in
# `SPECS` and named none of their families here, so 395 of 533 rows were counted in the denominator
# and printed in no table. `unrepresented_families` now refuses that state, because the failure was
# silent in the direction that flatters the report: every other part of the script saw those rows.
#
# Order is load-bearing twice over — `render` emits sections in this order and `sort_key` reads row
# order from it — so families are grouped by prefix in `SPECS` registration order, and within a
# prefix in the order that prefix's own spec tabulates them. The section each title cites is the one
# that *governs* the family, not the one the vector table lives in: a reader checking `MR-E` wants
# §6's encoding rules, not §12.2 again.
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
    ("RA", "L"): "Registrar auth — the audit trail (§9)",
    ("HF", ""): "Hook framework — startup graph validation (§9)",
    ("QP", "A"): "Carrier quirks — one profile applied (§9.1)",
    ("QP", "C"): "Carrier quirks — composition (§9.1)",
    ("QP", "G"): "Carrier quirks — startup validation, G9–G14 (§9.1)",
    ("LS", "C"): "Location service — AoR canonicalization (§3)",
    ("LS", "R"): "Location service — REGISTER processing and the CAS contract (§5)",
    ("LS", "K"): "Location service — the consistency contract (§6)",
    ("LS", "L"): "Location service — the lookup contract (§7)",
    ("LS", "H"): "Location service — the sharding key (§8)",
    ("MR", "T"): "Media relay — port semantics (§3)",
    ("MR", "N"): "Media relay — the null relay (§4)",
    ("MR", "E"): "Media relay — NG encoding, byte-exact (§6)",
    ("MR", "X"): "Media relay — exchange and timers (§8)",
    ("MR", "F"): "Media relay — faults and the error taxonomy (§9)",
    ("MR", "H"): "Media relay — health signals and node state (§10)",
    ("MR", "P"): "Media relay — per-trunk media policy (§13)",
    ("MR", "C"): "Media relay — media-policy configuration (§13.5)",
    ("NN", "X"): "Number normalisation — extraction and URI forms (§4)",
    ("NN", "T"): "Number normalisation — the closed transform set (§5)",
    ("NN", "G"): "Number normalisation — guards and fallback (§6)",
    ("NN", "E"): "Number normalisation — E.164 egress (§9)",
    ("NN", "C"): "Number normalisation — composition (§7)",
    ("NN", "B"): "Number normalisation — binding and the pipeline (§8)",
    ("AT", ""): "Affinity token — mint, parse and negative vectors (§10)",
    ("FR", ""): "Flow reference — mint, resolution and negative vectors (§14)",
    ("CC", "D"): "Cluster config — the document, versioning and substitution (§3)",
    ("CC", "R"): "Cluster config — roles and projection (§4, §5)",
    ("CC", "I"): "Cluster config — logical ids (§6)",
    ("CC", "V"): "Cluster config — validation across sections (§8)",
    ("CC", "K"): "Cluster config — key reload (§9.3)",
    ("CC", "T"): "Cluster config — trunk reload (§9.2)",
    ("CC", "S"): "Cluster config — shard-map handoff (§9.4)",
    ("AI", "D"): "Asserted identity — the `PaiRequest` derivation (§4)",
    ("AI", "T"): "Asserted identity — the emission gate, trust × privacy (§8)",
    ("AI", "S"): "Asserted identity — selection (§7)",
    ("AI", "A"): "Asserted identity — anonymous callers and `From` (§9)",
    ("AI", "N"): "Asserted identity — the normalisation seam (§6.1)",
    ("AI", "C"): "Asserted identity — `Privacy: critical` (§10)",
    ("AI", "P"): "Asserted identity — pipeline and binding (§6, §11)",
    ("AI", "X"): "Asserted identity — byte-exact forms (§7.1)",
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


def unowned_rows() -> list[str]:
    """Table rows shaped like vectors whose prefix no spec owns.

    `spec_rows` reads only the owner of a prefix, which is what stops a design doc inventing a row.
    The cost of that is silence in the other direction: a table of `QP-*` rows sat in
    `docs/designs/extension-framework.md` through two stories, looking exactly like a vector table,
    proving nothing, and passing `--check` — a fabricated row among them passed too. Registration is
    therefore not something to remember; an unowned row is an error with the fix in the message.
    """
    problems: list[str] = []
    for tree in TABLE_TREES:
        for path in sorted((ROOT / tree).rglob("*.md")):
            for number, line in enumerate(
                path.read_text(encoding="utf-8").splitlines(), start=1
            ):
                found = ANY_ROW_LINE.match(line)
                if found and found.group(1) not in SPECS:
                    problems.append(
                        f"{path.relative_to(ROOT)}:{number}: a table row with the shape of a "
                        f"vector, but no spec owns the prefix `{found.group(1)}` — register it in "
                        f"SPECS against the spec that states it, or stop writing it as a row"
                    )
    return problems


def unrepresented_families(rows: set[str]) -> list[str]:
    """Row families the report has no section for.

    `render` iterates `FAMILIES`, so a `(prefix, letter)` pair missing from it is a pair whose rows
    are counted by every other part of this script and then printed in no table. That was silent
    until `CF-17`: registering a prefix in `SPECS` got its rows read, counted, deferred and gated,
    and getting them onto the page was a second step nothing checked. `CF-8` registered seven
    prefixes, named none of their families, ticked both halves of its Acceptance, and left 395 of
    533 rows in the denominator and out of the document.

    The direction of the failure is why it needs a gate rather than care: the report kept *counting*
    the rows, so the headline number stayed honest while the tables quietly stopped backing it up.
    Being registered but unrepresentable is therefore an error with the fix in the message — the
    same shape as `unowned_rows`, one seam further along.
    """
    counted: dict[tuple[str, str], int] = {}
    for row in rows:
        family = family_of(row)
        counted[family] = counted.get(family, 0) + 1
    problems: list[str] = []
    for family in sorted(family for family in counted if family not in FAMILIES):
        prefix, letter = family
        label = f"{prefix}-{letter}" if letter else f"{prefix}-"
        # `spec_rows` only yields rows whose prefix `SPECS` owns, so the fallback is unreachable
        # today. It is here because a gate that tracebacks tells the reader less than one that
        # prints, and this function's whole job is to turn a silent state into a legible message.
        owner = SPECS[prefix][0].relative_to(ROOT) if prefix in SPECS else "an unregistered spec"
        problems.append(
            f"{label}: {counted[family]} rows in {owner} and no FAMILIES entry, so the report "
            f"counts them and shows them in no table — give the family a section title in FAMILIES"
        )
    return problems


def stated(expect: str) -> list[str]:
    """The values an `Expect` cell states, in the order it states them, citations removed.

    Each is written back with its unit, so the report quotes the row's own words: `"→ Respond
    `483`"` → `["483"]`; `"Timer C set 180 s"` → `["180 s"]`; `"as `503` from branch → R8"` →
    `["503"]`, because `R8` is a rule name and not a quantity.
    """
    for citation in CITATIONS:
        expect = citation.sub(" ", expect)
    found = [
        f"{number} {unit}".strip() for number, unit in STATED.findall(expect)
    ]
    return list(dict.fromkeys(found))


def claims() -> dict[str, list[str]]:
    """Row ID → the values its `Expect` column states.

    Only the *last* cell of the row's table line is read. The `Given` column is stimulus — a test
    builds it, and a value it contains says nothing about what the test checked afterwards.

    A row absent from this map has no table line in its owning spec: it is cited in prose but never
    tabulated, so there is no claim to read and no way to judge a proof of it.
    """
    values: dict[str, list[str]] = {}
    for prefix, (path, _section) in SPECS.items():
        if not path.is_file():
            continue
        for line in path.read_text(encoding="utf-8").splitlines():
            found = CLAIM_LINE.match(line)
            if not found or found.group(1) != prefix:
                continue
            cells = [cell.strip() for cell in found.group(4).split("|") if cell.strip()]
            if not cells:
                continue
            row = canonical(found.group(1), found.group(2), found.group(3))
            values[row] = list(
                dict.fromkeys(values.get(row, []) + stated(cells[-1]))
            )
    return values


def skip_string(text: str, index: int) -> int:
    """The index just past the `"`-delimited literal opening at `text[index]`.

    Only `"` is honoured. Rust's `'` is a lifetime at least as often as it is a character, and
    treating `&'static str` as an unterminated literal would swallow the rest of the file.
    """
    index += 1
    while index < len(text) and text[index] != '"':
        index += 2 if text[index] == "\\" else 1
    return index + 1


def arguments(inner: str) -> list[str]:
    """`inner` split on its top-level commas."""
    args: list[str] = []
    depth = start = index = 0
    while index < len(inner):
        char = inner[index]
        if char == '"':
            index = skip_string(inner, index)
            continue
        if char in "([{":
            depth += 1
        elif char in ")]}":
            depth -= 1
        elif char == "," and depth == 0:
            args.append(inner[start:index])
            start = index + 1
        index += 1
    args.append(inner[start:])
    return args


def call_body(text: str, open_paren: int) -> str:
    """What sits between `text[open_paren]` and the `)` that closes it."""
    depth = 0
    index = open_paren
    while index < len(text):
        char = text[index]
        if char == '"':
            index = skip_string(text, index)
            continue
        if char in "([{":
            depth += 1
        elif char in ")]}":
            depth -= 1
            if depth == 0:
                return text[open_paren + 1 : index]
        index += 1
    return text[open_paren + 1 :]


def function_body(lines: list[str], start: int) -> str:
    """The body of the function whose signature is `lines[start]`, signature line excluded.

    `cargo fmt --check` is in the gate, so the closing brace of an item is the first later line that
    is exactly its indentation plus `}` — which makes this reliable without parsing Rust. The
    signature is dropped deliberately: a test *name* is the coverage claim this check exists to look
    behind, so letting the name supply the value would make the whole thing circular.
    """
    closing = " " * (len(lines[start]) - len(lines[start].lstrip())) + "}"
    end = start + 1
    while end < len(lines) and lines[end] != closing:
        end += 1
    return "\n".join(lines[start + 1 : end])


def compared(body: str) -> str:
    """The arguments the assertions in `body` actually compare.

    Comments go first, then each assertion's message argument. Both are prose *about* the test:
    a comment quoting the row it proves, or a `panic!("expected 423, got {other:?}")` whose 423 is
    never compared to anything, must not be able to buy a proof.
    """
    text = LINE_COMMENT.sub("", body)
    kept: list[str] = []
    for macro in ASSERTION.finditer(text):
        inner = call_body(text, macro.end() - 1)
        kept.extend(arguments(inner)[: COMPARED_ARGS[macro.group(1)]])
    return "\n".join(kept)


def grouped(number: int) -> str:
    """`3600` → `3_600`, Rust's digit separator, so the two spellings are one value."""
    text = str(number)
    parts = []
    while len(text) > 3:
        parts.insert(0, text[-3:])
        text = text[:-3]
    parts.insert(0, text)
    return "_".join(parts)


def compares(text: str, value: str) -> bool:
    """Does `text` compare the quantity `value` states, in any spelling of it?"""
    numeral, _, unit = value.partition(" ")
    digits = numeral.replace("_", "")
    forms = {numeral, digits}
    if digits.isdigit():
        for multiplier in EQUIVALENTS.get(unit, (1,)):
            scaled = int(digits) * multiplier
            forms.update({str(scaled), grouped(scaled)})
    return any(
        re.search(rf"(?<![\w.]){re.escape(form)}(?![\w])", text) for form in forms
    )


class Proof(typing.NamedTuple):
    file: str
    test: str
    compared: str


def rust_sources() -> list[pathlib.Path]:
    return sorted(
        path
        for path in (ROOT / "crates").rglob("*.rs")
        if "target" not in path.parts
    )


def covered() -> dict[str, list[Proof]]:
    """Row ID → what proves it: the file, the test, and the text its assertions compare."""
    found: dict[str, list[Proof]] = {}
    for path in rust_sources():
        name = str(path.relative_to(ROOT))
        lines = path.read_text(encoding="utf-8").splitlines()
        functions = {
            index: signature.group(1)
            for index, line in enumerate(lines)
            if (signature := FN_LINE.match(line))
        }

        def proof(index: int) -> Proof:
            return Proof(name, functions[index], compared(function_body(lines, index)))

        for index, function in functions.items():
            named = NAMED_FOR.match(function)
            if named:
                prefix, letter, number = named.groups()
                row = canonical(prefix, letter or "", number)
                found.setdefault(row, []).append(proof(index))

        # A `// covers:` comment sits on the test it speaks for, so the assertions that back it are
        # the next function's — and a comment with no function after it proves nothing to read.
        for index, line in enumerate(lines):
            declared = COVERS.search(line)
            if not declared:
                continue
            following = next((at for at in sorted(functions) if at >= index), None)
            attributed = proof(following) if following is not None else Proof(name, "", "")
            for prefix, letter, number in ROW.findall(declared.group(1)):
                found.setdefault(canonical(prefix, letter, number), []).append(attributed)
    return found


def verdict(row: str, proofs: list[Proof], claimed: dict[str, list[str]]) -> tuple[str, list[str]]:
    """How strongly `proofs` prove `row`, and the values that decided it.

    `(PROVED, values)` — the row states nothing measurable, or every value it states is compared.
    `(SHAPE_ONLY, missing)` — it states values its proofs never compare.
    `(UNREADABLE, [])` — it has no table line, so there is no claim to read. Not proved: a row this
    script cannot classify is one it cannot vouch for.
    """
    if row not in claimed:
        return UNREADABLE, []
    values = claimed[row]
    if not values:
        return PROVED, []
    assertions = "\n".join(found.compared for found in proofs)
    missing = [value for value in values if not compares(assertions, value)]
    return (SHAPE_ONLY, missing) if missing else (PROVED, values)


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


def recorded_shape_only() -> tuple[dict[str, dict], list[str]]:
    """Rows already known to be covered without their stated value being compared.

    Not a waiver — the row keeps its test and keeps running — but a ledger, so the class `CF-12`
    found is countable rather than anecdotal, and so a *new* row cannot join it quietly. Every entry
    carries a reason, because "which value, and what does the test check instead" is the only part
    of this that a later reader cannot reconstruct.
    """
    problems: list[str] = []
    if not SCOPE.is_file():
        return {}, []
    data = tomllib.loads(SCOPE.read_text(encoding="utf-8"))
    rows: dict[str, dict] = {}
    for entry in data.get("unasserted", []):
        row = entry.get("id")
        if not row:
            problems.append("an unasserted entry has no `id`")
            continue
        if not entry.get("reason", "").strip():
            problems.append(f"{row}: recorded as shape only with no reason")
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


def render(
    rows: set[str],
    proofs: dict[str, list[Proof]],
    waived: dict[str, dict],
    verdicts: dict[str, tuple[str, list[str]]],
) -> tuple[str, set[str]]:
    """The report, and the set of rows it actually printed.

    The second element is what makes the headline auditable. A denominator computed from `rows` and
    tables emitted from `FAMILIES` are two different traversals, and `CF-17` is what happens when
    they disagree — so the caller compares them instead of trusting that they match, and the report
    states the comparison in its own words. Returning the emitted set rather than a bare count keeps
    the failure message able to name the rows.
    """
    proved = [row for row in rows if verdicts.get(row, (None,))[0] == PROVED]
    shape = [row for row in rows if verdicts.get(row, (None,))[0] == SHAPE_ONLY]

    # The sections are built first so the preamble can state what they contain. Any row this loop
    # does not reach is a row the document does not show, whatever the headline says.
    emitted: set[str] = set()
    sections: list[str] = []
    section_count = 0
    for family_key, title in FAMILIES.items():
        family = sorted(
            (row for row in rows if family_of(row) == family_key), key=sort_key
        )
        if not family:
            continue
        emitted.update(family)
        section_count += 1
        sections += [f"## {title}", "", "| Row | Status | Proved by / deferred to |", "|---|---|---|"]
        for row in family:
            if row in proofs:
                where = ", ".join(
                    f"`{path}`" for path in sorted({found.file for found in proofs[row]})
                )
                state, values = verdicts[row]
                named = ", ".join(f"`{value}`" for value in values)
                if state == PROVED:
                    note = f" — asserts {named}" if values else ""
                elif state == SHAPE_ONLY:
                    note = f" — states {named}; not compared"
                else:
                    note = " — no vector-table row to read a claim from"
                sections.append(f"| `{row}` | {state} | {where}{note} |")
            else:
                entry = waived.get(row, {})
                story = entry.get("story", "—")
                reason = " ".join(entry.get("reason", "").split())
                sections.append(f"| `{row}` | deferred | `{story}` — {reason} |")
        sections.append("")

    # `CF-17`'s acceptance in one sentence, written into the document rather than only checked: the
    # denominator above and the tables below are counted separately and compared here. When they
    # disagree the report says so and names the families it dropped, because a report that is silent
    # about what it omits is the defect, and a green gate is not the only reader this has to survive.
    missing = sorted(rows - emitted, key=sort_key)
    if missing:
        families = ", ".join(
            dict.fromkeys(
                f"`{prefix}-{letter}`" if letter else f"`{prefix}-`"
                for prefix, letter in (family_of(row) for row in missing)
            )
        )
        crosscheck = (
            f"**{len(missing)} of these rows appear in no table below** — the families "
            f"{families} have no section, so they are counted here and shown nowhere."
        )
    else:
        crosscheck = (
            f"Σ over the {section_count} sections below is {len(emitted)}, so every row counted "
            f"above is shown in exactly one table."
        )

    lines = [
        "# Conformance — every spec vector, and what proves it",
        "",
        "**Generated by `scripts/check-vectors.py`. Do not hand-edit.**",
        "",
        f"One row per vector in {sources()}.",
        "",
        "A row is *proved* when a test in the workspace covers it **and** compares every value the",
        "row states, *shape only* when a test covers it but never compares a value it states, and",
        "*deferred* when [vector-scope.toml](vector-scope.toml) says why and names the story that",
        "will.",
        "",
        # Counted, not derived. This used to read `len(rows) - len(waived)`, which is only correct
        # while *every* non-deferred row is covered — true by accident, because the gate refused any
        # row that was neither. `CF-8` registered six more specs and the assumption broke: the report
        # claimed 471 proved while 337 rows had no test at all. A headline number that cannot be
        # wrong is a headline number that is not measuring anything.
        f"**{len(proved)} of {len(rows)} rows proved**; {len(shape)} covered for shape only; "
        f"{len(waived)} deferred.",
        "",
        crosscheck,
        "",
        "## What these words mean",
        "",
        "**Proved** means: a test named for the row exists and runs, and — where the row's `Expect`",
        "column states a number — some assertion in that test compares that number. `CF-12` added",
        "the second half. Before it, `PB-F-1` read *Timer C set 180 s* and its test asserted only",
        "`[Kind::ResolveTargets, Kind::Forward, Kind::SetTimer]`, so the row and the code never met;",
        "the row was wrong about the value for the project's whole life and the report said proved",
        "throughout.",
        "",
        "**Shape only** means the row states a value and nothing in its proof compares it. The test",
        "runs and the behaviour it does check is checked; the quantity the row exists to pin is not.",
        "These rows are named in [vector-scope.toml](vector-scope.toml) with what the test asserts",
        "instead, and they do not count towards *proved*.",
        "",
        "**What proved still does not mean.** That the assertion compares the *right* thing: any",
        "assertion in the test satisfies the check, so a value used as stimulus and re-asserted",
        "elsewhere counts. That the test agrees with the row — `PB-R-5` states `486` and its test",
        "asserts `404`; the check can see a number missing, not which of the two is wrong. That a",
        "row stating no number is checked at all beyond its name: for those, *proved* is worth",
        "exactly what the test is worth. And nothing here says the spec itself is right.",
        "",
    ]
    return "\n".join(lines + sections), emitted


# `PB-F-1` and its proof exactly as they stood at `7d21a22`, the commit `PX-10` corrected. The row
# claims two numbers; the test compares neither, and the gate called it proved. Kept verbatim rather
# than paraphrased, because a detector tuned against a paraphrase of its own bug is not a detector.
WORKED_EXAMPLE_ROW = (
    "| PB-F-1 | Dialog-forming INVITE, 1 target | Forward: Via pushed (cookie present), "
    "`Max-Forwards` decremented, Record-Route with token parameter ≤ 200 B, Timer C set 180 s |"
)
WORKED_EXAMPLE_PROOF = '''fn pb_f_1_a_dialog_forming_invite_is_record_routed_with_a_branch_and_timer_c() {
    let (_, effects) = run(
        invite("sip:bob@b.example", vec![]),
        vec![target("sip:bob@10.0.0.1", 1_000)],
    );

    // Effect order is load-bearing: the Forward precedes the SetTimer that guards it.
    assert_eq!(
        effects.iter().map(Effect::kind).collect::<Vec<_>>(),
        [Kind::ResolveTargets, Kind::Forward, Kind::SetTimer]
    );

    let forwarded = effects
        .iter()
        .find_map(Effect::forwarded)
        .expect("a forward");
    let record_route = header_text(forwarded, &HeaderName::RecordRoute).expect("a Record-Route");
    assert!(record_route.contains("edge-1.example"), "{record_route}");

    let via = header_text(forwarded, &HeaderName::Via).expect("a Via");
    assert!(via.contains("branch=z9hG4bK-"), "{via}");
    assert!(
        via.contains(EDGE),
        "the Via must be answerable back to us: {via}"
    );
}'''


def self_test() -> list[str]:
    """Replay the case this check exists for, and the ways of faking it that must not work."""
    failures: list[str] = []

    def check(claim: str, held: bool) -> None:
        if not held:
            failures.append(claim)

    cells = [cell.strip() for cell in WORKED_EXAMPLE_ROW.split("|") if cell.strip()]
    values = stated(cells[-1])
    check(f"the row states 200 B and 180 s, read as {values}", values == ["200 B", "180 s"])

    lines = WORKED_EXAMPLE_PROOF.splitlines()
    asserted = compared(function_body(lines, 0))
    check(
        "the 7d21a22 proof compares neither value — this is the whole defect",
        not compares(asserted, "180 s") and not compares(asserted, "200 B"),
    )
    check(
        "it does compare the effect kinds it was written to compare",
        "Kind::SetTimer" in asserted,
    )

    # The three ways a row could be made to look proved without being proved.
    quoted = '{\n    // The row says Timer C is set 180 s.\n    assert!(timer.is_some());\n}'
    check(
        "a comment quoting the row's value is not an assertion",
        not compares(compared(function_body(quoted.splitlines(), 0)), "180 s"),
    )
    messaged = '{\n    assert_eq!(armed, expected, "the row says 180 s");\n}'
    check(
        "a value named only in a failure message is not compared",
        not compares(compared(function_body(messaged.splitlines(), 0)), "180 s"),
    )
    named = 'fn pb_f_1_timer_c_is_180_seconds() {\n    assert!(timer.is_some());\n}'
    check(
        "a value spelled only in the test's name is not compared",
        not compares(compared(function_body(named.splitlines(), 0)), "180 s"),
    )

    fixed = '{\n    assert_eq!(armed, Duration::from_secs(180));\n}'
    check(
        "a real comparison of the value passes",
        compares(compared(function_body(fixed.splitlines(), 0)), "180 s"),
    )
    grouping = '{\n    assert_eq!(expires, Some(3_600));\n}'
    check(
        "Rust's digit separator does not hide a value",
        compares(compared(function_body(grouping.splitlines(), 0)), "3600"),
    )
    milliseconds = '{\n    assert_eq!(config.timers.timer_c_ms, 240_000);\n}'
    check(
        "a value the code holds in ms answers a row that states seconds",
        compares(compared(function_body(milliseconds.splitlines(), 0)), "240 s"),
    )

    # `EX-12`'s half: the row shape is recognised even when nobody owns the prefix, which is what
    # turns "a table no gate reads" from invisible into an error.
    check(
        "a row whose prefix no spec owns is still recognised as a row",
        (found := ANY_ROW_LINE.match("| QP-Z-999 | a stimulus | an expectation |")) is not None
        and found.group(1) == "QP",
    )
    check(
        "a registered prefix is not reported as unowned",
        (found := ANY_ROW_LINE.match("| HF-9 | given | expect |")) is not None
        and found.group(1) in SPECS,
    )
    check(
        "a line that merely cites a row is not a row",
        ANY_ROW_LINE.match("| `media-anchor` | claims SDP | QP-A-1 … QP-A-4 |") is None,
    )
    return failures


def main() -> int:
    check_only = "--check" in sys.argv

    if failures := self_test():
        for failure in failures:
            print(f"self-test: {failure}")
        print(f"\nvectors: FAIL — the check cannot detect what it was built to detect")
        return 1
    if "--self-test" in sys.argv:
        print("vectors: self-test passed — the PB-F-1 case as of 7d21a22 is refused")
        return 0

    rows = spec_rows()
    if not rows:
        print("vectors: FAIL — no rows found in any spec", file=sys.stderr)
        return 1

    proofs = covered()
    waived, problems = deferred()
    shape_only, ledger_problems = recorded_shape_only()
    problems += ledger_problems
    claimed = claims()
    verdicts = {row: verdict(row, proofs[row], claimed) for row in proofs}
    problems += unowned_rows()
    problems += unrepresented_families(rows)

    for row in sorted(rows, key=sort_key):
        if row not in proofs and row not in waived:
            problems.append(f"{row}: in the spec, covered by no test, and not deferred")

    # A stale deferral is the failure mode that makes a report lie: it says "not proved yet" about
    # something that is, so nobody notices when the real gap reopens.
    for row in sorted(set(waived) & set(proofs), key=sort_key):
        problems.append(
            f"{row}: deferred in vector-scope.toml but covered by "
            f"{', '.join(sorted({found.file for found in proofs[row]}))} — remove the deferral"
        )

    for row in sorted(set(proofs) - rows, key=sort_key):
        problems.append(f"{row}: covered by a test but not a row in the spec")

    # `CF-12`. The row is covered, so the checks above are all satisfied — and the value it exists to
    # pin is still compared to nothing. Recording it says so out loud; saying nothing says "proved".
    for row in sorted(set(proofs) & rows, key=sort_key):
        state, values = verdicts[row]
        if state == SHAPE_ONLY and row not in shape_only:
            named = ", ".join(f"`{value}`" for value in values)
            problems.append(
                f"{row}: its Expect column states {named}, and no assertion in "
                f"{', '.join(sorted({found.test for found in proofs[row]}))} compares it — "
                f"assert the value, or record the row under [[unasserted]] in vector-scope.toml"
            )
        if state == UNREADABLE:
            problems.append(
                f"{row}: covered, but its spec has no vector-table row for it, so there is no "
                f"claim to check the test against — tabulate the row or drop the coverage"
            )

    # The same stale-entry rule the deferrals get, and what turns the list into a ratchet: once a
    # row's value is compared, its entry has to go, and it cannot come back without a gate failure.
    for row in sorted(shape_only, key=sort_key):
        if row not in proofs:
            problems.append(
                f"{row}: recorded as shape only but covered by no test at all — it is deferred, "
                f"not shape only"
            )
        elif verdicts[row][0] == PROVED:
            problems.append(
                f"{row}: recorded as shape only in vector-scope.toml but its proof now compares "
                f"every value it states — remove the entry"
            )

    report, emitted = render(rows, proofs, waived, verdicts)

    # The headline is checked against the tables, not merely computed beside them. `unrepresented_
    # families` already refuses the cause `CF-17` found, but this is the property that story is
    # actually about, and asserting the property as well as its known cause is what stops the class
    # reopening in a new way: any future path that drops a row from the document — a family filtered,
    # a section skipped, a traversal changed — moves this Σ and fails here even if every family still
    # has a title.
    if emitted != rows:
        unshown = sorted(rows - emitted, key=sort_key)
        named = ", ".join(unshown[:10]) + (
            f", and {len(unshown) - 10} more" if len(unshown) > 10 else ""
        )
        problems.append(
            f"the report counts {len(rows)} rows and its tables show {len(emitted)}: "
            f"{len(unshown)} counted and not shown ({named}) — a denominator that counts what "
            f"the document does not show is the bug"
        )

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
        f"vectors: {len([row for row in rows if verdicts.get(row, (None,))[0] == PROVED])}/"
        f"{len(rows)} rows proved, "
        f"{len([row for row in rows if verdicts.get(row, (None,))[0] == SHAPE_ONLY])} "
        f"covered for shape only, {len(waived)} deferred with a reason"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
