#!/usr/bin/env bash
#
# M1's exit proof: two real `sipx` CLI phones register through one sipx-clstr node, call each other
# through it, and hang up — with media flowing **directly between the phones**.
#
# Real sockets, real UDP, a separate process on the client side — a same-kernel, separate-process
# integration test. Everything the deterministic harness cannot tell you: that the listener binds,
# that a registration made by one process is found by another, that media flows. Not what an
# independent implementation would tell you: the phone is built from the kernel this repo pins, so a
# parser disagreement shared by both ends passes this unnoticed. That target is `CF-3`'s.
#
# Usage:  scripts/e2e-call.sh [--sipx <path to the sipx CLI>] [--port <node port>]
#
# Exit codes: 0 the call completed with audio · 1 a step failed · 2 the environment is not ready.
#
# The `sipx` CLI is not vendored: it is the kernel's own phone, built from the sipx checkout. Pass
# `--sipx`, or set SIPX, or have it on PATH.
#
# **One run at a time, per machine** — see the note above `preflight` for why that is a constraint
# rather than an omission, and why it does not make the CI job flaky.
#
# **This runs in CI** — `.github/workflows/ci.yml`, the `e2e` job, which builds the `sipx` CLI from
# the kernel tag this workspace pins and then runs exactly this script. It is deliberately not in
# `scripts/gate.sh`: the gate must stay runnable without a second checkout. `CF-15` wired it in after
# `DX-12` recorded the opposite decision, because the reason for that decision — that the CLI comes
# from another repository — turned out to cost about forty seconds of `cargo build`, while the cost of
# *not* running it was `FC-4` breaking this proof for a whole release with nothing watching.

set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

sipx="${SIPX:-}"
host=127.0.0.1
port=5060

while [[ $# -gt 0 ]]; do
    case "$1" in
        --sipx) sipx="${2:-}"; shift 2 ;;
        --port) port="${2:-}"; shift 2 ;;
        -h|--help) sed -n '2,19p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
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
    echo "  the point of this test is that the client side is a separate process on a real socket." >&2
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

# --------------------------------------------------------------------------- where it listens ---

# **This proof runs one at a time per machine, and that is a constraint rather than an oversight**
# (`CF-15`, settling the residue `CF-13` left here).
#
# `CF-13` gave the *phones* ephemeral ports, and the node kept `127.0.0.1:5060`. The obvious next
# step — make the node's address or port unique per run, so two runs never collide — is closed off
# from three directions at once, and it is worth writing down so the next reader does not spend the
# afternoon rediscovering it:
#
#   1. The **port** cannot vary. `sipx dial` derives the callee's transport address from the
#      request-URI, and location-service §3.2 N7 makes an absent port and an explicit one *distinct*
#      AoR keys (RFC 3261 §19.1.4: "a URI omitting any component with a default value will not match
#      a URI explicitly containing that component with its default value"). A node on `:41337` could
#      only be dialled as `sip:bob@127.0.0.1:41337`, which is not the AoR bob registered under.
#   2. The **address** cannot vary either, for a second reason that only shows up once you try it.
#      `sipx register` takes `--target`, so registration could be aimed at any address while the AoR
#      stayed literal — but `sipx dial` has no `--target`, so the call leg must reach the node
#      through the request-URI, which drags the *domain* along with the address.
#   3. And a runtime domain is refused on purpose. `scripts/check-proof-domains.py` (`FC-5`) requires
#      the domain a proof registers in to be a **static literal** it can compare against the tenant
#      document — "decided at runtime is not statically provable, and is refused rather than
#      skipped". That check is why `FC-4`'s `403`s cannot happen again silently, and it is worth
#      more than parallel local runs of this script.
#
# So the node's address stays literal, and concurrency is handled by refusing to start rather than by
# failing halfway through. **The CI job is unaffected**: `.github/workflows/ci.yml` runs `e2e` on its
# own `ubuntu-latest` VM, one run per machine, with nothing else on it binding `5060` — the collision
# CF-13 warned about is a developer's second terminal, not a CI condition. What this turns that
# developer's confusing mid-run failure into is an immediate exit 2, "the environment is not ready",
# which is the contract the header already declares.
preflight() {
    python3 - "$host" "$port" <<'PY'
import socket, sys
probe = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
try:
    probe.bind((sys.argv[1], int(sys.argv[2])))
except OSError as error:
    print(error, file=sys.stderr)
    sys.exit(1)
finally:
    probe.close()
PY
}

if ! held="$(preflight 2>&1)"; then
    echo "e2e-call: $host:$port is not available — $held" >&2
    echo "  Something already holds it: another run of this script, a previous node that did not" >&2
    echo "  exit, or a real SIP service. This proof needs that exact address (see the note above" >&2
    echo "  \`preflight\`), so it stops here rather than failing halfway through." >&2
    exit 2
fi

# ------------------------------------------------------------------------------- the node ------

step "build"
cargo build --quiet --bin sipx-clstr || fail "the node does not build"

step "start the node on $host:$port"
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
      bind: $host:$port
      advertise: $host:$port
  membership:
    - node: 1
      name: node-a
      zone: a
      roles: [edge, registrar]
      # cluster-membership MB5: a member on the call path owns flows, so it declares where a peer
      # dials it for the connection-owner RPC. Nothing in this build dials one — AF-3/AF-7 own that
      # — but the field is required of every such member and the node reports it as unapplied.
      rpc: 127.0.0.1:7223
  locationStore:
    backend: memory
  tenant:
    - name: default
      id: 1
      # A literal, deliberately: `scripts/check-proof-domains.py` compares this against the
      # address-of-record the phones register below, and only a literal can be compared before the
      # script runs. See the note above `preflight`.
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
grep -q "listening on" "$work/node.log" 2>/dev/null || fail "the node never bound $host:$port"
echo "  $(head -1 "$work/node.log")"

# ------------------------------------------------------------------------------ the phones -----

# The phones' ports are **asked for, not chosen** (`CF-13`). They used to be `15081` and `15071`,
# which are also two of the numbers the node crate's integration suite used to bind, so running this
# script while the suite ran produced `Address already in use` in whichever of the two was slower —
# and the failure landed on whatever diff happened to be under test. The suite no longer picks any
# port; neither does this.
#
# Each phone needs a *stable* port, so `--local 127.0.0.1:0` is not enough on its own: `register`
# publishes a contact and `answer` has to be listening on the port that contact names, and those are
# two separate processes. So the kernel is asked which port is free and that answer is used for both.
#
# The **node's** port stays fixed, for the N7 reason set out above; its *address* is what makes the
# run unique. These are on that same address, so the two mechanisms do not have to agree about
# anything beyond `$host`.
free_port() {
    python3 - "$host" <<'PY'
import socket, sys
s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
s.bind((sys.argv[1], 0))
print(s.getsockname()[1])
s.close()
PY
}

alice_port="$(free_port)" || fail "could not find a free port for alice"
bob_port="$(free_port)" || fail "could not find a free port for bob"
[[ -n "$alice_port" && -n "$bob_port" && "$alice_port" != "$bob_port" ]] \
    || fail "could not find two distinct free ports (alice=$alice_port bob=$bob_port)"

# A three-second 440 Hz tone. Audio is how this script proves media went *direct*: the node runs no
# relay of any kind, so anything bob hears travelled straight from alice.
python3 - "$work/tone.wav" <<'PY' || fail "could not write the test tone"
import math, struct, sys, wave
with wave.open(sys.argv[1], "w") as w:
    w.setnchannels(1); w.setsampwidth(2); w.setframerate(8000)
    w.writeframes(b"".join(struct.pack("<h", int(12000 * math.sin(2 * math.pi * 440 * i / 8000)))
                           for i in range(8000 * 3)))
PY

step "register both phones (alice on $alice_port, bob on $bob_port)"
for phone in "bob:$bob_port" "alice:$alice_port"; do
    name="${phone%%:*}"; local_port="${phone##*:}"
    out="$(timeout 15 "$sipx" register "sip:$name@$host" \
        --local "$host:$local_port" --json 2>&1)" \
        || fail "$name could not register: $out"
    grep -q '"status":"registered"' <<<"$out" || fail "$name did not register: $out"
    echo "  $name: $out"
done

step "place the call"
setsid timeout 40 "$sipx" answer --local "$host:$bob_port" \
    --duration 4 --wait 30 --record "$work/heard.wav" --json \
    >"$work/bob.json" 2>"$work/bob.err" </dev/null &
answer_pid=$!
sleep 1

dial="$(timeout 40 "$sipx" dial "sip:bob@$host" --local "$host:$alice_port" \
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

# The socket observation is deliberately narrower than the media claim. `ss` can establish only
# what this process owns at one inspection instant, and only when process ownership is readable. It
# cannot reconstruct the path of every packet. Combined with both phones' audio results and the
# absence of a relay implementation, exactly one node-owned UDP socket is evidence consistent with
# direct media; zero, ambiguity, and extra sockets are all failures rather than flattering silence.
if ! socket_observation="$(scripts/check-node-udp-sockets.sh "$node_pid" 2>&1)"; then
    fail "$socket_observation"
fi
echo "  ✓ $socket_observation"

# A proxy that holds a transaction no timer will collect is a slow, quiet outage, so the node logs
# how much state the kernel still holds for it and this waits for that to reach zero.
#
# **It takes up to a minute and that is correct — and this waited fifty seconds, which was wrong.**
# `CF-22`, after `PX-13` was merged, failed here, reverted, and then found to have been right all
# along. The arithmetic that mattered:
#
#   - RFC 3261 §17 holds a concluded transaction for its absorption timer, 64·T1 = 32 s at the
#     default T1, so that a retransmission arriving after the final response is answered from the
#     transaction rather than delivered to the application a second time. Asserting "empty
#     immediately" would be asserting a bug — that much the old comment had right.
#   - But this is a **proxy**, and one absorption window is not the worst case for a proxied
#     request whose next hop says nothing. §17.1.2.2 gives the client transaction Timer F (or B)
#     = 64·T1, and §16.8 confines Timer C to INVITE so nothing concludes a non-INVITE branch
#     sooner. §16.7 requires the *best* final response, which does not exist until that branch
#     concludes, and §17.2.2 gives a server transaction in Trying **no timer at all** — so the
#     server transaction's own 64·T1 (Timer J, or H) starts only once the first window has already
#     elapsed. **128·T1 = 64 s**, and every second of it is the RFC's.
#   - `seq 1 100` × `sleep 0.5` is 50 s: strictly between one window and two. A dialog whose far
#     end had gone away — `sipx dial` exits when its `--duration` elapses, so this script produces
#     exactly that — drained at 64.000 s and was reported as a leak. It was not one, and the
#     revert cost a release cycle.
#
# So the budget is past 128·T1, with room for the node's 500 ms sampling tick and for a loaded
# runner. What this catches is what it always should have: a count that does not come back to zero.
# `crates/sipx-clstr-sim/tests/transaction_drain.rs` is the same assertion in virtual time, where
# the same 128·T1 is derived rather than guessed, and it is in `scripts/gate.sh`.
drain_budget_s=100
step "wait for the transaction store to drain (RFC 3261's worst case is 128·T1 = 64s; waiting ${drain_budget_s}s)"
drained=""
for _ in $(seq 1 $((drain_budget_s * 2))); do
    last="$(grep -o 'outstanding=[0-9]*' "$work/node.log" | tail -1)"
    if [[ "$last" == "outstanding=0" ]]; then drained="yes"; break; fi
    sleep 0.5
done
[[ -n "$drained" ]] \
    || fail "the node still reports ${last:-nothing} ${drain_budget_s}s after the call — past
  RFC 3261's own worst case of 128·T1 (64s), so something is held that no §17 timer will collect"
echo "  ✓ the node holds no transactions afterwards"

printf '\n\033[1;32me2e-call: the call completed, with media flowing directly between the phones\033[0m\n'
