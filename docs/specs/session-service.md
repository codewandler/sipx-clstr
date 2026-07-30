# Optional session service

**Status:** normative target · **Epic:** `services-b2bua` · **Stories:** `BS-1` … `BS-3`

## 1. Scope and boundary

The session service terminates one SIP dialog and originates another when a feature structurally
requires it. It is a separately configured service, never a proxy mode and never on the default
proxy path. The proxy, registrar and routing cores do not depend on it.

Generic dialog, offer/answer, early-media, authentication and coupled-call behavior belongs in the
released sipx kernel. This repository owns service placement, routing policy, affinity, external
media-relay selection, configuration, observability and failure behavior. The service process does
not mix or forward RTP itself.

Queues and IVR are outside M4. M4 requires a two-leg bridge and a three-party conference focus.

## 2. Model

```text
SessionId     = opaque service-generated identifier
LegId         = inbound | outbound(n)
MediaMode     = signalling-only | anchored
SessionState  = offered | proceeding | early | connected | terminating | terminated
LegState      = inviting | early | confirmed | cancelling | ended
```

A session owns every leg and one offer/answer state machine per leg. No leg is shared behind a
mutex. Inputs are received messages, branch results, fired timers, relay results and policy
decisions. Outputs are send-request/send-response, arm/cancel-timer, route-leg, relay-command,
metric and terminate effects. Decision logic reads no socket or clock.

## 3. Two-leg behavior

An inbound offer is relayed to the selected outbound leg. Answers return on the same offer axis,
including reliable provisional responses, PRACK, UPDATE and re-INVITE. Early media uses the selected
relay before final answer and becomes the confirmed media path without allocating a second relay
session.

An inbound CANCEL cancels every unanswered outbound attempt and leaves no dialog. A final outbound
failure maps to the inbound leg through the specified response-selection policy. BYE on either
confirmed leg terminates the other. Glare is contained: a new offer on one leg while the peer leg
has an outstanding offer receives 491 and a bounded randomized retry input; it is not forwarded
into a second collision.

In `signalling-only` mode SDP is relayed according to policy and no `MediaRelay` effect is emitted.
In `anchored` mode every offer/answer is sent to the same selected relay node, whose identity rides
the session's affinity state. Relay failure before answer fails the session; after answer it follows
the published media-failure policy and is never hidden as successful media.

## 4. Conference behavior

A conference has one focus identity and at least three independently owned SIP legs. Joining a leg
allocates or updates conference state in the external relay. Leaving or failing a leg removes only
that participant; terminating the conference ends all remaining legs and deletes relay state.

The focus never embeds an audio mixer. DTMF policy is explicit per conference: deliver to the
service, pass to a selected leg, or suppress. The default is deliver to the service. Every join,
leave, mute-policy change and relay failure is observable with `SessionId` and `LegId` but no
credentials or key material.

## 5. Ownership, timers and failure

One healthy service instance owns a session. The affinity token routes subsequent requests to that
owner. M4 promises service HA for new sessions, not survival of an established session after owner
loss; the HA statement says so. A lost owner causes deterministic cleanup of expired relay state and
new calls route to a healthy owner.

All timers are named and fired as inputs: outbound attempt, PRACK, offer/glare retry, session expiry,
relay exchange and termination cleanup. Every timer has one arm and cancellation rule in the state
table tests. Background work is bounded and cancellation-safe.

## 6. Vectors

| ID | Input sequence | Required result |
|---|---|---|
| `SS-1` | Inbound INVITE → outbound 180 → 200 → ACK | Both dialogs confirmed; one session owner |
| `SS-2` | Reliable 183 with SDP → PRACK → 200 | Early media starts and is reused after answer |
| `SS-3` | Inbound CANCEL before answer | Outbound CANCEL; no surviving dialog or relay state |
| `SS-4` | Outbound 486 | Inbound 486; session terminates |
| `SS-5` | BYE on either confirmed leg | Peer BYE and bounded cleanup |
| `SS-6` | Simultaneous re-INVITEs | One axis proceeds; the other gets 491 and bounded retry |
| `SS-7` | Signalling-only session | No relay command is emitted |
| `SS-8` | Anchored UPDATE/re-INVITE | Same relay node and session are updated |
| `SS-9` | Relay failure before answer | Call fails explicitly; no false connected result |
| `SS-10` | Three legs join a conference | External relay reports three participants and audio proof passes |
| `SS-11` | One conference leg leaves | Other legs continue; departed relay participant is deleted |
| `SS-12` | Owner process is killed | Existing-session loss matches HA statement; a new session succeeds elsewhere |
