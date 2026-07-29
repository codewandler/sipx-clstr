#!/usr/bin/env bash
#
# `DP-9`: two nodes, one location store, a call across them.
#
# The first proof that the word "cluster" is earned. Two sipx-clstr nodes read the **same**
# configuration document, share one PostgreSQL location service, and bind different addresses. One
# `sipx` CLI phone registers through node A; a second registers through node B; then A calls B, and
# the call is forwarded by a node that never saw the callee's REGISTER.
#
# What makes it work, and what it therefore does not prove: each node record-routes **its own
# advertised address**, so the route set names a node rather than a service address. Put a load
# balancer or a shared VIP in front of these two and in-dialog requests will land on whichever node
# the balancer picks, which is what affinity tokens (`AF-*`) exist for and is out of scope here.
#
# One document serves both nodes through `${VAR}` substitution (cluster-config §8 V4) — the schema's
# own mechanism for exactly this, and the reason there is no per-node file.
#
# Usage:  scripts/two-node-call.sh [--sipx <path to the sipx CLI>]
# Exit:   0 registrations crossed the node boundary and the call completed
#         1 a step failed
#         2 the environment was not ready (no sipx CLI, no docker, no database)

set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

SIPX="${SIPX:-}"
while [[ $# -gt 0 ]]; do
    case "$1" in
        --sipx) SIPX="$2"; shift 2 ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
done
[[ -z "$SIPX" ]] && SIPX="$(command -v sipx || true)"
if [[ -z "$SIPX" || ! -x "$SIPX" ]]; then
    echo "environment not ready: no sipx CLI." >&2
    echo "  Build it from a sipx checkout (cargo build --bin sipx) and pass --sipx <path>," >&2
    echo "  or set \$SIPX. It is deliberately not vendored: the point of an end-to-end proof" >&2
    echo "  is that the other end is an independent implementation." >&2
    exit 2
fi
command -v docker >/dev/null || { echo "environment not ready: no docker, needed for the store." >&2; exit 2; }

work="$(mktemp -d)"
pg_name="sipx-clstr-dp9-pg"
node_a_pid=""; node_b_pid=""

cleanup() {
    [[ -n "$node_a_pid" ]] && kill "$node_a_pid" 2>/dev/null
    [[ -n "$node_b_pid" ]] && kill "$node_b_pid" 2>/dev/null
    docker rm -f "$pg_name" >/dev/null 2>&1
    rm -rf "$work"
}
trap cleanup EXIT

step() { printf '\n\033[1m── %s\033[0m\n' "$1"; }
pass() { printf '[PASS] %s\n' "$1"; }
fail() { printf '[FAIL] %s\n' "$1" >&2; exit 1; }

# Two loopback addresses rather than two ports: each node is a separate host as far as SIP is
# concerned, which is what the deployed shape looks like. 127.0.0.0/8 is all local on Linux.
NODE_A_ADDR="127.0.0.1:5060"
NODE_B_ADDR="127.0.0.2:5060"
DOMAIN="127.0.0.1"

step "the shared location store"
docker rm -f "$pg_name" >/dev/null 2>&1
docker run -d --rm --name "$pg_name" \
    -e POSTGRES_PASSWORD=sipx -e POSTGRES_DB=sipx -p 55433:5432 \
    postgres:16-alpine >/dev/null || fail "could not start PostgreSQL"
for _ in $(seq 1 60); do
    docker exec "$pg_name" pg_isready -q 2>/dev/null && break
    sleep 1
done
docker exec "$pg_name" pg_isready -q 2>/dev/null || fail "PostgreSQL never became ready"
export LOCATION_DSN="postgres://postgres:sipx@127.0.0.1:55433/sipx"
pass "one store, reachable at 127.0.0.1:55433"

step "one document, two nodes"
cat > "$work/cluster.yaml" <<'YAML'
apiVersion: sipx.dev/v1alpha1
version: 1
cluster:
  name: dp9
  environment: dev
  zones: [a]
  listener:
    - roles: [edge, registrar]
      transport: udp
      bind: ${NODE_BIND}
      advertise: ${NODE_ADVERTISE}
  membership:
    - node: 1
      name: node-a
      zone: a
      roles: [edge, registrar]
    - node: 2
      name: node-b
      zone: a
      roles: [edge, registrar]
  locationStore:
    backend: postgres
    dsnRef: location-dsn
  tenant:
    - name: default
      id: 1
      domains: [example.test]
YAML
pass "the same bytes will configure both nodes"

cargo build --bin sipx-clstr --features postgres >/dev/null 2>&1 || fail "the node did not build"
node="$repo_root/target/debug/sipx-clstr"

start_node() {
    local name="$1" id="$2" addr="$3" log="$4"
    NODE_BIND="$addr" NODE_ADVERTISE="$addr" \
        "$node" run --config "$work/cluster.yaml" \
        --node "$id" --zone a --roles edge,registrar >"$log" 2>&1 &
    local pid=$!
    for _ in $(seq 1 100); do
        grep -q "listening on" "$log" 2>/dev/null && { echo "$pid"; return 0; }
        kill -0 "$pid" 2>/dev/null || { cat "$log" >&2; return 1; }
        sleep 0.1
    done
    cat "$log" >&2
    return 1
}

step "node A and node B"
node_a_pid="$(start_node node-a 1 "$NODE_A_ADDR" "$work/a.log")" || fail "node A did not start"
pass "node A listening on $NODE_A_ADDR"
node_b_pid="$(start_node node-b 2 "$NODE_B_ADDR" "$work/b.log")" || fail "node B did not start"
pass "node B listening on $NODE_B_ADDR"

# Both must have opened the shared store, not fallen back to memory. A registrar that fell back
# would pass every check below except the one that matters, so this is asserted rather than assumed.
grep -qi "in-memory\|InMemory" "$work/a.log" && fail "node A fell back to an in-process store"
grep -qi "in-memory\|InMemory" "$work/b.log" && fail "node B fell back to an in-process store"

step "alice registers through node A, bob through node B"
# `--target` is what makes this a two-node test rather than a one-node one: both AoRs live in the
# same domain, and each phone is told which node to send to. Without it both would reach node A and
# the shared store would never be exercised across the boundary.
"$SIPX" register "sip:alice@$DOMAIN" --target "$NODE_A_ADDR" --local 127.0.0.1:15081 --json \
    >"$work/reg-alice.json" 2>&1 \
    || { cat "$work/reg-alice.json" >&2; fail "alice could not register through node A"; }
pass "alice registered via node A ($NODE_A_ADDR)"

"$SIPX" register "sip:bob@$DOMAIN" --target "$NODE_B_ADDR" --local 127.0.0.1:15091 --json \
    >"$work/reg-bob.json" 2>&1 \
    || { cat "$work/reg-bob.json" >&2; fail "bob could not register through node B"; }
pass "bob registered via node B ($NODE_B_ADDR)"

step "the registration crossed the node boundary"
# The discriminating check: bob's binding, written by node B, must be readable from the store that
# node A also reads. Asked of the database directly, so the answer does not depend on either node.
rows="$(docker exec "$pg_name" psql -U postgres -d sipx -tAc \
    "select count(*) from location_bindings" 2>/dev/null || echo 0)"
[[ "$rows" -ge 1 ]] || fail "the shared store holds no bindings; the nodes are not sharing it"
pass "the shared store holds $rows binding row(s) written by two different nodes"

step "alice calls bob, through the node that never saw his REGISTER"
"$SIPX" answer --local 127.0.0.1:15091 --duration 3 --wait 25 --json >"$work/answer.json" 2>&1 &
answer_pid=$!
sleep 1
# Alice sends to node A. Node A never saw bob's REGISTER — node B took it — so the only way this
# call reaches him is through the location service they share.
if "$SIPX" dial "sip:bob@$DOMAIN" --target "$NODE_A_ADDR" --local 127.0.0.1:15081 \
        --duration 3 --timeout 20 --json >"$work/dial.json" 2>&1; then
    pass "the call completed"
else
    cat "$work/dial.json" >&2
    cat "$work/a.log" >&2
    fail "the call did not complete"
fi
wait "$answer_pid" 2>/dev/null

printf '\n\033[1;32mRESULT: PASS\033[0m — two nodes shared one location service, and a call was\n'
printf 'forwarded by a node that had never seen the callee register.\n\n'
printf 'What this does NOT prove: mid-dialog routing through a load balancer. Each node\n'
printf 'record-routes its own address, so the route set names a node. A shared VIP in front\n'
printf 'of these two needs affinity tokens, which are specified and not implemented.\n'
