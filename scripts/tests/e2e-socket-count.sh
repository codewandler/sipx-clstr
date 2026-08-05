#!/usr/bin/env bash
# CF-20's isolated fixture for the e2e node-socket observation. The real proof uses the runner's
# `ss`; this injects deterministic output so missing/error/unreadable/zero/one/two cannot depend on
# whichever sockets CI happens to own.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
checker="$repo_root/scripts/check-node-udp-sockets.sh"
fixture_root="$(mktemp -d)"
trap 'rm -rf "$fixture_root"' EXIT

fail() {
    echo "e2e-socket-count: FAIL — $1" >&2
    exit 1
}

put_ss() {
    local body="$1"
    mkdir -p "$fixture_root/bin"
    printf '%s\n' '#!/usr/bin/env bash' "$body" > "$fixture_root/bin/ss"
    chmod +x "$fixture_root/bin/ss"
}

rejects() {
    local label="$1"
    local output
    if output="$(SIPX_CLSTR_SS="$fixture_root/bin/ss" "$checker" 4242 2>&1)"; then
        fail "$label passed: $output"
    fi
    [[ "$output" != *"exactly one observable UDP socket"* ]] \
        || fail "$label printed the success claim: $output"
}

rm -rf "$fixture_root/bin"
rejects "missing ss"

put_ss 'exit 9'
rejects "ss failure"

put_ss 'printf "%s\\n" "UNCONN 0 0 127.0.0.1:5060 0.0.0.0:*"'
rejects "unreadable ownership"

put_ss 'printf "%s\\n" "UNCONN 0 0 127.0.0.1:9999 0.0.0.0:* users:((\\\"other\\\",pid=99,fd=3))"'
rejects "zero owned sockets"

put_ss 'printf "%s\\n" "UNCONN 0 0 127.0.0.1:5060 0.0.0.0:* users:((\\\"sipx-clstr\\\",pid=4242,fd=8))"'
one="$(SIPX_CLSTR_SS="$fixture_root/bin/ss" "$checker" 4242 2>&1)" \
    || fail "one owned socket failed: $one"
[[ "$one" == *"exactly one observable UDP socket"* ]] \
    || fail "one owned socket omitted the success claim: $one"

put_ss 'printf "%s\\n" "UNCONN 0 0 127.0.0.1:5060 0.0.0.0:* users:((\\\"sipx-clstr\\\",pid=4242,fd=8))" "UNCONN 0 0 127.0.0.1:20000 0.0.0.0:* users:((\\\"sipx-clstr\\\",pid=4242,fd=9))"'
rejects "two owned sockets"

echo "e2e-socket-count: 5 rejection cases and the exact-one success case passed"
