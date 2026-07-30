#!/usr/bin/env bash
# Render this chart and feed the result to the real configuration loader.
#
# `deploy/helm/values.yaml` claims its `cluster:` tree is the document of
# `docs/specs/cluster-config.md`, and for two milestones nothing checked that claim: there is no
# `helm` job in CI, so neither `helm template` nor a load of the rendered tree had ever run, and the
# tree had drifted into roughly eighteen keys the loader refuses — including a media policy
# `media-relay` G-M6 will not start on (KO-14, DP-1).
#
# This is the check that makes the claim mechanical. For each role the default deployment set runs,
# the rendered document is loaded through `sipx-clstr run`, which validates and projects it (§8) before
# it touches a socket. A refused document prints "was refused" and every problem in path order; that
# string is the assertion here.
#
# What this does NOT prove: that the resource is a valid custom resource. KO-1 has not pinned the CRD,
# so there is no schema to validate the Kubernetes surface against — only the configuration document
# inside it.
#
# Usage:  deploy/helm/check-values.sh [extra helm template args…]
#         SIPX_CLSTR_BIN=path/to/sipx-clstr deploy/helm/check-values.sh
set -euo pipefail

chart_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$chart_dir/../.." && pwd)"
binary="${SIPX_CLSTR_BIN:-$repo_root/target/debug/sipx-clstr}"

if ! command -v helm >/dev/null 2>&1; then
    echo "check-values: helm is not installed; it renders the chart and there is no substitute" >&2
    exit 1
fi
if [[ ! -x "$binary" ]]; then
    echo "check-values: no node binary at $binary" >&2
    echo "  build it:  cargo build --bin sipx-clstr" >&2
    echo "  or point SIPX_CLSTR_BIN at one" >&2
    exit 1
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

helm lint "$chart_dir"
helm template "$chart_dir" "$@" > "$work/rendered.yaml"
python3 "$chart_dir/node-document.py" < "$work/rendered.yaml" > "$work/cluster.yaml"

# The substitutions §8 V4 resolves from the environment. Literal values, because an undefined name is
# a refusal by design and this check must fail on the document rather than on its own omissions.
export POD_IP="10.42.0.7"
export EDGE_ADDRESS="192.0.2.10"

# Every identity the default deployment set creates a workload for. A projection is per identity, so a
# document that loads for one role can still be refused for another — a role with no listener is
# refused by §5 P4, and that is how the proxy, echo and probe workloads used to fail.
#
# `echo`/`e2e-tester` are not combined with a call-path role (§4 R6), and no identity here runs two
# roles whose listeners share a transport (§5 P6).
identities=(
    "1 edge"
    "2 registrar"
    "3 inbound-proxy"
    "4 outbound-proxy"
    "5 e2e-tester"
    "6 echo"
)

failed=0
for identity in "${identities[@]}"; do
    read -r node roles <<< "$identity"
    log="$work/$node.log"
    # The loader runs before anything binds, so a short run is enough to separate "refused" from
    # "loaded". Whatever happens after the load — a port already in use, a DSN that does not resolve —
    # is the environment's business and not this document's.
    set +e
    timeout 3 "$binary" run --config "$work/cluster.yaml" \
        --node "$node" --zone local --roles "$roles" > "$log" 2>&1
    status=$?
    set -e

    if grep -q "was refused" "$log"; then
        printf '  REFUSED  node %s roles=%-14s\n' "$node" "$roles"
        sed 's/^/           /' "$log"
        failed=1
    elif [[ $status -eq 124 ]]; then
        printf '  loaded   node %s roles=%-14s (still running when the timeout fired)\n' \
            "$node" "$roles"
    else
        printf '  loaded   node %s roles=%-14s (exit %s after the load — environment, not schema)\n' \
            "$node" "$roles" "$status"
        sed 's/^/           /' "$log"
    fi
done

if [[ $failed -ne 0 ]]; then
    echo "check-values: the rendered document is refused by the loader" >&2
    exit 1
fi
echo "check-values: the rendered cluster document loads for every role in the default set"
