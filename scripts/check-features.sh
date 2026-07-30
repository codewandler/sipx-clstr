#!/usr/bin/env bash
# Build the feature combinations a downstream user might actually select.
#
# `--all-features` is not enough. A crate that does not compile with an optional feature turned
# *off* is invisible until someone turns it off, and by then it is in a release. The kernel has
# already shipped that bug once (its TLS feature), which is why this check exists here from the
# first commit rather than after the first incident.
#
# The combinations are the ones that mean something — each optional layer alone, and the default
# set — rather than the full power set, which is slow and mostly duplicated work.
set -euo pipefail

# The same flags CI builds with. Without this the script is a weaker check than the job that runs
# it: an unused import behind a disabled feature is a warning here and an error there, so a change
# can pass locally and fail on push.
export RUSTFLAGS="${RUSTFLAGS:--D warnings}"

# crate:features — an empty feature list means --no-default-features on its own.
combinations=(
    # No optional features today, and listed anyway: this list is hand-maintained, so a crate that
    # is not on it is a crate this check cannot see. `AF-4` added the entry with the crate.
    "sipx-clstr-affinity:"
    "sipx-clstr-registrar:"
    "sipx-clstr-registrar:serde"
    "sipx-clstr-registrar:test-suite"
    "sipx-clstr-registrar:serde,test-suite"
    "sipx-clstr-proxy:"
    "sipx-clstr-proxy:registrar-targets"
    "sipx-clstr-probe:"
    "sipx-clstr-sim:"
    "sipx-clstr-node:"
    "sipx-clstr-node:postgres"
)

status=0
for entry in "${combinations[@]}"; do
    crate="${entry%%:*}"
    features="${entry#*:}"
    label="$crate ${features:-<none>}"
    printf '  %-40s ' "$label"
    scratch="$(mktemp)"
    if cargo check --quiet -p "$crate" --no-default-features \
        ${features:+--features "$features"} 2>"$scratch"; then
        echo "ok"
    else
        echo "FAILED"
        cat "$scratch"
        status=1
    fi
    rm -f "$scratch"
done

if [ "$status" -eq 0 ]; then
    echo "features: every combination builds"
fi
exit "$status"
