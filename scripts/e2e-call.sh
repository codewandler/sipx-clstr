#!/usr/bin/env bash
#
# M1's exit proof: two real `sipx` CLI phones register through one sipx-clstr node, call each other
# through it, and hang up — with media flowing **directly between the phones**.
#
# Real sockets, real UDP, a real independent implementation of the client side. Everything the
# deterministic harness cannot tell you: that the listener binds, that the parser agrees with someone
# else's serializer, that a registration made by one process is found by another.
#
# Usage:  scripts/e2e-call.sh [--sipx <path to the sipx CLI>] [--port <node port>]
#
# Exit codes: 0 the call completed with audio · 1 a step failed · 2 the environment is not ready.
#
# The `sipx` CLI is not vendored: it is the kernel's own phone, built from the sipx checkout. Pass
# `--sipx`, or set SIPX, or have it on PATH.
#
# not-in-ci: needs the external `sipx` CLI, built from a separate repository at a version this one
# does not pin, plus two real UDP sockets and RTP between them. Vendoring or building it here would
# destroy the property that makes this a proof — that the far end is an *independent*
# implementation of the client side — so it is run by hand before a release rather than per push.
# `DX-12` decided this deliberately rather than leaving it an accident: what a runner *can* check is
# checked, by `scripts/check-proof-domains.py` (the domains it registers in are ones its own document
# serves) and by `scripts/check-site.py` (the page that offers it still points at a file that exists).

set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

sipx="${SIPX:-}"
port=5060

while [[ $# -gt 0 ]]; do
    case "$1" in
        --sipx) sipx="${2:-}"; shift 2 ;;
        --port) port="${2:-}"; shift 2 ;;
        -h|--help) sed -n '2,20p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "unknown option: $1" >&2; exit 2 ;;
    esac
done

if [[ -z "$sipx" ]]; then
    sipx="$(command -v sipx || true)"
fi
if [[ -z "$sipx" || ! -x "$sipx" ]]; then
    echo "e2e-call: no sipx CLI." >&2
    echo "  Build it from the sipx checkout (cargo build --bin sipx) and pass --sipx <path>," >&2
    echo "  or set SIPX. It is the kernel's own phone and is deliberately not vendored here:" >&2
    echo "  the point of this test is that the client side is an independent implementation." >&2
    exit 2
fi

work="$(mktemp -d)"
node_pid=""
answer_pid=""

cleanup() {
    [[ -n "$answer_pid" ]] && kill "$answer_pid" 2>/dev/null
    [[ -n "$node_pid" ]] && kill "$node_pid" 2>/dev/null
    rm -rf "$work"
}
trap cleanup EXIT

fail() {
    echo "e2e-call: FAIL — $1" >&2
    [[ -s "$work/node.log" ]] && { echo "--- node log ---" >&2; tail -20 "$work/node.log" >&2; }
    exit 1
}

step() { printf '\n\033[1m── %s\033[0m\n' "$1"; }

# ------------------------------------------------------------------------------- the node ------

step "build"
cargo build --quiet --bin sipx-clstr || fail "the node does not build"

step "start the node on 127.0.0.1:$port"
# Configured by a document, not by flags: `DP-10` made the schema the only configuration surface, and
# a proof script still passing `--listen` would be testing an interface that no longer exists.
cat > "$work/cluster.yaml" <<YAML
apiVersion: sipx.dev/v1alpha1
version: 1
cluster:
  name: e2e
  environment: dev
  zones: [a]
  listener:
    - roles: [edge, registrar]
      transport: udp
      bind: 127.0.0.1:$port
      advertise: 127.0.0.1:$port
  membership:
    - node: 1
      name: node-a
      zone: a
      roles: [edge, registrar]
  locationStore:
    backend: memory
  tenant:
    - name: default
      id: 1
      domains: [127.0.0.1]
YAML
setsid ./target/debug/sipx-clstr run --config "$work/cluster.yaml" \
    --node 1 --zone a --roles edge,registrar \
    >"$work/node.log" 2>&1 </dev/null &
node_pid=$!

# Wait for the line the node prints once it is bound, rather than sleeping and hoping.
for _ in $(seq 1 50); do
    grep -q "listening on" "$work/node.log" 2>/dev/null && break
    sleep 0.1
done
grep -q "listening on" "$work/node.log" 2>/dev/null || fail "the node never bound $port"
echo "  $(head -1 "$work/node.log")"

# ------------------------------------------------------------------------------ the phones -----

# A three-second 440 Hz tone. Audio is how this script proves media went *direct*: the node runs no
# relay of any kind, so anything bob hears travelled straight from alice.
python3 - "$work/tone.wav" <<'PY' || fail "could not write the test tone"
import math, struct, sys, wave
with wave.open(sys.argv[1], "w") as w:
    w.setnchannels(1); w.setsampwidth(2); w.setframerate(8000)
    w.writeframes(b"".join(struct.pack("<h", int(12000 * math.sin(2 * math.pi * 440 * i / 8000)))
                           for i in range(8000 * 3)))
PY

step "register both phones"
for phone in bob:15081 alice:15071; do
    name="${phone%%:*}"; local_port="${phone##*:}"
    out="$(timeout 15 "$sipx" register "sip:$name@127.0.0.1" \
        --local "127.0.0.1:$local_port" --json 2>&1)" \
        || fail "$name could not register: $out"
    grep -q '"status":"registered"' <<<"$out" || fail "$name did not register: $out"
    echo "  $name: $out"
done

step "place the call"
setsid timeout 40 "$sipx" answer --local 127.0.0.1:15081 \
    --duration 4 --wait 30 --record "$work/heard.wav" --json \
    >"$work/bob.json" 2>"$work/bob.err" </dev/null &
answer_pid=$!
sleep 1

dial="$(timeout 40 "$sipx" dial sip:bob@127.0.0.1 --local 127.0.0.1:15071 \
    --duration 4 --timeout 20 --play "$work/tone.wav" --stats --json 2>&1)"
echo "  alice: $dial"
grep -q '"status":"answered"' <<<"$dial" || fail "the call was not answered: $dial"

# Let bob finish writing its result.
for _ in $(seq 1 60); do
    grep -q '"status":"answered"' "$work/bob.json" 2>/dev/null && break
    sleep 0.2
done

bob="$(grep '"status":"answered"' "$work/bob.json" 2>/dev/null || true)"
[[ -n "$bob" ]] || fail "bob never reported an answered call: $(cat "$work/bob.json" "$work/bob.err" 2>/dev/null)"
echo "  bob:   $bob"

# --------------------------------------------------------------------------- the assertions ----

step "assert"

grep -q '"heard_audio":true' <<<"$bob" \
    || fail "bob heard no audio — media did not reach him"
echo "  ✓ bob heard audio"

samples="$(sed -n 's/.*"samples_recorded":\([0-9]*\).*/\1/p' <<<"$bob")"
[[ -n "$samples" && "$samples" -gt 8000 ]] \
    || fail "bob recorded only ${samples:-0} samples; expected more than a second's worth"
echo "  ✓ bob recorded $samples samples"

[[ -s "$work/heard.wav" ]] || fail "the recording is empty"
echo "  ✓ the recording is $(stat -c%s "$work/heard.wav") bytes"

# **Media went direct.** The node runs no relay — there is no `MediaRelay` in M1 at all — so RTP that
# reached bob came from alice's socket and nowhere else. Asserted here as the absence of the thing
# that would contradict it: if the node had touched media, the node would have had to open a media
# port, and it opens exactly one socket.
sockets="$(ss -lunp 2>/dev/null | grep -c "pid=$node_pid," || true)"
if [[ "$sockets" -gt 1 ]]; then
    fail "the node holds $sockets UDP sockets; M1's node forwards signalling only"
fi
echo "  ✓ the node holds one socket — signalling only, so the media was direct"

# A proxy that leaks one transaction per call is a slow, quiet outage, so the node logs how much
# state the kernel still holds for it and this waits for that to reach zero.
#
# **It takes about half a minute, and that is correct.** RFC 3261 keeps a concluded transaction alive
# for its absorption timer — 64·T1, thirty-two seconds — so that a retransmission arriving after the
# final response is answered from the transaction rather than delivered to the application a second
# time. Asserting "empty immediately" would be asserting a bug. What this catches is the count that
# never returns to zero at all.
step "wait for the transaction store to drain (RFC 3261's 64·T1 absorption window)"
drained=""
for _ in $(seq 1 100); do
    last="$(grep -o 'outstanding=[0-9]*' "$work/node.log" | tail -1)"
    if [[ "$last" == "outstanding=0" ]]; then drained="yes"; break; fi
    sleep 0.5
done
[[ -n "$drained" ]] \
    || fail "the node still reports ${last:-nothing} after 50s — a leaked transaction"
echo "  ✓ the node holds no transactions afterwards"

printf '\n\033[1;32me2e-call: the call completed, with media flowing directly between the phones\033[0m\n'
