#!/usr/bin/env bash
# The gate. Everything CI runs, in the order that fails fastest.
#
# Run this before marking any story done. If it is green here and red in CI, that difference is a
# bug in this script — fix it here rather than working around it there.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

step() { printf '\n\033[1m── %s\033[0m\n' "$1"; }

step "fmt"
cargo fmt --all --check

step "clippy"
cargo clippy --workspace --all-targets --all-features -- -D warnings

step "tests"
cargo test --workspace --all-features

# The durable location store needs a database, so it is opt-in rather than part of the default gate.
# `SIPX_CLSTR_TEST_DATABASE_URL` runs it; CI has a job that always does.
if [[ -n "${SIPX_CLSTR_TEST_DATABASE_URL:-}" ]]; then
    step "postgres location store"
    cargo test -p sipx-clstr-node --features postgres
fi

step "features"
scripts/check-features.sh

step "provenance"
scripts/check-provenance.sh

step "vectors"
scripts/check-vectors.py --check

step "docs"
scripts/check-docs.sh

printf '\n\033[1;32mgate: green\033[0m\n'
