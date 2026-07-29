#!/usr/bin/env bash
#
# `DP-9` in a cluster: two nodes in Kubernetes, one location store, a call across them.
#
# The same proof as `scripts/two-node-call.sh`, run against the devspace profile instead of two local
# processes. Two pods read the **same** ConfigMap document, each advertising its own pod IP, both
# pointed at one PostgreSQL location service. Two `sipx` CLI phones register — one through each pod —
# and then one calls the other, forwarded by a pod that never saw the callee's REGISTER.
#
# The phones run inside the cluster, as a pod. They have to: this machine routes the pod CIDR
# 10.42.0.0/16 into a WireGuard interface, so a packet from the host to a pod is swallowed by the
# tunnel and never arrives. Pod-to-pod has no such ambiguity, and it is also the arrangement a real
# client is in. The `sipx` CLI is baked into a small image rather than built here — it is an
# independent implementation, and rebuilding it with this repo's tooling would blur the boundary the
# proof depends on.
#
# What this does not prove: mid-dialog routing through a single Service in front of both pods. Each
# node record-routes its own pod IP, so the route set names a pod. A shared ClusterIP would spread
# in-dialog requests across both, which is what affinity tokens exist for and is not implemented.
#
# Usage:  scripts/k8s-two-node-call.sh [--sipx <path>] [--namespace <ns>]
# Exit:   0 the cross-node call completed · 1 a step failed · 2 environment not ready

set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

NS="sipx-clstr-dev"
while [[ $# -gt 0 ]]; do
    case "$1" in
        --namespace) NS="$2"; shift 2 ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
done
command -v kubectl >/dev/null || { echo "environment not ready: no kubectl." >&2; exit 2; }
kubectl get ns "$NS" >/dev/null 2>&1 || { echo "environment not ready: namespace $NS is absent; deploy first." >&2; exit 2; }

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

step() { printf '\n\033[1m── %s\033[0m\n' "$1"; }
pass() { printf '[PASS] %s\n' "$1"; }
fail() { printf '[FAIL] %s\n' "$1" >&2; exit 1; }

step "the deployed cluster"
kubectl -n "$NS" rollout status deploy/sipx-clstr-node-a --timeout=120s >/dev/null 2>&1 \
    || fail "node-a is not ready"
kubectl -n "$NS" rollout status deploy/sipx-clstr-node-b --timeout=120s >/dev/null 2>&1 \
    || fail "node-b is not ready"
NODE_A="$(kubectl -n "$NS" get pod -l sipx-clstr/node=node-a -o jsonpath='{.items[0].status.podIP}')"
NODE_B="$(kubectl -n "$NS" get pod -l sipx-clstr/node=node-b -o jsonpath='{.items[0].status.podIP}')"
[[ -n "$NODE_A" && -n "$NODE_B" && "$NODE_A" != "$NODE_B" ]] \
    || fail "expected two distinct pod IPs, got '$NODE_A' and '$NODE_B'"
pass "node-a at $NODE_A, node-b at $NODE_B — two pods, two addresses"

# Both must be on the shared store. A node that fell back to its own memory would pass every check
# below except the one that matters, so it is read out of the log rather than assumed.
for node in node-a node-b; do
    kubectl -n "$NS" logs "deploy/sipx-clstr-$node" --tail=50 2>/dev/null \
        | grep -q 'store="postgres"' \
        || fail "$node is not using the shared location store"
done
pass "both nodes report store=\"postgres\" — one location service, not two"

# One document, not two. Read back from the cluster so the claim is about what is deployed.
kubectl -n "$NS" get configmap sipx-clstr-cluster -o jsonpath='{.data.cluster\.yaml}' >"$work/deployed.yaml" \
    || fail "no cluster document in the namespace"
grep -q 'advertise: \${POD_IP}:5060' "$work/deployed.yaml" \
    || fail "the deployed document does not use per-node substitution"
pass "one ConfigMap document, resolved per node through \${POD_IP}"

# Emptied first, so the count below is a statement about *this* run rather than about history.
kubectl -n "$NS" exec deploy/sipx-clstr-postgres -- \
    psql -U postgres -d sipx -qc "truncate location_bindings" >/dev/null 2>&1 || true

step "two phones, registering through different pods"
# The phones run **inside** the cluster, as a pod, not on the host. They have to: this machine routes
# the pod CIDR 10.42.0.0/16 into a WireGuard interface, so a packet from the host to a pod is
# swallowed by the tunnel and never arrives. Pod-to-pod has no such ambiguity, and it is also the
# arrangement a real client would be in.
kubectl -n "$NS" delete pod sipx-phones --ignore-not-found --wait=true >/dev/null 2>&1

# The address-of-record domain is node-a's address for *both* users, while bob is registered
# *through* node-b with `--target`. That is what makes this a cross-node test: bob's binding is
# written by node-b, and the INVITE that finds it is resolved by node-a. `sipx dial` takes no
# `--target` — the URI it is given is the destination — so the AoR domain has to be an address, and
# using one shared address keeps both AoRs in the same namespace for the lookup. No port in the
# AoR — `register` refuses one there, and `dial` defaults to 5060, which is where the node listens.
DOMAIN="$NODE_A"
cat > "$work/phones.sh" <<SCRIPT
set -u
ME="\$(hostname -i | awk '{print \$1}')"
echo "phones at \$ME"

sipx register "sip:alice@$DOMAIN" --target "$NODE_A:5060" --local "\$ME:15081" --json \
    || { echo "REGISTER-ALICE-FAILED"; exit 1; }
echo "ALICE-REGISTERED"

sipx register "sip:bob@$DOMAIN" --target "$NODE_B:5060" --local "\$ME:15091" --json \
    || { echo "REGISTER-BOB-FAILED"; exit 1; }
echo "BOB-REGISTERED"

sipx answer --local "\$ME:15091" --duration 4 --wait 25 --record /tmp/heard.wav --json &
sleep 1
# Alice sends to node-a, which never saw bob's REGISTER. The only route to him is the shared store.
sipx dial "sip:bob@$DOMAIN" --local "\$ME:15081" --duration 4 --timeout 20 --play /tone.wav --json || { echo "DIAL-FAILED"; exit 1; }
echo "CALL-COMPLETED"
wait
SCRIPT

kubectl -n "$NS" run sipx-phones --image=sipx-phone:dev --image-pull-policy=IfNotPresent \
    --restart=Never --attach --rm --quiet \
    --overrides='{"spec":{"containers":[{"name":"sipx-phones","image":"sipx-phone:dev","imagePullPolicy":"IfNotPresent","command":["sh","-s"],"stdin":true,"stdinOnce":true,"tty":false}]}}' \
    -i < "$work/phones.sh" > "$work/phones.log" 2>&1
cat "$work/phones.log"

grep -q "ALICE-REGISTERED" "$work/phones.log" || fail "alice could not register through node-a"
pass "alice registered through node-a"
grep -q "BOB-REGISTERED" "$work/phones.log" || fail "bob could not register through node-b"
pass "bob registered through node-b"

step "the registrations are in one store, not two"
rows="$(kubectl -n "$NS" exec deploy/sipx-clstr-postgres -- \
    psql -U postgres -d sipx -tAc "select count(*) from location_bindings" 2>/dev/null | tr -d '[:space:]')"
[[ "${rows:-0}" -ge 2 ]] \
    || fail "the shared store holds ${rows:-0} binding(s); two nodes are not sharing it"
pass "$rows bindings in one database, written by two different pods"

step "alice calls bob through the pod that never saw his REGISTER"
grep -q "CALL-COMPLETED" "$work/phones.log" || {
    kubectl -n "$NS" logs deploy/sipx-clstr-node-a --tail=25 >&2 2>&1
    fail "the cross-node call did not complete"
}
pass "the call completed"

# Media is asserted, not assumed: the callee must have actually heard the tone the caller played.
# Signalling that completes while no audio arrives is the failure this catches, and it is the same
# standard the single-node proof holds itself to.
grep -q '"heard_audio":true' "$work/phones.log" \
    || fail "the call completed but no audio arrived — signalling passed and media did not"
pass "the callee heard the tone the caller played — media flowed directly, no relay exists"

printf '\n\033[1;32mRESULT: PASS\033[0m — two Kubernetes pods, one configuration document, one\n'
printf 'location service, and a call forwarded by a pod that never saw the callee register.\n\n'
printf 'Not proved: a single Service in front of both. Each node record-routes its own pod IP,\n'
printf 'so in-dialog requests name a pod. Putting a ClusterIP there needs affinity tokens.\n'
