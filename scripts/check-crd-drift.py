#!/usr/bin/env python3
"""The `SipxCluster` custom resource and the configuration schema are one definition, not two.

`docs/specs/cluster-config.md` §7 is the section registry of the cluster configuration document.
`docs/specs/sipx-cluster-crd.md` says the custom resource's `spec` **is** that document's `cluster:`
tree, verbatim, plus a closed set of deployment fields the document deliberately cannot express.
"Is" is a claim, and a claim nothing executes is prose — so this check reads the five places the two
halves are spelled and fails when any two disagree.

`KO-1` decided the single-source mechanism and this script is the half of that decision a contributor
can run. The mechanism is **one shared definition with a declared inclusion**: neither artefact is
generated from the other, because generation needs a schema compiler that would have to exist in one
of them first, and whichever direction it ran the *other* file would become the derived copy nobody
reads. Instead §7 stays the sole place a configuration section is defined, the CRD spec **names** each
section without restating any of its fields, and this check enforces that the naming is total in both
directions. A field can therefore be added in exactly one place, and forgetting the second place is a
red gate rather than a silent divergence.

That failure mode is not hypothetical here. `DP-8` replaced three provisional flags and roughly thirty
documented commands went on naming flags the binary had stopped accepting, through a release, with the
whole gate green — the same class, one tree over. `DP-11` then added `admission` to §7, and nothing
anywhere would have noticed had the resource not grown the section with it.

What is compared, and against what:

1. **The schema version is one string.** The resource's `apiVersion` *is* the schema version of the
   document it carries (cluster-config §3 — one version field on the thing being versioned), so
   `sipx-cluster-crd.md`'s pin, `cluster-config.md` §3's example, the loader's `API_VERSION` constant
   and `deploy/helm/values.yaml`'s `apiVersion` must be byte-identical. Four spellings, one fact.
2. **The config half is exactly §7.** Every section §7 registers has a row in the CRD spec's mapping
   table, and every config-half row names a section §7 registers. A section added to §7 without a row
   is drift; a row naming a section §7 does not have is an invented dialect.
3. **The operator half is closed and agreed.** The deployment fields the CRD spec declares are exactly
   `node-document.py`'s `OPERATOR_KEYS` and exactly the keys `templates/sipxcluster.yaml` writes into
   `spec` itself. Those three lists decide, between them, which half of `spec` a node reads: a key the
   template starts writing that nobody adds to the other two leaks into the configuration document,
   where §8 V2's closed world refuses it by name at start-up rather than here.
4. **The mapping is 1:1 with the chart.** Every `values.yaml` path the table names resolves; every
   top-level key the chart writes under `cluster:` is named by a row; and a row that says the default
   set omits a section is checked to have actually omitted it. A key the chart grows and the table
   does not is drift in the direction the table is supposed to prevent.
5. **The `deployment:` half is closed and declared too.** Every path the chart writes under
   `deployment:` — one level below each key — is either an operator-half row of §5 or a row of §5's
   chart-local table, and every chart-local row resolves. §5 has always said that a `values.yaml` key
   with no `SipxCluster` field is either a chart-managed dependency or a defect, and for two stories
   nothing read that sentence: `deployment.rtpengine.enabled` sat under it as a second spelling of
   `cluster.mediaPool[].mode: managed`, recorded in §11 and green in the gate (`KO-15`). Reading one
   level down is what makes a switch added *inside* a declared block as visible as a new block.

What this does NOT check: that the resource is a valid CRD (there is no CRD manifest until `KO-3`),
that the operator honours any of it, or that the chart's rendered document loads — the last is
`deploy/helm/check-values.sh`, which feeds the rendered tree to the real loader and is the only thing
that reads the *contents* of a section. This script reads names.

`--self-test` replays a drift of each kind against fixtures and asserts this script refuses it. It runs
on every invocation, because a checker that has stopped detecting is worse than no checker.

Exit 0 when the definitions agree, 1 otherwise.
"""

from __future__ import annotations

import pathlib
import re
import sys
import typing

try:
    import yaml
except ModuleNotFoundError:  # pragma: no cover - the message *is* the behaviour
    # Loudly, and never by checking less. `check-site.py` treats PyYAML as optional because it has a
    # weaker reading to fall back on; this check has none — the chart's `cluster:` keys are one of the
    # definitions being compared, and a hand-rolled YAML reader would be a second parser to trust.
    raise SystemExit(
        "crd-drift: FAIL — PyYAML is not installed, and this check cannot read "
        "deploy/helm/values.yaml without it.\n"
        "  install it:  python3 -m pip install pyyaml"
    ) from None

ROOT = pathlib.Path(__file__).resolve().parent.parent
CONFIG_SPEC = ROOT / "docs" / "specs" / "cluster-config.md"
CRD_SPEC = ROOT / "docs" / "specs" / "sipx-cluster-crd.md"
VALUES = ROOT / "deploy" / "helm" / "values.yaml"
TEMPLATE = ROOT / "deploy" / "helm" / "templates" / "sipxcluster.yaml"
NODE_DOCUMENT = ROOT / "deploy" / "helm" / "node-document.py"
LOADER = ROOT / "crates" / "sipx-clstr-node" / "src" / "config" / "mod.rs"

# The registry table of cluster-config §7. Its first cell names one or more sections, backticked, and
# anything after the first `(` is the owner's own field list — `listener[]` (`roles`, `transport`, …) —
# which this check deliberately does not read. Restating those fields is the defect it exists to stop.
SECTION_HEADING = re.compile(r"^#+\s*7\.")
NEXT_HEADING = re.compile(r"^#+\s")
TABLE_ROW = re.compile(r"^\|(.+)\|\s*$")
BACKTICKED = re.compile(r"`([^`]+)`")

# `apiVersion: <group>/<version>` from the first fenced block that carries one.
FENCE = re.compile(r"^```")
API_VERSION_LINE = re.compile(r"^\s*apiVersion:\s*(\S+)")

# `pub const API_VERSION: &str = "sipx.dev/v1alpha1";`
LOADER_CONST = re.compile(r'API_VERSION:\s*&str\s*=\s*"([^"]+)"')
# `OPERATOR_KEYS = ("image", "roles", "nodeSelector", "tolerations")`
OPERATOR_KEYS_ASSIGNMENT = re.compile(r"OPERATOR_KEYS\s*=\s*\(([^)]*)\)")

# The halves a mapping row may declare. A row is read only when its third cell is one of these, which
# is what lets the spec carry other tables — the Kubernetes-native fields, the status conditions —
# without this parser having an opinion about them.
CONFIG, OPERATOR = "config", "operator"
ABSENT = "—"

# A row of §5's chart-local table: three cells, the first a backticked `deployment.` path. The
# three-cell shape alone is not enough — the Kubernetes-native table beside it is also three cells —
# so the `deployment.` prefix is what tells them apart, and it is also the only prefix this table is
# allowed to carry: a chart-local row is by definition a key that reaches no `SipxCluster` field.
CHART_LOCAL_PATH = re.compile(r"^`(deployment\.[A-Za-z][\w.-]*)`$")


def cells(line: str) -> list[str] | None:
    """The cells of a markdown table row, or `None` if `line` is not one."""
    found = TABLE_ROW.match(line.rstrip())
    if not found:
        return None
    return [cell.strip() for cell in found.group(1).split("|")]


def registered_sections(text: str) -> list[str]:
    """The section names cluster-config §7 registers, in the order it registers them.

    Read from §7's table only. A backticked name elsewhere in that spec is a citation, and reading
    citations would let a section into the registry by being mentioned.
    """
    sections: list[str] = []
    inside = False
    for line in text.splitlines():
        if SECTION_HEADING.match(line):
            inside = True
            continue
        if inside and NEXT_HEADING.match(line):
            break
        if not inside:
            continue
        row = cells(line)
        if not row or len(row) < 2:
            continue
        first = row[0]
        if first.startswith("---") or first in ("Section", "#"):
            continue
        # The owner's field list is not the registry's business.
        named = BACKTICKED.findall(first.split("(")[0])
        sections.extend(name.removesuffix("[]") for name in named)
    return sections


def pinned_version(text: str) -> str | None:
    """The `group/version` the CRD spec pins, read from its first fenced block that carries one."""
    fenced = False
    for line in text.splitlines():
        if FENCE.match(line):
            fenced = not fenced
            continue
        if fenced and (found := API_VERSION_LINE.match(line)):
            return found.group(1)
    return None


class Mapping(typing.NamedTuple):
    """One row of the CRD spec's `values.yaml` → `spec` table."""

    values_path: str
    spec_field: str
    half: str


def mapping_rows(text: str) -> list[Mapping]:
    """The mapping table's rows: every four-cell row whose third cell names a half."""
    rows: list[Mapping] = []
    for line in text.splitlines():
        row = cells(line)
        if not row or len(row) != 4 or row[2] not in (CONFIG, OPERATOR):
            continue
        values_path = row[0] if row[0] == ABSENT else "".join(BACKTICKED.findall(row[0]))
        spec_field = "".join(BACKTICKED.findall(row[1]))
        rows.append(Mapping(values_path, spec_field, row[2]))
    return rows


def template_spec_keys(text: str) -> list[str]:
    """The keys `templates/sipxcluster.yaml` writes into `spec` itself.

    Everything else in `spec` is `.Values.cluster` copied in verbatim, so this is the operator half as
    the chart actually spells it — the one of the three lists that a rendered resource proves.
    """
    keys: list[str] = []
    inside = False
    for line in text.splitlines():
        if line.startswith("spec:"):
            inside = True
            continue
        if not inside:
            continue
        if line and not line[0].isspace():
            break
        if found := re.match(r"^  ([A-Za-z][\w-]*):", line):
            keys.append(found.group(1))
    return keys


def operator_keys(text: str) -> list[str]:
    """`node-document.py`'s `OPERATOR_KEYS`, the list that subtracts the operator half."""
    found = OPERATOR_KEYS_ASSIGNMENT.search(text)
    if not found:
        return []
    named = found.group(1).replace('"', "").replace("'", "").split(",")
    return [key.strip() for key in named if key.strip()]


def chart_local_paths(text: str) -> list[str]:
    """The `deployment:` paths §5's chart-local table declares — the keys that reach no `spec` field.

    Read by prefix rather than by position, so the table can move within §5 and so the
    Kubernetes-native table's three-cell rows beside it are not mistaken for declarations.
    """
    paths: list[str] = []
    for line in text.splitlines():
        row = cells(line)
        if not row or len(row) != 3:
            continue
        if found := CHART_LOCAL_PATH.match(row[0]):
            paths.append(found.group(1))
    return paths


def chart_deployment_paths(deployment: dict, mapped: set[str]) -> list[str]:
    """The `deployment:` paths the chart writes that a chart-local row has to declare.

    A key the mapping table sends to `spec` is copied verbatim and is not descended into: its fields
    belong to the resource, and §5 names the key rather than the tree under it. Everything else is
    the chart's own and is read **one level down**, so that a switch added inside an already-declared
    block — which is exactly the shape `KO-15` removed — is as visible as a whole new block. Deeper
    than one level is a value's shape rather than a fact the chart decides.
    """
    paths: list[str] = []
    for key, value in deployment.items():
        if f"deployment.{key}" in mapped:
            continue
        if isinstance(value, dict) and value:
            paths.extend(f"deployment.{key}.{sub}" for sub in value)
        else:
            paths.append(f"deployment.{key}")
    return paths


def resolve(tree: object, path: str) -> tuple[bool, object]:
    """Walk a dotted path through a loaded YAML tree. `(found, value)`."""
    current = tree
    for step in path.split("."):
        if not isinstance(current, dict) or step not in current:
            return False, None
        current = current[step]
    return True, current


def missing_files() -> list[str]:
    return [
        f"{path.relative_to(ROOT)} is missing — this check reads it as one of the "
        f"definitions that must agree"
        for path in (CONFIG_SPEC, CRD_SPEC, VALUES, TEMPLATE, NODE_DOCUMENT, LOADER)
        if not path.is_file()
    ]


def check() -> list[str]:
    if problems := missing_files():
        return problems

    config_text = CONFIG_SPEC.read_text(encoding="utf-8")
    crd_text = CRD_SPEC.read_text(encoding="utf-8")
    values = yaml.safe_load(VALUES.read_text(encoding="utf-8")) or {}

    problems: list[str] = []

    # ── 1. one schema version, four spellings ────────────────────────────────────────────────────
    spellings = {
        "docs/specs/sipx-cluster-crd.md (the pin)": pinned_version(crd_text),
        "docs/specs/cluster-config.md §3": pinned_version(config_text),
        "crates/sipx-clstr-node/src/config/mod.rs API_VERSION": (
            found.group(1)
            if (found := LOADER_CONST.search(LOADER.read_text(encoding="utf-8")))
            else None
        ),
        "deploy/helm/values.yaml apiVersion": values.get("apiVersion"),
    }
    if unreadable := [where for where, value in spellings.items() if not value]:
        problems += [
            f"no schema version could be read from {where} — the resource's apiVersion is the "
            f"document's schema version, so every one of the four must spell it"
            for where in unreadable
        ]
    elif len(set(spellings.values())) != 1:
        problems.append(
            "the schema version is spelled four ways and they disagree: "
            + "; ".join(f"{where} = {value}" for where, value in spellings.items())
        )

    # ── 2 & 3. the two halves of `spec` ──────────────────────────────────────────────────────────
    sections = registered_sections(config_text)
    if not sections:
        return problems + [
            "no sections could be read out of cluster-config.md §7 — the registry is this check's "
            "source of truth and an empty read would pass everything"
        ]
    rows = mapping_rows(crd_text)
    if not rows:
        return problems + [
            "docs/specs/sipx-cluster-crd.md carries no mapping rows — the table's rows are "
            "`| values path | spec field | config|operator | owner |`"
        ]

    declared = {half: [] for half in (CONFIG, OPERATOR)}
    for row in rows:
        declared[row.half].append(row.spec_field.removeprefix("spec."))

    for section in sections:
        if section not in declared[CONFIG]:
            problems.append(
                f"cluster-config §7 registers `{section}` and the resource does not carry it — "
                f"add a `config` row for `spec.{section}` to sipx-cluster-crd.md's mapping table"
            )
    for field in declared[CONFIG]:
        if field not in sections:
            problems.append(
                f"sipx-cluster-crd.md maps `spec.{field}` as configuration and cluster-config §7 "
                f"registers no such section — the resource must not invent a second dialect"
            )
    for half, values_of in ((CONFIG, declared[CONFIG]), (OPERATOR, declared[OPERATOR])):
        duplicated = {name for name in values_of if values_of.count(name) > 1}
        problems += [
            f"sipx-cluster-crd.md maps `spec.{name}` twice in the {half} half" for name in sorted(duplicated)
        ]

    from_node_document = operator_keys(NODE_DOCUMENT.read_text(encoding="utf-8"))
    from_template = template_spec_keys(TEMPLATE.read_text(encoding="utf-8"))
    for where, found in (
        ("node-document.py's OPERATOR_KEYS", from_node_document),
        ("templates/sipxcluster.yaml's own `spec` keys", from_template),
    ):
        if sorted(found) != sorted(declared[OPERATOR]):
            problems.append(
                f"the operator half of `spec` disagrees: sipx-cluster-crd.md declares "
                f"{sorted(declared[OPERATOR])} and {where} has {sorted(found)} — a key in one and "
                f"not the others is read as configuration by whichever list omits it"
            )

    # ── 4. the mapping is 1:1 with the chart ─────────────────────────────────────────────────────
    # A row that says the default set omits a section still *names* that section, so a chart which
    # grows it gets the one message that says what to do — the `—` has to become a path — rather than
    # that plus a second one claiming no row mentions it.
    named_in_table: set[str] = set()
    for row in rows:
        if row.values_path == ABSENT:
            if row.half == CONFIG:
                named_in_table.add(f"cluster.{row.spec_field.removeprefix('spec.')}")
            continue
        named_in_table.add(row.values_path)
        found, _ = resolve(values, row.values_path)
        if not found:
            problems.append(
                f"sipx-cluster-crd.md maps `{row.values_path}` to `{row.spec_field}` and "
                f"deploy/helm/values.yaml has no such key — either the chart dropped it or the "
                f"row should say `{ABSENT}`"
            )
    for row in rows:
        if row.values_path != ABSENT:
            continue
        candidate = f"cluster.{row.spec_field.removeprefix('spec.')}"
        if row.half == CONFIG and resolve(values, candidate)[0]:
            problems.append(
                f"sipx-cluster-crd.md says the default set omits `{candidate}` and "
                f"deploy/helm/values.yaml carries it — the mapping is the chart's contract, so an "
                f"added section is a row that has to change with it"
            )

    for key in (values.get("cluster") or {}):
        if f"cluster.{key}" not in named_in_table:
            problems.append(
                f"deploy/helm/values.yaml writes `cluster.{key}` and sipx-cluster-crd.md's mapping "
                f"table names no row for it — the mapping is field by field or it is decoration"
            )

    # ── 5. the `deployment:` half is closed and declared too ─────────────────────────────────────
    # The operator half is four rows of the mapping table; everything else under `deployment:` is the
    # chart's own and has to say so in §5's chart-local table, with a reason. Undeclared is not
    # "probably a dependency" — it is the state `deployment.rtpengine.enabled` was in.
    mapped_deployment = {
        row.values_path
        for row in rows
        if row.half == OPERATOR and row.values_path.startswith("deployment.")
    }
    declared_local = chart_local_paths(crd_text)
    deployment = values.get("deployment")
    if not isinstance(deployment, dict):
        problems.append(
            "deploy/helm/values.yaml has no `deployment:` tree — it is one of the two trees the "
            "chart is made of, and this check reads it as the operator half's source"
        )
    elif not declared_local:
        problems.append(
            "sipx-cluster-crd.md §5 declares no chart-local `deployment.` rows — every key under "
            "`deployment:` that reaches no `SipxCluster` field needs one, and an empty read would "
            "pass every key the chart grows"
        )
    else:
        for path in chart_deployment_paths(deployment, mapped_deployment):
            if path not in declared_local:
                problems.append(
                    f"deploy/helm/values.yaml writes `{path}` and sipx-cluster-crd.md declares no "
                    f"row for it — a `deployment:` key with no `SipxCluster` field is either a "
                    f"chart-local one §5 declares with a reason or a defect (M7)"
                )
        for path in declared_local:
            if f"deployment.{path.split('.')[1]}" in mapped_deployment:
                problems.append(
                    f"sipx-cluster-crd.md declares `{path}` chart-local and the mapping table sends "
                    f"that key to `spec` — K4: no key is in both halves"
                )
            elif not resolve(values, path)[0]:
                problems.append(
                    f"sipx-cluster-crd.md declares `{path}` as a chart-local key and "
                    f"deploy/helm/values.yaml has no such key — the chart dropped it, or the row did"
                )

    return problems


# ── the self-test ────────────────────────────────────────────────────────────────────────────────
#
# Each fixture is the smallest thing that exercises one parser, and each drift is one a contributor
# could plausibly commit. The point is not coverage of the drift space; it is that the parsers still
# read what they claim to read, since every check above is only as good as its parse.

REGISTRY_FIXTURE = """
## 7. The section registry

| Section | Roles that consume it | Content owned by | Reload class |
|---|---|---|---|
| `name`, `environment`, `zones` | all | this spec | rollout |
| `listener[]` (`roles`, `transport`, `bind`) | as declared | `DP-5` | rollout |
| `admission` (`maxInFlightTransactions`) | `edge` | `DP-11` | reloadable |

| # | Rule |
|---|---|
| S1 | **A section has exactly one owner.** |

## 8. Validation
| `notASection` | not in the registry | no | no |
"""

MAPPING_FIXTURE = """
| `values.yaml` path | `SipxCluster` field | Half | Content owned by |
|---|---|---|---|
| `cluster.name` | `spec.name` | config | cluster-config §7 |
| — | `spec.admission` | config | `DP-11` |
| `deployment.image` | `spec.image` | operator | this spec |

| `values.yaml` path | Where it goes | Why |
|---|---|---|
| `apiVersion` | the resource's own `apiVersion` | one version field |
"""

CHART_LOCAL_FIXTURE = """
| `values.yaml` path | What the chart does with it | Why it is not a field of this resource |
|---|---|---|
| `deployment.operator.replicas` | sizes the operator's own Deployment | the operator is not a cluster node |
| `deployment.affinity` | pod affinity for the objects the chart creates | K3's operator half is closed |
"""


def self_test() -> list[str]:
    failures: list[str] = []

    def check_that(claim: str, held: bool) -> None:
        if not held:
            failures.append(claim)

    sections = registered_sections(REGISTRY_FIXTURE)
    check_that(
        f"§7's first cell yields one name per section, `[]` stripped, read as {sections}",
        sections == ["name", "environment", "zones", "listener", "admission"],
    )
    check_that(
        "a rule table inside §7 contributes no sections",
        "S1" not in sections and "Rule" not in sections,
    )
    check_that(
        "a table after §7 is not part of the registry — this is what stops §8 registering sections",
        "notASection" not in sections,
    )

    rows = mapping_rows(MAPPING_FIXTURE)
    check_that(
        f"only rows naming a half are read, found {[row.spec_field for row in rows]}",
        [row.spec_field for row in rows] == ["spec.name", "spec.admission", "spec.image"],
    )
    check_that(
        "a three-cell table beside the mapping table is not read as a mapping row",
        all(row.values_path != "apiVersion" for row in rows),
    )
    check_that(
        "a section the default set omits is read as absent rather than as a path",
        [row.values_path for row in rows if row.spec_field == "spec.admission"] == [ABSENT],
    )
    check_that(
        "the halves are told apart",
        [row.half for row in rows] == [CONFIG, CONFIG, OPERATOR],
    )

    check_that(
        "the pinned version is read out of a fenced block",
        pinned_version("text\n```yaml\napiVersion: sipx.dev/v1alpha1\nkind: SipxCluster\n```")
        == "sipx.dev/v1alpha1",
    )
    check_that(
        "an apiVersion in prose is not a pin — the pin is the spec's own example resource",
        pinned_version("the group is apiVersion: sipx.dev/v9 in passing") is None,
    )

    check_that(
        "the template's own `spec` keys are read at one indent level",
        template_spec_keys(
            "spec:\n  image:\n    repository: x\n  roles:\n    {{- toYaml . }}\n"
            "  {{- with .Values.deployment.tolerations }}\n  tolerations:\n  {{- end }}\n"
        )
        == ["image", "roles", "tolerations"],
    )
    check_that(
        "OPERATOR_KEYS is read as a list of bare names",
        operator_keys('OPERATOR_KEYS = ("image", "roles", "nodeSelector")')
        == ["image", "roles", "nodeSelector"],
    )

    local = chart_local_paths(CHART_LOCAL_FIXTURE)
    check_that(
        f"a chart-local row is read by its `deployment.` prefix, found {local}",
        local == ["deployment.operator.replicas", "deployment.affinity"],
    )
    check_that(
        "the Kubernetes-native table's three-cell rows are not chart-local declarations",
        chart_local_paths(MAPPING_FIXTURE) == [],
    )

    written = chart_deployment_paths(
        {"image": {"tag": ""}, "rtpengine": {"enabled": True, "replicas": 1}, "affinity": {}},
        {"deployment.image"},
    )
    check_that(
        f"a key mapped to `spec` is a verbatim subtree and is not descended into, read as {written}",
        written
        == ["deployment.rtpengine.enabled", "deployment.rtpengine.replicas", "deployment.affinity"],
    )
    check_that(
        "a switch added inside an already-declared block is undeclared — KO-15's own shape",
        "deployment.rtpengine.enabled" in written and "deployment.rtpengine.enabled" not in local,
    )

    check_that(
        "a dotted path is resolved through the tree",
        resolve({"cluster": {"tenant": [1]}}, "cluster.tenant") == (True, [1])
        and resolve({"cluster": {}}, "cluster.tenant")[0] is False,
    )
    return failures


def main() -> int:
    if failures := self_test():
        for failure in failures:
            print(f"self-test: {failure}")
        print("\ncrd-drift: FAIL — the check cannot detect what it was built to detect")
        return 1
    if "--self-test" in sys.argv:
        print("crd-drift: self-test passed — every parser still reads what it claims to")
        return 0

    problems = check()
    for problem in problems:
        print(problem)
    if problems:
        print(f"\ncrd-drift: FAIL — {len(problems)} problem(s)")
        return 1
    print(
        "crd-drift: the custom resource and cluster-config §7 are one definition — "
        "one schema version, every section named, the mapping 1:1 with the chart, "
        "the `deployment:` half declared"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
