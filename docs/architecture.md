# sipx-clstr — architecture charts

Companion charts to the [vision](vision.md) and the epic [designs](designs/). Each chart names
the design doc that owns its decisions; when a chart and a design disagree, the design wins.

## 1. Cluster topology

The reference deployment ([deployment](designs/deployment.md)): one region, three zones, every
signalling node the same binary with roles from config. Media is its own pool, controlled — never
linked ([media-control](designs/media-control.md)) — whether the Kubernetes operator manages that
pool or it is external ([k8s-deployment-operator](designs/k8s-deployment-operator.md)). The
`e2e-tester` role is drawn outside the border on purpose: it enters through the public path like
any customer, because a probe that skips the front door cannot detect a broken front door
([e2e-tester](designs/e2e-tester.md)).

```text
  Public SIP — carriers / trunks / UAs     e2e-tester (probe role)
                    |                  dials the border on a schedule
                    |                   or when its API is triggered
                    |                                 |
                    +----------------+----------------+
                                     |
                     DNS NAPTR/SRV  +  L4 VIP (source-preserving)
                                     |
          +--------------------------+--------------------------+
          |                          |                          |
     SIP edge A                 SIP edge B                 SIP edge C
   UDP/TCP/TLS/WS/WSS         UDP/TCP/TLS/WS/WSS         UDP/TCP/TLS/WS/WSS
   owns its client conns      owns its client conns      owns its client conns
          |                          |                          |
          +----------- internal RPC (connection-owner) ---------+
          |                          |                          |
   +------+--------------------------+--------------------------+------+
   |                                 |                                 |
Registrar shards               Routing / policy                echo service
rendezvous(tenant‖AoR)         trunks · breakers               (test tenant AoR;
   |                           RoutePlan · overload            answers probes)
PostgreSQL (HA)                                                (later: B2BUA
serializable per-AoR txns                                      session services)
   |
   +---------------------- media allocator / MediaRelay ---------------+
                                     |
                    rendezvous(tenant ‖ Call-ID ‖ from-tag)
                                     |
              +----------------------+----------------------+
              |                      |                      |
        rtpengine A            rtpengine B            rtpengine C
        NG control (private)   RTP/SRTP (public)      dedicated hosts
```

Signalling scales by adding edges; media scales by adding relay nodes; neither waits on the
other. Management interfaces are private-network only.

## 2. Layering: what sipx provides, what sipx-clstr adds

The kernel boundary follows *upstream first* ([AGENTS.md](../AGENTS.md),
[upstream.md](upstream.md)): protocol logic below the line, orchestration above it.

```mermaid
flowchart TB
    subgraph clstr["sipx-clstr (this repo)"]
        direction TB
        profiles["deployment profiles + config roles<br/>(extension-framework, deployment)"]
        hooks["hook runtime · RFC registry<br/>(extension-framework)"]
        proxy["proxy engine §16 — sans-IO<br/>(proxy-engine)"]
        driver["proxy transaction driver 1→N<br/>(proxy-engine, decided: lives here)"]
        reg["registrar + LocationStore CAS<br/>(registrar-location)"]
        aff["affinity tokens · flow_ref · owner RPC<br/>(cluster-affinity)"]
        rt["RoutePlan · trunks · overload<br/>(routing-trunks)"]
        media["MediaRelay: Null | rtpengine NG<br/>(media-control)"]
        harness["deterministic cluster harness · conformance<br/>(conformance-harness)"]
        probe["e2e-tester: probe engine · echo · trigger API<br/>(e2e-tester) — off the call path"]
        profiles --> hooks --> proxy
        proxy --> driver
        proxy --> reg
        proxy --> aff
        proxy --> rt
        proxy --> media
        profiles --> probe
    end
    oper["Helm chart + k8s operator: values.yaml → SipxCluster<br/>(k8s-deployment-operator)"]
    oper -. "renders config · selects roles + profile" .-> profiles
    subgraph sipx["sipx kernel (upstream)"]
        direction TB
        txl["sipx-sip: lossless messages · 4 transaction FSMs (RFC 3261 + 6026)"]
        tp["sipx-transport: UDP/TCP/TLS/WS/WSS · pool · RFC 3263 resolve"]
        other["sipx-sdp · sipx-call dialogs · digest formulas"]
    end
    driver --> txl
    driver --> tp
    reg --> other
    probe --> other
    probe --> tp
    harness -. "drives sans-IO parts on virtual time" .-> proxy
    harness -. "seeded probe scenarios" .-> probe
    pg[("PostgreSQL")]
    rtpe["rtpengine pool — operator-managed or external"]
    reg --> pg
    media --> rtpe
    oper -. "managed mode only" .-> rtpe
```

## 3. Dialog-forming call: token minted, media anchored

An INVITE through the cluster ([proxy-engine](designs/proxy-engine.md),
[cluster-affinity](designs/cluster-affinity.md)). The proxy stays out of the dialog; the token it
plants in Record-Route is the only cluster state the call needs.

```mermaid
sequenceDiagram
    participant A as UA Alice
    participant E1 as Edge A
    participant L as Location service
    participant M as rtpengine (via MediaRelay)
    participant E2 as Edge B (owns Bob's conn)
    participant B as UA Bob
    A->>E1: INVITE (offer)
    E1->>E1: validate §16.3 · authenticate
    E1->>L: resolve AoR → bindings (Path, flow_ref)
    E1->>M: offer(SDP) — node chosen by rendezvous hash
    M-->>E1: rewritten SDP
    E1->>E1: mint affinity token → Record-Route<br/>(tenant · shard · media node · expiry · tag)
    E1->>E2: connection-owner RPC (Bob's flow_ref → Edge B)
    E2->>B: INVITE over Bob's owned connection
    B-->>E1: 200 OK (answer)
    E1->>M: answer(SDP)
    E1-->>A: 200 OK (Record-Route with token)
    A->>B: ACK — dialog is end-to-end; route set carries the token
```

## 4. Mid-dialog request: any edge, zero lookups

The defining property (*state rides the message*): the M2 exit criterion asserts the cross-node
dialog-lookup counter reads zero.

```mermaid
sequenceDiagram
    participant A as UA Alice
    participant EC as Edge C (never saw this call)
    participant M as rtpengine
    participant B as UA Bob
    A->>EC: re-INVITE (Route: token from Record-Route)
    EC->>EC: verify token — tenant, direction,<br/>media node, expiry (fail ⇒ hard reject)
    Note over EC: no dialog database, no cross-node lookup
    EC->>M: update(SDP) — same node id, from the token
    M-->>EC: rewritten SDP
    EC->>B: forward along route set
    B-->>A: 200 OK (via EC)
```

## 5. Connection ownership and node loss

Connections cannot move; ownership makes that explicit
([cluster-affinity](designs/cluster-affinity.md)). Service HA is the guarantee; call-survival HA
is deliberately not promised in v1 ([deployment](designs/deployment.md)).

```mermaid
flowchart LR
    inv["request for AoR bob@t1"] --> look["location lookup"]
    look --> bind["binding carries<br/>flow_ref = signed(node, conn, generation)"]
    bind --> alive{"owning edge<br/>reachable?"}
    alive -- yes --> rpc["owner RPC → edge writes<br/>to its connection"]
    alive -- no --> alt{"another registered<br/>flow / binding?"}
    alt -- yes --> next["try next target"]
    alt -- no --> unavail["temporarily unavailable<br/>(M3: 430 Flow Failed / push wake)"]
    rpc --> ok["delivered"]
    reconnect["client reconnects to any edge"] -. "generation bump<br/>new flow_ref in binding" .-> bind
```

## 6. The request pipeline is the hook surface

Extensions attach to typed phases; they never edit the core
([extension-framework](designs/extension-framework.md)). Media anchoring itself is just a module
on the offer/answer-bearing phases.

```text
 message parsed → request validated → before auth → after auth
       → [registrar path: before/after registrar update]
       → before target resolution → targets resolved
       → before forward  ──►  branches out
       ◄──  response received → before response forward

 module manifest: hooks · deps · conflicts · headers/tags owned · state needs · timers
 startup: framework computes the extension graph — invalid set fails boot, not a call
 profiles: CoreProxy · ModernRegistrar · CarrierInterconnect · WebSocketUA
```

## 7. Synthetic end-to-end probe: dial the border, echo, verdict

The outside view ([e2e-tester](designs/e2e-tester.md)). The probe is an ordinary external UA in a
test tenant; it takes the public path, and the same run record comes out whether the schedule
fired it or the API triggered it.

```mermaid
sequenceDiagram
    participant OP as Operator / CI / k8s operator
    participant T as e2e-tester (probe role)
    participant E as SIP edge — the border
    participant L as Location service
    participant EC as Echo endpoint (test tenant)
    Note over T: schedule fires (interval + jitter)<br/>— or an API trigger: POST /probes/.../runs
    T->>E: REGISTER probe AoR — via DNS/VIP, per edge + transport
    E-->>T: 200 OK
    T->>E: INVITE echo@test-tenant (correlation marker)
    E->>L: resolve AoR → echo binding
    E->>EC: forward (marker intact)
    EC-->>T: 200 OK (marker reflected)
    T->>EC: ACK, then BYE
    T->>T: verdict = pass / fail(step, cause) /<br/>inconclusive(probe-side) + per-step latency
    T-->>OP: run record · metrics by edge/transport/zone<br/>· alert on consecutive failures
    Note over T,EC: test tenant has no trunk access —<br/>a probe can never reach a carrier
```

## 8. Deployment control plane: one config file to a running cluster

How the platform is delivered and kept true ([k8s-deployment-operator](designs/k8s-deployment-operator.md)).
DP-1's config schema *is* the CR spec; DP-2's manifests stay the readable reference of what the
operator generates.

```mermaid
flowchart TB
    vals["values.yaml — the one config file<br/>helm install · helm upgrade, any time"] --> chart["Helm chart:<br/>operator · CRDs · RBAC · one SipxCluster"]
    chart --> cr[["SipxCluster CR<br/>spec = DP-1 config schema"]]
    chart --> op["sipx-clstr operator<br/>diff desired vs observed"]
    cr --> op
    op --> res["reconciled: edges (host net / source-preserving L4) ·<br/>registrar shards · routing · PostgreSQL · e2e-tester ·<br/>Secrets (token keys) · PDBs · NetworkPolicies · scrape config"]
    op --> pool{"media pool mode"}
    pool -- managed --> mm["operator runs rtpengine:<br/>host net · RTP ranges · NG private ·<br/>readiness on NG answering<br/>(self-contained k3s demo)"]
    pool -- external --> me["declared, not managed:<br/>validate endpoints + ranges,<br/>publish membership,<br/>never create/restart/scale"]
    op --> classify{"classify each changed field"}
    classify -- "hot-reloadable:<br/>trunks · keys · shard map · route policy" --> reload["push to running nodes —<br/>no restart, no call disturbed"]
    classify -- "invalid / incompatible" --> reject["rejected at admission —<br/>last good config keeps running,<br/>nothing partially applied"]
    classify -- "needs a rollout:<br/>listeners · profile · image · replicas" --> stages["staged plan:<br/>one role, one zone at a time"]
    stages --> drain["drain: stop accepting → clients re-register →<br/>bounded window → terminate"]
    stages --> hand["drain-then-switch on the shard map"]
    reload --> keys["key rotation: distribute, then activate"]
    stages --> health{"between stages:<br/>probe verdict + invariant metrics"}
    health -- regressed --> pause["pause · report · no further stages"]
    health -- ok --> stages
    res --> status["status: Ready · ProfileCompatible · ShardMapConverged ·<br/>KeysDistributed · rollout stage · last probe verdict"]
    metrics[("Prometheus — DP-3 metric set")] --> rules["recording rules: regs+latency per shard · CPS ·<br/>in-flight tx · media sessions · shed rate"]
    rules --> scale["autoscaler (phase 2)"]
    scale --> gate{"guardrails"}
    gate -- "shedding · invariant non-zero ·<br/>probe failing · zone floor" --> hold["hold — this is a correctness<br/>signal, not a capacity signal"]
    gate -- ok --> op
    scale -. "scale-in never bypasses the drain path" .-> drain
    verdict["e2e-tester verdict"] --> health
    verdict --> status
    verdict -. "rollout gate" .-> op
```

A newer config arriving mid-rollout supersedes the plan — the operator re-plans from observed
state rather than interleaving two rollouts, which is what makes "re-deploy whenever you like"
safe rather than merely possible.
