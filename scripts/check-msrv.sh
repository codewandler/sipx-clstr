#!/usr/bin/env bash
# Build the workspace on the floor it declares.
#
# `rust-version` in Cargo.toml is a promise to every consumer: "this builds on that compiler". It
# is also the one claim in the manifest that a developer's machine can never falsify, because a
# development machine runs current stable and current stable builds everything. So the claim rots
# silently, and the first person to find out is whoever pins the declared floor — which is exactly
# how `CF-9` was found: `KO-13`'s container image pinned `rust-version`, and the image did not
# build.
#
# The floor is read from Cargo.toml rather than written here. A second copy of the number is a
# second thing to forget, and the whole point of this check is that the number and reality cannot
# drift apart.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

if [[ -n "${SIPX_SKIP_MSRV:-}" ]]; then
    echo "msrv: skipped (SIPX_SKIP_MSRV set)"
    exit 0
fi

# `[workspace.package]`'s rust-version, and only it. The crates inherit with `.workspace = true`,
# so there is exactly one line to find.
floor="$(sed -n 's/^rust-version = "\([0-9.]*\)".*/\1/p' Cargo.toml | head -1)"
if [[ -z "$floor" ]]; then
    echo "msrv: could not read rust-version from Cargo.toml" >&2
    exit 2
fi

if ! rustup toolchain list 2>/dev/null | grep -q "^${floor}[.-]"; then
    cat >&2 <<EOF
msrv: the declared floor's toolchain is not installed.

  Cargo.toml declares rust-version = "${floor}", and this check is the only thing that keeps that
  claim true. CI always runs it, so skipping it here only moves the failure to the push.

  Install it once (minimal profile, no docs or extra components):

      rustup toolchain install ${floor} --profile minimal

  On a machine where that is genuinely not possible, SIPX_SKIP_MSRV=1 documents the choice.
EOF
    exit 2
fi

# A separate target directory, not the workspace's. Two rustc versions sharing one target dir
# invalidate each other's fingerprints, so every ordinary `cargo test` after this check would be a
# full rebuild — which is how an MSRV check ends up being deleted for being slow. Kept separate, a
# warm run is seconds.
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR_MSRV:-target/msrv}"

# Deliberately not `-D warnings`. CI sets it workspace-wide, but the question here is "does the
# floor compile this?", not "is the floor's lint set identical to stable's". An older rustc has an
# older lint set: a lint added after the floor cannot fire, and one removed after it can fire for
# no reason a consumer would care about. Failing the floor on that would make the check brittle in
# a way that has nothing to do with the promise it exists to keep.
export RUSTFLAGS=""

echo "  floor ${floor}: $(rustc "+${floor}" --version)"

# `--all-targets --all-features` rather than the library alone: the gate lints and tests that
# surface on stable, and a floor that only holds for `cargo build` is a narrower promise than the
# one the manifest appears to make. `--locked` because a floor established against a resolved
# dependency set is the only kind that means anything.
cargo "+${floor}" check --workspace --all-targets --all-features --locked

echo "msrv: the workspace builds on its declared floor (${floor})"
