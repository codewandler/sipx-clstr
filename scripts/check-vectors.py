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
7. A deferral names a story that nobody will act on — and `CF-24` is the measurement of what that
   costs. 239 of 428 deferred rows (56%) named a story that had already closed, because this script
   read a deferral's `story` field in order to *print* it and never once read that story's `status`.
   Two rules, both mechanical, applied to `[[deferred]]` and to `[[unasserted]]` alike:

   - the named story exists and is not `done`. `blocked` and `backlog` are live: a story waiting on
     something still has somebody to unblock it, where a closed one has nobody at all.
   - the named story is **not the story that wrote the spec** the row lives in — `SPECS` carries
     that story per prefix. This is the mechanism that produced 188 of the 239: a spec story
     registers its own vector rows and defers them to itself, so the moment the spec is written and
     the story closes, every row it registered is orphaned. A deferral names the story that will
     *implement* the row; the one that tabulated it is done by construction.

   `CX-6` had already put this rule on the specs — a `docs/specs/` paragraph may not defer to a
   story, because a story can stop existing and a spec outlives it. This is the same rule in the
   file where the ledger actually lives.

8. A file under `docs/specs/` that `SPECS` never registered and `EXCLUDED` never named. `CF-25` is
   the measurement: `owner-rpc.md` landed 296 normative lines and a thirteen-row scenario table,
   and `--check` stayed green at 154/583 with none of it in view — not because anyone decided
   that, but because enumeration was nobody's job. It now runs in both directions: an
   unregistered, unexcluded spec is refused by name; a stale exclusion — document gone, spec also
   registered, reason empty, story closed — is refused the way a stale deferral is; a registered
   prefix whose owner tabulates no row of it is refused, so registration cannot be nominal; and
   the unowned-row guard reads past the `XX-N` shape — a rule table keyed by a backticked
   test-function name is a set of named executable claims, admitted only in a document `EXCLUDED`
   names.

9. A story file whose frontmatter `status` is not one of the five lifecycle values, or whose `id`
   is not a story id. Rules 7 and 8 both turn on a story's status, and `CLOSED_STATUSES` was an
   exact-match denylist read over free text: `don` is not `done`, so a **mistyped** status counted
   as a live owner — the failure direction was "accept", in the two rules whose whole purpose is
   refusing dead letters. `CF-27` closes that by validating at ingestion instead: the five values
   are a closed set, a file outside it is refused by name, and `CLOSED_STATUSES` is then a
   partition of something known rather than a denylist over anything. The same ingestion is where
   `docs/stories/_TEMPLATE.md` stopped registering `{{ID}}` — an unfilled id beside a perfectly
   valid `status: backlog`, which made the template a live story any deferral could name, and
   makes every unfilled copy of it one too.

Also writes `docs/reference/conformance.md`: generated, never hand-edited, and checked in so a
reader who does not run the suite can still see what is proved. `--check` fails if the committed file
is out of date, the way the story board is checked.

`--self-test` puts this script's own judgement to pinned cases, and `main` runs it on every
invocation before reading anything: the check that a detector still detects is worth more than the
detector. It replays the `PB-F-1` case as it stood at `7d21a22` — the spec row and the test body of
the day, verbatim — with the ways of faking a proof that must not buy one; the vector-row and
named-scenario shapes, in both directions, since a guard that matches everything is no guard;
deferral and shape-only liveness against a fixed status table; every way an `EXCLUDED` entry can
rot; a rowless registration; and story-status ingestion, `{{ID}}` included. The success line counts
the cases, so one deleted reads as a smaller number rather than as silence.

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
STORIES = ROOT / "docs" / "stories"

# Every spec that carries a vector table, with the prefix its rows use, the section the table
# lives in, and the story that *wrote* those rows — the section is quoted in the generated report,
# so it cannot drift from this table, and the story is the one a deferral may never name (`CF-24`).
#
# The third field is per *prefix* rather than per spec, because two specs carry two tables each and
# the second table was written later by a different story: `FR` is affinity-token §14 (`AF-2`) beside
# `AT`'s §10 (`AF-1`), and `QP` is hook-framework §9.1 (`EX-12`, which moved the rows in from a
# design record) beside `HF`'s §9 (`EX-1`). A per-spec answer would name the wrong story for both.
# `spec_authors` checks every value here against the spec's own `**Stories:**` header, so a
# hand-written column cannot quietly disagree with the document it describes.
#
# A prefix is owned by exactly one spec, and `spec_rows` reads rows only from the owner — so a design
# doc citing `PB-F-1`, or this script's own docstring, cannot invent a row somebody has to prove. The
# convention that makes this safe: a spec's *rules* are named bare (`V1`, `D3`) and only its *vectors*
# carry the prefix (`CC-V-1`). A rule citation like `CC-V2` has no second hyphen and does not match.
#
# Six of these were unregistered until `CF-8`, which meant ~394 normative rows were prose nothing
# executed. Demonstrated rather than inferred: a fabricated `LS-R-999` passed the gate untouched.
SPECS = {
    "PB": (ROOT / "docs" / "specs" / "proxy-behavior.md", "§12", "PX-1"),
    "EP": (ROOT / "docs" / "specs" / "e2e-probe.md", "§10", "ET-1"),
    # `registrar-auth` states its authorship as `Status: accepted (RG-2)` rather than in a
    # `**Stories:**` field, so `spec_authors` has nothing to cross-check this value against. Same for
    # `operational-capability-baseline`, whose header says `**Tracker:** CX-8`.
    "RA": (ROOT / "docs" / "specs" / "registrar-auth.md", "§8", "RG-2"),
    "HF": (ROOT / "docs" / "specs" / "hook-framework.md", "§9", "EX-1"),
    "LS": (ROOT / "docs" / "specs" / "location-service.md", "§9", "RG-1"),
    "MR": (ROOT / "docs" / "specs" / "media-relay.md", "§12", "ME-1"),
    "NN": (ROOT / "docs" / "specs" / "number-normalisation.md", "§11", "RT-6"),
    "AT": (ROOT / "docs" / "specs" / "affinity-token.md", "§10", "AF-1"),
    "FR": (ROOT / "docs" / "specs" / "affinity-token.md", "§10", "AF-2"),
    "CC": (ROOT / "docs" / "specs" / "cluster-config.md", "§12", "DP-1"),
    "AI": (ROOT / "docs" / "specs" / "asserted-identity.md", "§13", "RT-7"),
    # `QP` is `hook-framework`'s second table, and the second prefix that spec owns. Registered by
    # `EX-12`, which had to move the rows out of `docs/designs/extension-framework.md` first: a
    # design record is not normative, `spec_rows` reads only the owner, and a prefix aimed at
    # `docs/designs/` would have made a design normative by accident. Same demonstration as `CF-8`'s
    # — a fabricated `QP-Z-999` row passed `--check` until the rows had a spec to live in.
    "QP": (ROOT / "docs" / "specs" / "hook-framework.md", "§9.1", "EX-12"),
    # `SC` is the `SipxCluster` resource's admission and status rows, registered by `KO-1` in the same
    # commit that writes them — the lesson `EX-12` paid for is that registration is not something to
    # do afterwards.
    "SC": (ROOT / "docs" / "specs" / "sipx-cluster-crd.md", "§10", "KO-1"),
    # M4 target specifications are registered when written. Their rows remain explicitly deferred
    # in vector-scope.toml until the implementation and milestone-proof stories land — deferred to
    # those stories, never to the story in this column, which is what wrote the rows down.
    "SS": (ROOT / "docs" / "specs" / "session-service.md", "§6", "BS-1"),
    "OB": (ROOT / "docs" / "specs" / "operational-capability-baseline.md", "§5", "CX-8"),
}

# Documents whose normative claims deliberately live outside the vector ledger — each one a named
# entry, because the alternative is the shape `CF-25` was filed on: `owner-rpc.md` landed 296
# normative lines and a §10 scenario table, and `--check` stayed green without reading one rule of
# it. Choosing not to register a prefix is legitimate — a row and the test that executes it should
# arrive in one commit, which is `cluster-membership` §11's own reasoning — but the *choice* has to
# leave a trace a tool reads, and this table is that trace. Every file under `docs/specs/` is
# therefore in exactly one place: registered in `SPECS`, or named here with a reason
# (`unenumerated_specs` refuses both a file in neither and an entry gone stale).
#
# `story` is the story that will execute the document's claims, held to the deferral ledger's
# liveness rule (`CF-24`): it exists and is not `done`. Empty means the claims are already carried
# by a registered prefix's own rows, so the ledger rather than a story is the owner.
EXCLUDED: dict[str, dict[str, str]] = {
    # §11 maps every rule a test executes today onto `cluster-config`'s `CC` rows, each deferred
    # in vector-scope.toml with a live owner; the fields with no rows yet get them in the same
    # commit as the loader code that implements them.
    "docs/specs/cluster-membership.md": {
        "story": "",
        "reason": "registers no prefix by design (§11): its rules execute through "
        "cluster-config's `CC` rows, and the fields without rows yet get them beside their "
        "loader code",
    },
    # §10 names thirteen harness scenarios instead of rows: its rules are the behaviour of two
    # nodes, a channel and a connection table under faults, and what executes that is a scenario.
    "docs/specs/owner-rpc.md": {
        "story": "AF-7",
        "reason": "registers no prefix by design (§10): its rules are the thirteen named harness "
        "scenarios that arrive with the implementation",
    },
    # The `external_route_*` scenario table under *EX-6: the async external routing hook* —
    # specified at design grain, and EX-6's own handoff records that nothing executes them until
    # the hook runtime exists.
    "docs/designs/extension-framework.md": {
        "story": "EX-3",
        "reason": "carries the eleven `external_route_*` scenarios at design grain; nothing "
        "executes them until the hook runtime exists",
    },
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
# The rule-table shape `ANY_ROW_LINE` cannot see (`CF-25`): a row keyed by a backticked
# test-function name rather than a row ID — `owner-rpc` §10's shape, thirteen named scenarios
# invisible to the guard above because their first cell is not `XX-N`. Such a row is a named
# executable claim exactly the way a vector row is, so a document carrying one must be named in
# `EXCLUDED` with the story that will execute it. Three-plus snake_case words is where the corpus
# separates: every schema field a spec tabulates is one or two (`call_id`, `last_activity`), and
# a scenario name is a sentence. A three-word *value* name rides on its document's entry where one
# exists (`max_pending_per_flow` in `owner-rpc` §8); anywhere else it is an error whose message
# carries both fixes, because the cheap fix for a false positive is keying the table differently
# and the cheap fix for a real scenario table is a registry line — both deliberate, both visible.
#
# Supporting optional call parentheses is `CF-27`, found reviewing `CF-25`, which documented the
# two-word evasion and left this one. A test-function name is as naturally written `` `delivers()` ``
# as `` `delivers` ``, and requiring the closing backtick to follow the name meant one keystroke of
# house style put a whole scenario table back out of view. The captured group stays the bare name,
# so the error message names the scenario rather than a call. The argument matcher refuses both a
# closing parenthesis and a backtick, so it cannot run past its own cell.
SCENARIO_LINE = re.compile(r"^\|\s*`([a-z][a-z0-9]*(?:_[a-z0-9]+){2,})(?:\([^)`]*\))?`\s*\|")
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

# A story's YAML frontmatter, and the fields this check reads out of it. `note:` and `title:` hold
# prose but are harmlessly captured too: `id` and `status` are the only two fields inspected below.
# Capture the whole value, including an empty one and an inline comment. Requiring `\S+` here made
# malformed YAML-shaped values disappear from `fields`, so ingestion silently treated the file as
# having no status at all — exactly the fail-open direction this boundary exists to prevent.
FRONTMATTER = re.compile(r"\A---\n(.*?)\n---", re.S)
FRONTMATTER_FIELD = re.compile(r"^([a-z]+):[ \t]*(.*)$", re.M)
# The closed schema needs only scalar ids and lifecycle names. Accept the ordinary YAML spellings
# authors use for those — plain, single quoted, or double quoted, with an optional inline comment —
# and reject every structured or empty form. Supporting arbitrary YAML here would add a dependency
# to a gate for no semantic gain: no mapping, sequence, tag, anchor, or escape can be a lifecycle
# value.
PLAIN_FRONTMATTER_SCALAR = re.compile(
    r"\A([A-Za-z0-9][A-Za-z0-9-]*)(?:[ \t]+#.*)?\Z"
)
SINGLE_QUOTED_FRONTMATTER_SCALAR = re.compile(r"\A'([^'\r\n]*)'(?:[ \t]+#.*)?\Z")
DOUBLE_QUOTED_FRONTMATTER_SCALAR = re.compile(r'\A"([^"\\\r\n]*)"(?:[ \t]+#.*)?\Z')
# The `**Stories:**` field of a spec header, to the end of that header paragraph. Used only to
# cross-check `SPECS`' third column, so reading generously is the safe direction.
SPEC_STORIES = re.compile(r"\*\*Stories:\*\*(.*?)(?:\n\s*\n|\Z)", re.S)
STORY_ID = re.compile(r"\b([A-Z]{2}-\d+)\b")
# `STORY_ID` finds a story id inside prose; this one is the whole `id:` field and nothing else.
# `docs/stories/_TEMPLATE.md` is why it is anchored: `id: {{ID}}` beside a valid `status: backlog`
# registered the template as a live `backlog` story that any deferral could name, and a copy of it
# with the id still unfilled — which is how the board says to start one — is the same phantom under
# a name this script has no list of.
STORY_FILE_ID = re.compile(r"\A[A-Z]{2}-\d+\Z")
# The files in `docs/stories/` that are not stories: the generated board, and the template every
# story is copied from. `check-docs.py::check_stories` draws the boundary in the same two names, so
# the two gates agree on what a story file is.
NOT_A_STORY = ("README.md", "_TEMPLATE.md")
# The story lifecycle, closed — the five values the board is generated from and the story frontmatter
# schema names. Validating against this at ingestion is `CF-27`, and what it buys is the direction of
# the failure: `CLOSED_STATUSES` below is an exact-match set, so before this existed a story whose
# status read `don` was not `done`, and therefore counted as a **live** owner in both ledger rules.
# A denylist over free text can only fail open. Held to a closed vocabulary, it is a partition of
# something known, and the self-test pins that relationship rather than trusting the two lists to
# stay aligned. Kept in lifecycle order so the error teaches the same progression the board does.
LIFECYCLE_STATUSES = ("backlog", "ready", "in-progress", "blocked", "done")
# The statuses that mean nobody is going to do this. `blocked` is deliberately not one of them: a
# blocked story has somebody who can unblock it, where a `done` story has nobody at all.
CLOSED_STATUSES = frozenset({"done"})

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
    # `PX-14`. The family exists because neither of its neighbours owns the rule: it is §8's and §9's
    # terminal results crossed with §7.1's queue, and being nobody's is exactly how a `487` came to
    # re-originate an answered call with every row above it green.
    ("PB", "T"): "Proxy — a terminal result and the queued target set (§7.1, §8, §9)",
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
    ("LS", "A"): "Location service — REGISTER admission (§5.1)",
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
    ("SC", "A"): "SipxCluster — admission, cluster-scope (§10)",
    ("SC", "S"): "SipxCluster — observed status (§10)",
    ("SS", ""): "Optional session service — call and conference behavior (§6)",
    ("OB", ""): "Operational capability baseline — release proof (§5)",
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
    for prefix, (path, _section, _author) in SPECS.items():
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
    """Table rows shaped like executable claims that no registry reads.

    Two shapes. A row keyed `XX-N` whose prefix `SPECS` does not own: `spec_rows` reads only the
    owner of a prefix, which is what stops a design doc inventing a row, and the cost of that is
    silence in the other direction — `EX-12` found a table of `QP-*` rows in
    `docs/designs/extension-framework.md` that had sat through two stories, looking exactly like a
    vector table, proving nothing, and passing `--check`, a fabricated row among them passing too.
    And a row keyed by a backticked test-function name (`CF-25`): the same kind of claim with no
    prefix at all, which is how `owner-rpc.md` §10 named thirteen scenarios nothing executed while
    the gate stayed green. A named-scenario row is admitted only in a document `EXCLUDED` names,
    because that entry is the trace — the reason, and the live story — that the claims deliberately
    await their implementation. Registration is therefore not something to remember; an unowned row
    is an error with the fix in the message.
    """
    problems: list[str] = []
    for tree in TABLE_TREES:
        for path in sorted((ROOT / tree).rglob("*.md")):
            rel = str(path.relative_to(ROOT))
            for number, line in enumerate(
                path.read_text(encoding="utf-8").splitlines(), start=1
            ):
                found = ANY_ROW_LINE.match(line)
                if found and found.group(1) not in SPECS:
                    problems.append(
                        f"{rel}:{number}: a table row with the shape of a "
                        f"vector, but no spec owns the prefix `{found.group(1)}` — register it in "
                        f"SPECS against the spec that states it, or stop writing it as a row"
                    )
                    continue
                scenario = SCENARIO_LINE.match(line)
                if scenario and rel not in EXCLUDED:
                    problems.append(
                        f"{rel}:{number}: a rule table keyed by `{scenario.group(1)}` — a named "
                        f"scenario no registered prefix carries — in a document EXCLUDED does not "
                        f"name; name the document there with a reason and the story that will "
                        f"execute it, or key the table by registered vector rows"
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


def unenumerated_specs(
    existing: set[str],
    registered: set[str],
    excluded: dict[str, dict],
    statuses: dict[str, str],
) -> list[str]:
    """Spec files the coverage picture never heard of, and stale entries in the ledger of them.

    `CF-25`: `SPECS` grew by hand, so a new spec was in the coverage picture only if its author
    remembered — `owner-rpc.md` carried 296 normative lines and a thirteen-row scenario table, and
    `--check` passed without reading one rule of it. So the direction reverses: every file under
    `docs/specs/` is either registered in `SPECS` or named in `EXCLUDED` with a reason, and an
    absence is an error naming the file. The entries themselves get the ratchet rules every other
    ledger here has — an entry whose document is gone, whose spec is also registered, whose reason
    is empty, or whose story is closed is refused, because a trace that can rot silently is an
    absence with better manners (`CF-24`).

    The two rules have different reach, and `CF-27` is the correction. *Enumerate or exclude* is a
    duty `docs/specs/` alone carries: a design record is not normative, so one registering nothing
    is the normal case rather than an omission. *Registered and excluded* is a contradiction
    wherever it occurs — the registries name two trees (`TABLE_TREES`), and `EXCLUDED` already
    names a design among them, so an excluded design that later gained a registration would have
    left the gate believing both readings and saying neither.
    """
    problems: list[str] = []
    # Registry disagreement is a property of the registries, not of the normative-spec tree. Keep
    # it separate from the enumeration below so a design is covered too, and so a stale path named
    # in both places reports the contradiction as well as having gone stale.
    for path in sorted(registered & set(excluded)):
        problems.append(
            f"{path}: registered in SPECS and named in EXCLUDED — the two claims contradict, "
            f"and a reader cannot tell which one the gate believed; remove one"
        )
    for path in sorted(existing):
        if (
            path.startswith("docs/specs/")
            and path not in registered
            and path not in excluded
        ):
            problems.append(
                f"{path}: a spec registered nowhere — its rules exist outside the coverage "
                f"picture that claims to describe them; register a prefix in SPECS, or name it "
                f"in EXCLUDED with the reason it carries no vector table"
            )
    for path in sorted(excluded):
        entry = excluded[path]
        if path not in existing:
            problems.append(
                f"{path}: named in EXCLUDED and no such document exists — remove the entry"
            )
        if not entry.get("reason", "").strip():
            problems.append(
                f"{path}: named in EXCLUDED with no reason — an exclusion is a named reason, "
                f"not a waiver"
            )
        story = entry.get("story", "").strip()
        if not story:
            continue
        if story not in statuses:
            problems.append(
                f"{path}: EXCLUDED names `{story}`, which is no story in docs/stories/ — name "
                f"the story that will execute the document's claims, or the trace is addressed "
                f"to nobody"
            )
        elif statuses[story] in CLOSED_STATUSES:
            problems.append(
                f"{path}: EXCLUDED names `{story}`, which is `status: {statuses[story]}` — that "
                f"story will not execute these claims; re-point the entry at the story that will"
            )
    return problems


def rowless_registrations(rows: set[str], specs: dict) -> list[str]:
    """Prefixes registered in `SPECS` whose owner tabulates no row of them.

    The loophole `unenumerated_specs` would otherwise open (`CF-25`): registration silences the
    enumeration, and a registration is one line — so a prefix with no rows behind it would put a
    spec inside the coverage picture while adding nothing to the denominator, which reads as
    covered in exactly the direction nobody checks. `CF-21` owns holding the published copies of
    the count to its generator; this is the generator refusing to produce a count a nominal entry
    is hiding inside.
    """
    tabulated = {family_of(row)[0] for row in rows}
    problems: list[str] = []
    for prefix in specs:
        if prefix not in tabulated:
            problems.append(
                f"{prefix}: registered against {specs[prefix][0].relative_to(ROOT)} and that "
                f"document tabulates no `{prefix}` row — a rowless registration reads as covered "
                f"while gating nothing; tabulate the vectors, or move the spec to EXCLUDED with "
                f"a reason"
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
    for prefix, (path, _section, _author) in SPECS.items():
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


def frontmatter_scalar(raw: str) -> str | None:
    """A scalar spelling admitted by the story id/status schema, or `None` when structured."""
    value = raw.strip()
    for pattern in (
        PLAIN_FRONTMATTER_SCALAR,
        SINGLE_QUOTED_FRONTMATTER_SCALAR,
        DOUBLE_QUOTED_FRONTMATTER_SCALAR,
    ):
        matched = pattern.match(value)
        if matched:
            return matched.group(1)
    return None


def ingest_story(name: str, text: str) -> tuple[tuple[str, str] | None, list[str]]:
    """One story file's `(id, status)`, and the reasons it is not a story to hold anyone to.

    `CF-27`. Both ledger rules ask "is the story that owns this row still live", and the answer is
    only as good as what got ingested — so the judgement is here, at the one place a status enters
    this script, rather than at the two places it is read.

    Three outcomes. The board and the template are not stories and register nothing, silently.
    A file whose `id` is not a story id registers nothing and *is* a problem: `{{ID}}` is what an
    unfilled copy of `docs/stories/_TEMPLATE.md` carries, and letting it in makes a live `backlog`
    story out of a placeholder that every deferral can then name. Anything else registers — and if
    its status is outside `LIFECYCLE_STATUSES`, registers **and** reports.

    Registering a bad status rather than dropping it is deliberate. The run is red either way, and
    the reader's next move differs: dropped, a deferral naming the story reads "no story in
    docs/stories/", which is false and sends them looking for a missing file instead of at the
    typo one line above.
    """
    if name in NOT_A_STORY:
        return None, []
    block = FRONTMATTER.match(text)
    if not block:
        return None, []
    fields = dict(FRONTMATTER_FIELD.findall(block.group(1)))
    if "id" not in fields or "status" not in fields:
        return None, []
    raw_story, raw_status = fields["id"], fields["status"]
    story = frontmatter_scalar(raw_story)
    status = frontmatter_scalar(raw_status)
    if story is None or not STORY_FILE_ID.match(story):
        shown_story = story if story is not None else raw_story.strip() or "<empty>"
        return None, [
            f"{name}: frontmatter `id: {shown_story}` is not a story id — an unfilled template copy "
            f"registers as a live story that a deferral can name and nobody owns; fill the id in, "
            f"or the file is not a story"
        ]
    if status is None or status not in LIFECYCLE_STATUSES:
        shown_status = status if status is not None else raw_status.strip() or "<empty>"
        return (story, shown_status), [
            f"{name}: `status: {shown_status}` is not one of "
            f"{' | '.join(LIFECYCLE_STATUSES)} — the deferral and exclusion ledgers ask "
            f"whether this story is still live, and a status outside the lifecycle answers that "
            f"question by accident; correct the value"
        ]
    return (story, status), []


def story_statuses() -> tuple[dict[str, str], list[str]]:
    """Story id → its frontmatter `status`, and every invalid id or status ingestion found.

    Read from `docs/stories/*.md`, not from the generated board: the board is a rendering of exactly
    this field, and reading the rendering would make this gate depend on somebody having remembered
    to run `/track:board`.

    `glob`, never `rglob` — agent worktrees under `.claude/` are full checkouts of this repository,
    so a walk finds a second copy of every story and a sibling's in-flight edit would decide this
    gate. That is the lesson `check-docs.py::markdown_files` paid for, and it applies here for the
    same reason.
    """
    statuses: dict[str, str] = {}
    problems: list[str] = []
    for path in sorted(STORIES.glob("*.md")):
        found, reasons = ingest_story(path.name, path.read_text(encoding="utf-8"))
        problems += reasons
        if found:
            statuses[found[0]] = found[1]
    return statuses, problems


def spec_authors() -> tuple[dict[str, str], list[str]]:
    """Prefix → the story that wrote its rows, cross-checked against the spec's own header.

    `SPECS`' third column is hand-written, and the rule it feeds — a deferral may not name the story
    that wrote the spec — is only as good as that column. So every value is compared against the
    `**Stories:**` field of the spec it belongs to: the declared author must be one of the stories
    the document itself associates with. That is a membership test rather than an equality one on
    purpose, because two specs carry two tables written by two stories (see `SPECS`), and the header
    lists all of them without saying which table is whose.

    A spec with no `**Stories:**` field is skipped rather than reported: `registrar-auth` and
    `operational-capability-baseline` state authorship in other words, and inventing a header for
    them belongs in a story about spec headers, not in this check.
    """
    authors: dict[str, str] = {}
    problems: list[str] = []
    for prefix, (path, _section, author) in SPECS.items():
        authors[prefix] = author
        if not path.is_file():
            continue
        field = SPEC_STORIES.search(path.read_text(encoding="utf-8"))
        if not field:
            continue
        named = STORY_ID.findall(field.group(1))
        if named and author not in named:
            problems.append(
                f"{prefix}: SPECS names `{author}` as the story that wrote these rows, and "
                f"{path.relative_to(ROOT)} does not list it among {', '.join(named)} — one of the "
                f"two is wrong, and a deferral rule reading the wrong story protects nothing"
            )
    return authors, problems


def dead_letters(
    entries: dict[str, dict], table: str, statuses: dict[str, str], authors: dict[str, str]
) -> list[str]:
    """Entries whose `story` is a reason nobody will act on.

    `CF-24`: this file's `story` field was read in order to print it and never to check it, and 239
    of 428 deferred rows — 56% — named a story that had already closed. One problem per row, in spec
    order, and the first reason that applies wins: a row is one dead letter, not a list of ways of
    being one.

    An entry naming no story at all is not this function's business — `deferred` already refuses that
    for `[[deferred]]`, and an `[[unasserted]]` entry is allowed to name none, because what it
    records is what a test asserts *instead* rather than work somebody owes.
    """
    held = "deferred to" if table == "deferred" else "recorded as shape only against"
    problems: list[str] = []
    for row in sorted(entries, key=sort_key):
        story = entries[row].get("story", "").strip()
        if not story:
            continue
        if story not in statuses:
            problems.append(
                f"{row}: {held} `{story}`, which is no story in docs/stories/ — name a story that "
                f"exists, or the reason is addressed to nobody"
            )
            continue
        if statuses[story] in CLOSED_STATUSES:
            problems.append(
                f"{row}: {held} `{story}`, which is `status: {statuses[story]}` — that story will "
                f"not do it; re-point the row at the story that will"
            )
            continue
        prefix = family_of(row)[0]
        if story == authors.get(prefix):
            spec = SPECS[prefix][0].relative_to(ROOT) if prefix in SPECS else "its spec"
            problems.append(
                f"{row}: {held} `{story}`, the story that wrote {spec} — a spec story is done once "
                f"the spec is written, so its own rows are orphaned the day it closes; name the "
                f"story that will implement the row"
            )
    return problems


def sources() -> str:
    """The vector tables this report covers, named from `SPECS` so the sentence cannot go stale."""
    parts = [
        f"[{path.stem}](../specs/{path.name}) {section}"
        for path, section, _author in SPECS.values()
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

    # `CF-25`. The denominator above counts rows; this sentence accounts for *documents*, so a
    # spec outside the ledger is visible as a decision rather than invisible as an omission. The
    # counts are computed from the registries the gate enforces, never restated by hand — `CF-21`
    # owns holding any copy of them elsewhere to this document.
    registered_files = {path for path, _section, _author in SPECS.values()}
    excluded_specs = [path for path in EXCLUDED if path.startswith("docs/specs/")]
    enumeration = (
        f"Every file under `docs/specs/` is enumerated: {len(registered_files)} documents "
        f"registered across {len(SPECS)} prefixes, and {len(excluded_specs)} outside the vector "
        f"ledger — named in *Outside the vector ledger* below, beside every other document that "
        f"carries named-but-unexecuted scenarios. A spec in neither place, a registered prefix "
        f"that tabulates no rows, and a stale exclusion are each a gate failure, so a spec "
        f"cannot read as covered by being nominal."
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
        "*deferred* when [vector-scope.toml](vector-scope.toml) says why and names a **live** story",
        "that will — one that exists and is not `done`, and is not the story that wrote the spec.",
        "Both halves are enforced on every run (`CF-24`): before they were, 239 of 428 deferred rows",
        "named a story that had already closed, so the deferred count below read as \"waiting on",
        "somebody\" while 56% of its reasons were addressed to nobody.",
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
        enumeration,
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

    # The other half of the inventory (`CF-25`): what the count above deliberately does not
    # include, named with the story that will change that. Emitted from `EXCLUDED` so it cannot
    # drift from the registry the gate enforces.
    if EXCLUDED:
        lines += [
            "## Outside the vector ledger",
            "",
            "Documents whose normative claims deliberately register no vector prefix — named",
            "scenarios that arrive with their implementation, or rules that execute through",
            "another spec's rows. Each is a named entry in `scripts/check-vectors.py`, because the",
            "alternative was measured: `owner-rpc.md` carried 296 normative lines and a",
            "thirteen-row scenario table through a green gate that had never read it. An entry's",
            "story is held live the way a deferral's is; `—` means the claims are already carried",
            "by a registered prefix's own rows.",
            "",
            "| Document | Executed by | Why it registers no prefix |",
            "|---|---|---|",
        ]
        for path in sorted(EXCLUDED):
            entry = EXCLUDED[path]
            link = f"[{pathlib.PurePosixPath(path).stem}]({path.replace('docs/', '../', 1)})"
            story = f"`{entry['story']}`" if entry.get("story", "").strip() else "—"
            lines.append(f"| {link} | {story} | {entry.get('reason', '')} |")
        lines.append("")

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

# `docs/stories/_TEMPLATE.md`, quoted whole (`CF-27`). Whole, because the phantom is
# made of two fields that only look wrong together: `id: {{ID}}` is obviously a placeholder, and
# `status: backlog` is a perfectly ordinary live status — a case trimmed down to the id would pass
# for the wrong reason, since a block with no `status` line registers nothing anyway and would
# prove the detector nothing. Every field the file carries is kept for the same reason: this is the
# text a new story is copied from, and the case is worth exactly as much as it is faithful.
STORY_TEMPLATE = """---
id: {{ID}}
title: {{TITLE}}
pillar: {{PILLAR}}
status: backlog
priority:
design:
epic:
areas:
note:
---

# {{TITLE}}

## Goal
One or two sentences: the outcome this delivers and which value/pillar it serves.

## Acceptance
- [ ] A testable criterion. A behavioral change names the failing-first test that proves it.
- [ ] …

## Progress
- (running log / checklist — a resuming agent reads this to know exactly where things stand)

## Notes
- Links, blockers, design pointers, relevant files.
"""


def story_fixture(status: str) -> str:
    """A filled copy of the real story template, with only `status` varied."""
    return (
        STORY_TEMPLATE.replace("{{ID}}", "CF-999")
        .replace("{{TITLE}}", "Story-ingestion self-test")
        .replace("{{PILLAR}}", "Core")
        .replace("status: backlog", f"status: {status}")
    )


def self_test() -> tuple[list[str], int]:
    """Replay the case this check exists for, and the ways of faking it that must not work.

    Returns the failed claims and how many were put — the count is printed on success, so deleting
    a case is a number that moved rather than a silence nobody can see.
    """
    failures: list[str] = []
    cases = 0

    def check(claim: str, held: bool) -> None:
        nonlocal cases
        cases += 1
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

    # `CF-24`'s half, replayed on the four stories the sweep found. The ledger is real by the time
    # this runs, so the cases are put to `dead_letters` directly: what has to keep working is the
    # judgement, not this file's current contents.
    statuses = {
        "RT-7": "done",
        "ME-1": "done",
        "DP-8": "done",
        "RT-2": "backlog",
        "FC-1": "blocked",
        "BS-1": "ready",
    }
    authors = {"AI": "RT-7", "MR": "ME-1", "CC": "DP-1", "SS": "BS-1"}

    def letters(entries: dict[str, dict], table: str = "deferred") -> list[str]:
        return dead_letters(entries, table, statuses, authors)

    check(
        "a deferral naming a closed story is refused",
        len(letters({"CC-D-1": {"story": "DP-8", "reason": "x"}})) == 1,
    )
    # `BS-1` is `ready`, so only the authorship rule can catch this one — which is the half that
    # matters. 188 of `CF-24`'s 239 rows were produced by a spec story deferring its own rows to
    # itself, and the day it did that it was still open and the ledger still looked correct.
    spec_story = letters({"SS-1": {"story": "BS-1", "reason": "x"}})
    check(
        "a deferral naming the story that wrote the spec is refused while that story is still open",
        len(spec_story) == 1 and "wrote" in spec_story[0],
    )
    check(
        "a deferral naming a live implementing story passes",
        letters({"AI-A-1": {"story": "RT-2", "reason": "x"}}) == [],
    )
    check(
        "a `blocked` story is a live owner",
        letters({"CC-D-1": {"story": "FC-1", "reason": "x"}}) == [],
    )
    check(
        "a deferral naming a story that does not exist is refused",
        len(letters({"CC-D-1": {"story": "ZZ-99", "reason": "x"}})) == 1,
    )
    check(
        "a closed owner is reported once, not once per rule it breaks",
        len(letters({"MR-E-1": {"story": "ME-1", "reason": "x"}})) == 1,
    )
    check(
        "an [[unasserted]] entry naming a closed story is refused too",
        len(letters({"CC-V-9": {"story": "DP-8", "reason": "x"}}, "unasserted")) == 1,
    )
    check(
        "an [[unasserted]] entry naming no story is not a dead letter",
        letters({"CC-V-9": {"reason": "x"}}, "unasserted") == [],
    )

    # `CF-25`'s half, replayed as the tree stood when it was filed: `docs/specs/owner-rpc.md`
    # registered nowhere, excluded nowhere, and `--check` green at 154/583 with none of its 296
    # normative lines in view. The enumeration must refuse that tree by name — and every way an
    # exclusion entry can rot must be refused too, or the trace is an absence with better manners.
    tree = {
        "docs/specs/owner-rpc.md",
        "docs/specs/proxy-behavior.md",
        "docs/designs/extension-framework.md",
    }
    on_record = {"docs/specs/proxy-behavior.md"}
    spec_statuses = {"AF-7": "backlog", "AF-3": "done"}

    unregistered = unenumerated_specs(tree, on_record, {}, spec_statuses)
    check(
        "a spec registered nowhere is refused, and the message names the file",
        len(unregistered) == 1 and "docs/specs/owner-rpc.md" in unregistered[0],
    )
    entry = {"docs/specs/owner-rpc.md": {"story": "AF-7", "reason": "x"}}
    check(
        "a named exclusion with a reason and a live story is the trace that passes",
        unenumerated_specs(tree, on_record, entry, spec_statuses) == [],
    )
    check(
        "an exclusion with no reason is an absence with a name, and refused",
        len(
            unenumerated_specs(
                tree,
                on_record,
                {"docs/specs/owner-rpc.md": {"story": "AF-7", "reason": " "}},
                spec_statuses,
            )
        )
        == 1,
    )
    check(
        "an exclusion of a registered spec is a contradiction, and refused",
        len(
            unenumerated_specs(
                tree, on_record | {"docs/specs/owner-rpc.md"}, entry, spec_statuses
            )
        )
        == 1,
    )
    check(
        "an exclusion whose document is gone is stale, and refused",
        len(
            unenumerated_specs(
                tree - {"docs/specs/owner-rpc.md"}, on_record, entry, spec_statuses
            )
        )
        == 1,
    )
    check(
        "an exclusion naming a closed story is a dead letter, and refused",
        len(
            unenumerated_specs(
                tree,
                on_record,
                {"docs/specs/owner-rpc.md": {"story": "AF-3", "reason": "x"}},
                spec_statuses,
            )
        )
        == 1,
    )
    check(
        "an exclusion naming a story that does not exist is refused",
        len(
            unenumerated_specs(
                tree,
                on_record,
                {"docs/specs/owner-rpc.md": {"story": "ZZ-9", "reason": "x"}},
                spec_statuses,
            )
        )
        == 1,
    )
    check(
        "an exclusion naming no story passes — the claims are carried by another prefix's rows",
        unenumerated_specs(
            tree,
            on_record,
            {"docs/specs/owner-rpc.md": {"story": "", "reason": "x"}},
            spec_statuses,
        )
        == [],
    )

    # `CF-27`: the two rules have different reach. *Enumerate or exclude* is `docs/specs/`'s duty
    # alone — a design that registers nothing is the normal case, not an omission — but *registered
    # and excluded* is a contradiction in either tree, and `EXCLUDED` names a design today, so the
    # filter that scoped the whole function to `docs/specs/` made that pair unsayable.
    design = "docs/designs/extension-framework.md"
    both_excluded = {
        "docs/specs/owner-rpc.md": {"story": "AF-7", "reason": "x"},
        design: {"story": "AF-7", "reason": "x"},
    }
    check(
        "a design that is only excluded passes — enumeration is docs/specs/'s duty",
        unenumerated_specs(tree, on_record, both_excluded, spec_statuses) == [],
    )
    check(
        "a design in neither registry is not an omission",
        unenumerated_specs(
            tree,
            on_record,
            {"docs/specs/owner-rpc.md": {"story": "AF-7", "reason": "x"}},
            spec_statuses,
        )
        == [],
    )
    contradicted = unenumerated_specs(
        tree, on_record | {design}, both_excluded, spec_statuses
    )
    check(
        "an excluded design that gains a registration is the same contradiction, and refused",
        len(contradicted) == 1 and design in contradicted[0],
    )

    # Registration must not be nominal: a prefix with no rows behind it silences the enumeration
    # while adding nothing to the denominator, which reads as covered in exactly the direction
    # nobody checks.
    nominal = {"PB": SPECS["PB"], "ZZ": (ROOT / "docs" / "specs" / "zz.md", "§1", "XX-0")}
    rowless = rowless_registrations({"PB-V-1"}, nominal)
    check(
        "a registered prefix that tabulates no row is refused",
        len(rowless) == 1 and rowless[0].startswith("ZZ:"),
    )
    check(
        "a prefix with rows behind it passes",
        rowless_registrations({"PB-V-1"}, {"PB": SPECS["PB"]}) == [],
    )

    # The widened row shape: a rule table keyed by a test-function name is a set of named
    # executable claims, and the guard has to see it — §10's rows were invisible to
    # `ANY_ROW_LINE` because their first cell is not `XX-N`.
    check(
        "a rule-table row keyed by a test-function name is recognised",
        (scenario := SCENARIO_LINE.match("| `owner_rpc_delivers_cross_node` | none | claim |"))
        is not None
        and scenario.group(1) == "owner_rpc_delivers_cross_node",
    )
    check(
        "a two-word schema field is not a scenario",
        SCENARIO_LINE.match("| `expires_at` | absolute instant | §5.2 |") is None,
    )
    check(
        "a camelCase configuration key is not a scenario",
        SCENARIO_LINE.match("| `drainTimeout` | duration | 30 s |") is None,
    )
    check(
        "an uppercase-led value name is not a scenario",
        SCENARIO_LINE.match("| `T_write` | 2 s | less than `T_owner` | owner-side |") is None,
    )
    check(
        "an unbackticked name is prose, not a claim",
        SCENARIO_LINE.match("| owner rpc delivers cross node | none | claim |") is None,
    )
    check(
        "a parenthesized rule-table scenario is recognised and named without its call syntax",
        (
            scenario := SCENARIO_LINE.match(
                "| `owner_rpc_delivers()` | remote owner | request reaches the connection |"
            )
        )
        is not None
        and scenario.group(1) == "owner_rpc_delivers",
    )

    # `CF-27`'s ingestion boundary, using complete story documents rather than a pair of fields.
    # Every valid status must register unchanged: testing only `done` would turn the old denylist
    # into a different denylist, and this story is specifically about making the vocabulary closed.
    for status in LIFECYCLE_STATUSES:
        found, status_problems = ingest_story(
            f"CF-999-{status}-fixture.md", story_fixture(status)
        )
        check(
            f"the lifecycle status `{status}` is accepted from a complete story",
            found == ("CF-999", status) and status_problems == [],
        )

    bad_name = "CF-999-invalid-status-fixture.md"
    found, status_problems = ingest_story(bad_name, story_fixture("don"))
    check(
        "a complete story with `status: don` makes the gate red and names its file",
        found == ("CF-999", "don")
        and len(status_problems) == 1
        and bad_name in status_problems[0]
        and "`status: don`" in status_problems[0],
    )

    commented_name = "CF-999-commented-invalid-status-fixture.md"
    found, status_problems = ingest_story(commented_name, story_fixture("don # typo"))
    check(
        "an invalid status with a YAML comment is parsed, refused, and named",
        found == ("CF-999", "don")
        and len(status_problems) == 1
        and commented_name in status_problems[0]
        and "`status: don`" in status_problems[0],
    )

    mapping_name = "CF-999-mapping-status-fixture.md"
    found, status_problems = ingest_story(mapping_name, story_fixture("{bad: value}"))
    check(
        "a structured status fails closed instead of disappearing from ingestion",
        found == ("CF-999", "{bad: value}")
        and len(status_problems) == 1
        and mapping_name in status_problems[0],
    )

    empty_name = "CF-999-empty-status-fixture.md"
    found, status_problems = ingest_story(empty_name, story_fixture(""))
    check(
        "an empty status fails closed instead of disappearing from ingestion",
        found == ("CF-999", "<empty>")
        and len(status_problems) == 1
        and empty_name in status_problems[0],
    )

    quoted_name = "CF-999-quoted-valid-status-fixture.md"
    found, status_problems = ingest_story(quoted_name, story_fixture('"done"'))
    check(
        "a quoted valid YAML status is normalized and accepted",
        found == ("CF-999", "done") and status_problems == [],
    )

    # The template itself is infrastructure, not a backlog story. An unfilled *copy* is different:
    # it has a story filename but still no conforming id, and must be refused rather than silently
    # becoming the live owner of rows addressed to `{{ID}}`.
    check(
        "the pinned story-ingestion shape is the complete repository template",
        STORY_TEMPLATE == (STORIES / "_TEMPLATE.md").read_text(encoding="utf-8"),
    )
    template, template_problems = ingest_story("_TEMPLATE.md", STORY_TEMPLATE)
    check(
        "the complete story template is excluded from story ingestion",
        template is None and template_problems == [],
    )
    phantom_letters = dead_letters(
        {"CC-D-1": {"story": "{{ID}}", "reason": "x"}}, "deferred", {}, {}
    )
    check(
        "a deferral naming the template placeholder is refused as no story",
        len(phantom_letters) == 1 and "no story in docs/stories/" in phantom_letters[0],
    )
    copy_name = "CF-999-unfilled-template-copy.md"
    copied, copy_problems = ingest_story(copy_name, STORY_TEMPLATE)
    check(
        "a complete but unfilled template copy is refused by file name",
        copied is None
        and len(copy_problems) == 1
        and copy_name in copy_problems[0]
        and "`id: {{ID}}`" in copy_problems[0],
    )

    return failures, cases


def main() -> int:
    check_only = "--check" in sys.argv

    failures, self_test_cases = self_test()
    if failures:
        for failure in failures:
            print(f"self-test: {failure}")
        print(f"\nvectors: FAIL — the check cannot detect what it was built to detect")
        return 1
    if "--self-test" in sys.argv:
        print(
            f"vectors: self-test passed — {self_test_cases} pinned cases cover the PB-F-1 "
            f"proof parser, vector and scenario shapes, live ledger ownership, registry "
            f"enumeration, and story status/id ingestion"
        )
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
    problems += rowless_registrations(rows, SPECS)

    # `CF-24`. Both ledgers are checked against the board's own `status` field, so "deferred with a
    # reason" means a reason with somebody still behind it. `CF-27` added the second return value:
    # that field is only worth reading if it is one of the five values the lifecycle has, and both
    # rules below fail *open* on anything else.
    statuses, status_problems = story_statuses()
    problems += status_problems
    authors, author_problems = spec_authors()
    problems += author_problems
    problems += dead_letters(waived, "deferred", statuses, authors)
    problems += dead_letters(shape_only, "unasserted", statuses, authors)

    # `CF-25`. The enumeration runs in both directions: a spec file nothing registers is refused,
    # and so is a stale entry in the exclusion ledger — including its story going `done`, which is
    # `CF-24`'s liveness rule applied to the one ledger it did not yet cover.
    existing = {
        str(path.relative_to(ROOT))
        for tree in TABLE_TREES
        for path in (ROOT / tree).rglob("*.md")
    }
    registered = {str(path.relative_to(ROOT)) for path, _section, _author in SPECS.values()}
    problems += unenumerated_specs(existing, registered, EXCLUDED, statuses)

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
        f"covered for shape only, {len(waived)} deferred with a live owner"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
