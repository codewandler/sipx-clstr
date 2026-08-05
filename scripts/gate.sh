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

# `CF-22`. A step of its own, ahead of the suite that also contains it, and both halves of that are
# deliberate. **Ahead**, because a node holding a transaction no timer will collect is a slow, quiet
# outage and this is the cheapest thing in the gate that would notice — one crate's test binary, run
# in virtual time, so RFC 3261's full 128·T1 costs nothing. **Named**, because the failure it exists
# for is a resource lifetime rather than a wrong answer, and a red here has to say *which resource*;
# "tests" does not. `cargo test --workspace` below runs it a second time for a few milliseconds, and
# that is the price of the step naming itself.
#
# It also carries the number `scripts/e2e-call.sh` got wrong. Its bound is **128·T1**, derived and
# measured rather than assumed: a proxied request to a silent next hop spends Timer F (or B) on the
# client transaction before §16.7 has a final response to forward, and only then does Timer J (or H)
# start on the server transaction. A one-window bound rejects correct code — it is what reverted
# `PX-13` — so the test that measures the worst case goes red if this bound is halved.
step "transaction drain"
cargo test -p sipx-clstr-sim --test transaction_drain

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

# Here rather than only in CI, on this repository's own rule: green locally and red in CI is a bug
# in this script. The declared floor is the one claim a current-stable machine structurally cannot
# falsify, so leaving it to CI guarantees the failure is found after the push instead of before.
# It is a `check` on its own target directory, so a warm run is seconds; only the first is slow.
step "msrv"
scripts/check-msrv.sh

step "provenance"
scripts/check-provenance.sh

step "vectors"
scripts/check-vectors.py --check

# `CF-20`. The real e2e job exercises this against the node, but the gate must prove all failure
# directions without depending on the runner's current sockets or privilege level.
step "e2e socket assertion"
scripts/tests/e2e-socket-count.sh

# `KO-1`. The SipxCluster resource and cluster-config §7 are one definition; this is the half of that
# claim a contributor can run. Reads names only — the rendered document's *contents* are
# `deploy/helm/check-values.sh`'s, which needs helm and a built binary and so is not in the gate.
step "crd drift"
scripts/check-crd-drift.py

step "docs"
scripts/check-docs.sh

# `FC-5`. The end-to-end proofs are the evidence behind the release's cluster claim, and they are the
# one part of the gate that nothing else compiles or runs — `FC-4` changed the node underneath them
# and three of them answered `403` for a release cycle while everything here stayed green. Cheap
# enough to sit in the fast half: it reads four files and compares strings.
step "proof domains"
scripts/check-proof-domains.py

# `DX-12`. Deliberately **after** the build, because that is what makes it worth more here than in
# CI: `target/debug/sipx-clstr` exists by now, so the CLI reference is checked against the binary's
# own `--help` rather than against a static reading of `main.rs`. The `docs` workflow has no Rust
# toolchain and gets the static reading; the script says which one it used on every run.
step "site"
scripts/check-site.py

printf '\n\033[1;32mgate: green\033[0m\n'
