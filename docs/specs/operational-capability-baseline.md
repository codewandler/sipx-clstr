# Operational capability baseline

**Status:** normative milestone target · **Milestone:** M4 · **Tracker:** `CX-8`

## 1. Purpose

M4 is the first release gate for the complete endpoint-and-platform system rather than for one
crate, node or protocol role. It is satisfied only by capabilities reachable through released
public surfaces and proved across real sockets and a three-zone deployment. A parser, state machine
or media primitive that no released caller can select does not satisfy a row.

This document contains no comparison target. Requirements come from the RFCs, the project visions,
the normative subsystem specifications, and the executable scenarios below.

Normative words **MUST**, **MUST NOT**, **SHOULD** and **MAY** are used as in RFC 2119 and RFC 8174.

## 2. Repository boundary

The sipx kernel owns protocol-generic syntax, transactions, dialog/call behavior, digest
primitives, endpoint media, the diagnostic phone and its release artifacts. This platform owns
proxying, registration service state, routes and trunks, affinity/connection ownership, push
orchestration, external media-relay control, optional session services, deployment and platform
release artifacts.

Every kernel dependency is consumable only after a tagged release is pinned and re-read in
[`upstream.md`](../upstream.md). Kernel `main`, a local patch or a shadow implementation does not
satisfy M4.

## 3. Required capability sets

### 3.1 Released endpoint and library

The pinned kernel release **MUST** provide reachable outbound INVITE authentication, early media,
call-level ICE and DTLS-SRTP selection, two-dialog coupling, call-level bridge/conference access,
attestation, diversion history and overload control. Its diagnostic phone **MUST** satisfy its
`DPH-1` … `DPH-12` specification vectors, and its crates and Linux x86_64/arm64 binaries **MUST** be
published as immutable artifacts.

The required kernel stories are `S-28`, `C-2`, `M-27`, `M-28`, `C-1`, `C-6`, `S-20`, `S-21`,
`T-22`, `P-8` … `P-13`, `A-9`, `A-10`, `X-44` … `X-46`, `M-32` and `M-34`.

### 3.2 Proxy, registrar and reachability

The platform **MUST** provide transaction-stateful and stateless forwarding, forking, CANCEL,
in-dialog route-set handling, server-side digest, a durable bounded location store, GRUU, Outbound
flow routing, push wake/refresh handoff, exact listener exposure and explicit failure settlement.
No admitted branch or registration operation may disappear as a log-only failure.

### 3.3 Cluster, trunks and media

Affinity tokens and flow references **MUST** route mid-dialog requests with zero global dialog
lookups. Trunks **MUST** have scoped selection, resolved transports, failover, CPS/concurrency
limits, source admission, egress header policy, outbound authentication, attestation policy,
diversion history and overload behavior. Media anchoring **MUST** use `MediaRelay`; the signalling
process never handles a media packet.

### 3.4 Optional session service

The separately enabled service specified by
[`session-service.md`](session-service.md) **MUST** bridge two dialog legs and provide a three-party
conference focus. It is never a proxy mode and is disabled unless configured. It uses released
kernel coupling/dialog primitives and external media-relay control.

### 3.5 Operations and distribution

The release **MUST** include immutable node images, an installable versioned chart, configuration
that applies or refuses every accepted security-relevant field, invariant metrics, continuous call
probes, a published HA/failure table and a capability document generated from executed evidence.
The built image that passes the proof is the image that is published.

## 4. Milestone story set

M2 and M3 are prerequisites. In addition to their unfinished stories, M4 includes:

| Area | Stories |
|---|---|
| Review remediation | `CX-7`, `DP-13` … `DP-15`, `PX-12` … `PX-15`, `RG-16` … `RG-21`, `FC-1`, `FC-6`, `RT-12`, `CF-20`, `KO-16`, `ET-7`, `CF-3`, `DX-14` |
| Reachability | `RG-22`, `RG-23`, `AF-3` … `AF-7`, `PX-4`, `RG-5` |
| Trunks and media | `RT-2` … `RT-5`, `RT-8`, `RT-9`, `RT-13` … `RT-15`, `ME-2` … `ME-5` |
| Session service | `BS-1`, `BS-2`, `BS-3` |
| Deployment/proof | `DP-2` … `DP-4`, `ET-4` … `ET-7`, `KO-2`, `KO-12`, `KO-17`, `CX-9`, `CX-10`, `CX-11` |

`CX-8` is the open tracker. A new defect blocks M4 only when it falsifies one of this document's
required paths or proofs; unrelated feature work does not expand the milestone.

## 5. Proof scenarios

All scenarios have finite setup, execution and cleanup budgets. Each archives configuration,
artifact digests, seed, logs, packet capture where permitted, and machine-readable verdicts.

| ID | Scenario | Pass condition |
|---|---|---|
| `OB-1` | Two released diagnostic phones register and call through one node | Authenticated call, audio and orderly BYE |
| `OB-2` | Signalling matrix | UDP, TCP, TLS, WS and WSS each complete without downgrade |
| `OB-3` | Media matrix | G.711 and Opus; plain RTP, SDES and DTLS-SRTP each carry asserted audio |
| `OB-4` | Early media | Caller receives remote audio before final answer and keeps the stream after 2xx |
| `OB-5` | NAT reachability | A WSS Outbound client and an ICE media path complete behind NAT |
| `OB-6` | GRUU and push | One instance is reached by GRUU; a sleeping binding is woken and refreshes before delivery |
| `OB-7` | Trunk policy | Resolution/failover, authentication, admission, attestation and diversion history are observed |
| `OB-8` | Media anchoring | Cross-zone call uses the selected external relay and survives UPDATE/re-INVITE |
| `OB-9` | Session service | A two-leg bridge and a three-party conference pass audio and teardown assertions |
| `OB-10` | Three-zone HA | Zero cross-node dialog lookups; killing one node leaves new calls and registrations working |
| `OB-11` | Bounded overload | Seeded load reaches shedding without timer collapse and cleans up every owned call |
| `OB-12` | Artifact reproduction | Published crates, binaries, image and chart install by immutable reference and rerun the smoke proof |

## 6. Release rule

M4 closes only when every required local story is `done`, every upstream row is `landed` in the
pinned tag, both repository gates are green, `OB-1` … `OB-12` pass on the publishable artifacts,
and the public documentation states the same capability and HA boundaries as the evidence.

Transcription, endpoint TURN candidates, ICE restart, queues, IVR, presence, SIPREC and IMS are not
M4 requirements. Their absence is documented and does not weaken any claim above.
