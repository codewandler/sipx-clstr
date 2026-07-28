#!/usr/bin/env bash
#
# Provenance gate, with the integration carve-out.
#
# AGENTS.md non-negotiable #1: SIP-stack prior art is never referenced as design rationale —
# "another SIP server does it this way" is not an argument, anywhere: code, comments, docs,
# stories, designs, fixture names, package metadata or commit messages. Rationale cites RFCs,
# sipx specs, or our own specs in docs/specs/.
#
# **The carve-out.** Named integration and interop targets — systems this platform talks to or is
# tested against — may be named anywhere, as targets and never as behavioral precedent. Those
# names live in scripts/provenance-allow.txt, in this repository, with a reason each. That is not
# a contradiction: a term we are willing to write down is by definition not a term we refuse.
#
# The denylist itself is deliberately NOT stored here, so that the terms we refuse to mention are
# not mentioned by the check that refuses them. It is supplied by:
#
#   SIPX_DENYLIST       — newline- or comma-separated terms, or
#   SIPX_DENYLIST_FILE  — path to a file with one term per line (# comments allowed)
#
# and falls back to ~/notes/sipx-research/denylist.txt for local runs.
#
# Usage:  scripts/check-provenance.sh [--history]
#         --history   also scan the full commit log (slower; run in CI and before release)
#
# Exit codes: 0 clean · 1 hit found · 2 misconfigured (no denylist available in CI)

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

scan_history=0
[[ "${1:-}" == "--history" ]] && scan_history=1

# ---------------------------------------------------------------- load the denylist ----
terms=()

# `read` drops a trailing line that has no newline, so every loop below uses the
# `|| [[ -n "$line" ]]` form. Losing the last term would make this gate pass silently.
add_terms() {
    local line term
    while IFS= read -r line || [[ -n "$line" ]]; do
        line="${line%%#*}"
        while IFS= read -r term || [[ -n "$term" ]]; do
            term="${term#"${term%%[![:space:]]*}"}"
            term="${term%"${term##*[![:space:]]}"}"
            [[ -n "$term" ]] && terms+=("$term")
        done < <(printf '%s\n' "$line" | tr ',' '\n')
    done
}

if [[ -n "${SIPX_DENYLIST:-}" ]]; then
    add_terms <<<"$SIPX_DENYLIST"
fi

denylist_file="${SIPX_DENYLIST_FILE:-$HOME/notes/sipx-research/denylist.txt}"
if [[ ${#terms[@]} -eq 0 && -r "$denylist_file" ]]; then
    add_terms <"$denylist_file"
fi

if [[ ${#terms[@]} -eq 0 ]]; then
    if [[ -n "${CI:-}" ]]; then
        echo "provenance: FAIL — no denylist available." >&2
        echo "  Set the SIPX_DENYLIST secret in the CI environment. An unconfigured gate" >&2
        echo "  that passes is worse than no gate at all." >&2
        exit 2
    fi
    echo "provenance: skipped — no denylist configured locally."
    echo "  Set SIPX_DENYLIST, or create $denylist_file, to enable this check."
    exit 0
fi

# ----------------------------------------------------------- apply the carve-out ----
allow_file="$repo_root/scripts/provenance-allow.txt"
allowed=()
if [[ -r "$allow_file" ]]; then
    while IFS= read -r line || [[ -n "$line" ]]; do
        line="${line%%#*}"
        line="${line#"${line%%[![:space:]]*}"}"
        line="${line%"${line##*[![:space:]]}"}"
        [[ -n "$line" ]] && allowed+=("$line")
    done <"$allow_file"
fi

kept=()
carved=0
for term in "${terms[@]}"; do
    skip=0
    for ok in "${allowed[@]:-}"; do
        # Case-insensitive exact match: the allowlist names a system, not a substring rule. A
        # prefix match here would let one allowed name silently permit a family of denied ones.
        if [[ "${term,,}" == "${ok,,}" ]]; then
            skip=1
            break
        fi
    done
    if [[ $skip -eq 1 ]]; then
        carved=$((carved + 1))
    else
        kept+=("$term")
    fi
done

if [[ ${#kept[@]} -eq 0 ]]; then
    echo "provenance: FAIL — every denylist term is carved out as an integration target." >&2
    echo "  That leaves nothing checked. Narrow scripts/provenance-allow.txt." >&2
    exit 2
fi

# ------------------------------------------------------------------------- scanning ----
# One alternation pattern, so each corpus is scanned in a single pass.
pattern="$(printf '%s\n' "${kept[@]}" | paste -sd '|' -)"

status=0

# Tracked files only: untracked scratch is ignored by design.
if git rev-parse --git-dir >/dev/null 2>&1 && [[ -n "$(git ls-files)" ]]; then
    files_hit="$(git grep -I -n -i -E "$pattern" -- . \
        ':!scripts/check-provenance.sh' ':!scripts/provenance-allow.txt' || true)"
    if [[ -n "$files_hit" ]]; then
        echo "provenance: FAIL — prior-art reference in tracked files:" >&2
        printf '%s\n' "$files_hit" >&2
        echo >&2
        echo "  If this names an integration or interop target rather than a precedent, add it" >&2
        echo "  to scripts/provenance-allow.txt with a reason. If it is rationale, rewrite it to" >&2
        echo "  cite an RFC or a spec in docs/specs/." >&2
        status=1
    fi
else
    files_hit="$(grep -rInE "$pattern" . \
        --exclude-dir=.git --exclude-dir=target --exclude-dir=node_modules --exclude-dir=notes \
        --exclude='check-provenance.sh' --exclude='provenance-allow.txt' || true)"
    if [[ -n "$files_hit" ]]; then
        echo "provenance: FAIL — prior-art reference found:" >&2
        printf '%s\n' "$files_hit" >&2
        status=1
    fi
fi

if [[ $scan_history -eq 1 ]] && git rev-parse HEAD >/dev/null 2>&1; then
    log_hit="$(git log --format='%H%n%an%n%ae%n%s%n%b' | grep -inE "$pattern" || true)"
    if [[ -n "$log_hit" ]]; then
        echo "provenance: FAIL — prior-art reference in commit history:" >&2
        printf '%s\n' "$log_hit" >&2
        echo "  History must be rewritten before this repository is published." >&2
        status=1
    fi
fi

if [[ $status -eq 0 ]]; then
    echo "provenance: clean (${#kept[@]} terms checked, ${carved} carved out as integration targets$([[ $scan_history -eq 1 ]] && echo ', including history'))"
fi

exit $status
