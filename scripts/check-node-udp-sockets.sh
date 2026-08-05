#!/usr/bin/env bash
# Establish the one fact CF-20 permits the e2e proof to claim: at inspection time, `ss` can see
# exactly one UDP socket owned by the node PID. Fail closed when the tool or ownership metadata is
# unavailable; an absence of evidence is not evidence of one signalling-only socket.
set -uo pipefail

node_pid="${1:-}"
ss_command="${SIPX_CLSTR_SS:-ss}"

fail() {
    echo "node-udp-sockets: FAIL — $1" >&2
    exit 1
}

[[ "$node_pid" =~ ^[1-9][0-9]*$ ]] || fail "expected a positive node PID"
if [[ "$ss_command" == */* ]]; then
    [[ -x "$ss_command" ]] \
        || fail "ss is unavailable; UDP socket ownership cannot be inspected"
elif ! command -v "$ss_command" >/dev/null 2>&1; then
    fail "ss is unavailable; UDP socket ownership cannot be inspected"
fi

if ! listing="$("$ss_command" -H -lunp 2>&1)"; then
    fail "ss could not inspect UDP process ownership"
fi

# An ordinary unprivileged `ss` can list sockets while hiding every process owner. Calling that
# zero would blur two different facts, so name the ambiguity and refuse it before counting.
if [[ -n "$listing" && "$listing" != *"pid="* ]]; then
    fail "ss listed UDP sockets but process ownership is unreadable"
fi

owned="$(grep -E -c "pid=${node_pid}([,)]|$)" <<<"$listing" || true)"
case "$owned" in
    1)
        echo "the node owns exactly one observable UDP socket at inspection time"
        ;;
    0)
        fail "ss observed zero UDP sockets owned by node PID $node_pid; expected exactly one"
        ;;
    *)
        fail "ss observed $owned UDP sockets owned by node PID $node_pid; expected exactly one"
        ;;
esac
