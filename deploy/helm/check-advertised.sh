#!/usr/bin/env bash
# Hold the chart's package metadata to what `helm template` actually renders.
#
# `helm show chart` is the one surface an operator reads without opening a template, and until
# KO-16 it promised a complete installation — the operator, its CRDs and RBAC, a self-contained
# local environment — while the only template was a single custom resource nothing serves (V-19).
# A render that succeeds is easy to mistake for a deployment that works; this check makes the
# package say the opposite out loud until the executable pieces exist, and goes red the moment
# the metadata starts promising again.
#
# What it asserts, from the two acceptance surfaces (`helm show chart`, `helm template`) plus the
# one chart file helm deliberately never renders:
#
#   1. the description labels the chart non-operational/unserved and names what is absent
#      (no CRD, no operator), and no longer claims to install the executable pieces;
#   2. version/appVersion are the repository's release (Cargo.toml [workspace.package]) — the
#      only version this repo actually cuts — rather than a package number nothing ever released;
#   3. the rendered object inventory is exactly one document of kind SipxCluster, and the
#      rendered stream itself carries the UNSERVED marker, so a manifest saved to a file and
#      read later still says what applying it will not do;
#   4. templates/NOTES.txt exists and names the missing pieces and the stories that ship them —
#      read from source, because `helm template` never emits notes and this environment has no
#      cluster for an install --dry-run.
#
# Not a gate step, for the same reason as check-values.sh (scripts/gate.sh names it): it needs
# helm. What it does NOT prove: that the rendered document loads — that is check-values.sh's,
# through the node's own loader.
#
# Usage: deploy/helm/check-advertised.sh
set -euo pipefail

chart_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$chart_dir/../.." && pwd)"

if ! command -v helm >/dev/null 2>&1; then
    echo "check-advertised: helm is not installed; it renders the surfaces being checked" >&2
    exit 1
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

helm show chart "$chart_dir" > "$work/chart.yaml"
helm template "$chart_dir" > "$work/rendered.yaml"

CHART_DIR="$chart_dir" REPO_ROOT="$repo_root" WORK="$work" python3 - <<'PY'
import os
import re
import sys
import tomllib

import yaml

chart_dir = os.environ["CHART_DIR"]
repo_root = os.environ["REPO_ROOT"]
work = os.environ["WORK"]

with open(os.path.join(work, "chart.yaml")) as f:
    chart = yaml.safe_load(f)
with open(os.path.join(work, "rendered.yaml")) as f:
    rendered = f.read()
with open(os.path.join(repo_root, "Cargo.toml"), "rb") as f:
    release = tomllib.load(f)["workspace"]["package"]["version"]

problems = []
description = chart.get("description") or ""

# V-19's defect, matched as the claim rather than as its words: "Installs the …" is a promise
# about what `helm install` does, and the honest description uses the same nouns (CRD, operator,
# RBAC) to say they are absent — so the tripwire is the verb phrase, not the vocabulary.
if re.search(r"[Ii]nstalls the\b", description):
    problems.append(
        "the description promises a complete installation (“Installs the …”) "
        "while the chart renders one unserved custom resource and ships no CRD, operator, "
        "RBAC or workload"
    )

# The label has to be explicit, not inferable: an operator reading `helm show chart` gets one
# paragraph, and that paragraph must say the thing is not operational and what is missing.
if not re.search(r"non-operational|unserved", description, re.IGNORECASE):
    problems.append(
        "the description never labels the chart non-operational/unserved; a successful render "
        "then impersonates a working deployment"
    )
if "no CRD" not in description:
    problems.append("the description does not say that no CRD ships")
if not re.search(r"no operator|no controller", description):
    problems.append("the description does not say that no operator/controller ships")

# One version fact in this repository: the release Cargo.toml [workspace.package] cuts. The
# chart is not published (KO-12) and there is no image behind appVersion's default tag (KO-17),
# so any other number here is a package nothing ever released, presented as current fact.
for field in ("version", "appVersion"):
    value = str(chart.get(field, ""))
    if value != release:
        problems.append(
            f"Chart.yaml {field} is {value!r} but the repository's release is {release!r} "
            f"(Cargo.toml [workspace.package] version) — the chart may not advertise a "
            f"package this project never cut"
        )

# The inventory claim, from the surface itself: exactly one rendered object, and it is the
# SipxCluster. Anything more is a template this check has not heard of; anything less is a chart
# that no longer even renders its one resource.
documents = [doc for doc in yaml.safe_load_all(rendered) if doc]
kinds = [doc.get("kind") for doc in documents]
if kinds != ["SipxCluster"]:
    problems.append(
        f"expected the rendered inventory to be exactly one SipxCluster, found {kinds!r}"
    )

# The rendered stream must carry the marker itself: a manifest piped to a file loses NOTES.txt
# and the chart description, and `kubectl apply -f` is read by people who saw neither.
if "UNSERVED" not in rendered:
    problems.append(
        "the rendered manifest carries no UNSERVED marker; saved output looks like a ready "
        "SIP deployment"
    )

# The install-time note. helm never templates NOTES.txt into manifests, so the source file is
# the only offline surface to hold; helm prints it verbatim at install and --dry-run time.
notes_path = os.path.join(chart_dir, "templates", "NOTES.txt")
if not os.path.exists(notes_path):
    problems.append(
        "templates/NOTES.txt does not exist — a fresh install prints nothing, and the one "
        "moment an operator is guaranteed to be reading goes silent"
    )
else:
    with open(notes_path) as f:
        notes = f.read()
    if "CRD" not in notes or not re.search(r"operator|controller", notes):
        problems.append(
            "NOTES.txt does not say that no CRD/controller ships with this chart"
        )
    for blocker in ("KO-3", "ET-4"):
        if blocker not in notes:
            problems.append(f"NOTES.txt does not name blocker {blocker}")
    if "https://github.com/codewandler/sipx-clstr" not in notes:
        problems.append(
            "NOTES.txt names no link to the blockers; the note must let an operator reach "
            "them without a checkout"
        )

if problems:
    print("check-advertised: the chart advertises more than it installs", file=sys.stderr)
    for problem in problems:
        print(f"  - {problem}", file=sys.stderr)
    sys.exit(1)

print(
    "check-advertised: metadata matches the rendered inventory — one SipxCluster, "
    "labeled unserved, at release " + release
)
PY
