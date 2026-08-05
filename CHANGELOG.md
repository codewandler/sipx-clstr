# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.14.0] — 2026-08-05

This release makes the registrar's answer match both its authority and its durable state. An
authenticated principal cannot replace or erase another address of record; a location-store read
that failed or could not be decoded becomes `503` instead of a plausible empty result; multi-contact
reconciliation remains correct as earlier operations move the set; and the node now renders the
registrar's complete decided response, including granted Contact lifetimes and q values, Path,
`Supported`, `Require`, `Unsupported` and `Min-Expires`.

The cluster document takes its next step without pretending the runtime is finished. Membership,
key and shard-map sections now load and validate, including whole-document reload constraints, but
keys and placement still do not reach the live affinity and ownership paths. Inline credentials and
private keys are refused without copying their bytes into errors or process output. Existing
deployments must also add the now-required RPC endpoint to every call-path member.

The assurance layer is stricter about its own claims: connection faults compose across node death
and restart, release-facing capability/count statements are checked in both directions, normative
tables and specifications cannot fall out of the gate silently, and a `done` story must leave both
an Acceptance record and an exact changelog citation. The release does not add an operator, apply
runtime keys, deliver to connection owners, execute outgoing proxy CANCEL, select a TCP-only kernel
listener, or claim an independently implemented SIP peer.

### Changed

- **The complete workspace now resolves the immutable sipx `v1.0.0-beta.5` tag without a sibling
  checkout participating in the build** (`CX-15`). All four kernel pins moved together, the
  temporary `[patch]` was removed, and the binary identifies both this release and its kernel as
  `sipx-clstr 0.14.0 (sipx kernel 1.0.0-beta.5)`. The workspace, six internal lockfile packages,
  Helm chart, public release marker and checked CLI examples all name `0.14.0`. The patch-free gate
  and a real-socket call with the phone built from the same released kernel tag passed before the
  release commit was tagged.

### Fixed

- **REGISTER responses now carry the registrar's complete decided outcome on the wire** (`RG-19`,
  validated synthesis **V-10**). One exhaustive driver renderer covers successful and rejected
  outcomes without reconstructing registrar policy: a `200` lists every active Contact with its
  granted lifetime and stored q, followed by the stored Path in order and `Supported: path`; `423`,
  `421` and `420` responses now carry `Min-Expires`, `Require` and every `Unsupported` offender.
  A required-header build failure discards the partial response and produces a controlled bare
  `500`. Four failing-first real-UDP node tests prove LS-R-11/17/18/20 through typed kernel parsing.

- **Configuration refusals cannot echo an inline secret into an error or process log** (`FC-8`).
  `cluster-config` V9 now makes redaction a property of `ConfigError`: a
  refusal may describe an inline value or omit `found`, but never carries the value. The existing
  DSN, tenant nonce, and affinity-key paths are pinned with distinct sentinels, and the same test
  exposed and closed an uncovered `keyRef` neighbour inside deferred listener and management TLS
  blocks. `CC-V-25` proves all three reference spellings across five paths through the real binary.
  The operator admission-response surface is honestly deferred as `CC-V-26` to `KO-3`, because no
  operator exists yet to execute that claim; rejection still creates no object and changes no
  existing status.

- **A `done` story now has to leave both closure records the repository promises** (`CF-18`).
  The documentation gate requires an exact story-ID token in a parenthetical changelog citation
  and at least one checked item in the story's Acceptance section; a bare mention and a longer ID
  no longer pass by substring. Its self-test pins the original twelve property failures by name.
  The generated board remains track's definition rather than being independently reimplemented;
  contributors regenerate it after frontmatter changes. The same pass restores the historical
  records that the new rule found:

  - Every normative specification was brought under the vector gate, including a fabricated-row
    rejection that proved the denominator was enforced (`CF-8`).
  - The public site gained its stable information architecture (`DX-2`), guides for the shipped
    node (`DX-4`), migration maps (`DX-7`), affinity and registrar-shard explanations (`DX-8`),
    trunk and media clustering pages (`DX-9`), and deployment and scaling guidance (`DX-10`).
  - Startup now installs tracing before it reports unapplied configuration (`FC-2`), while a tenant
    auth block is applied where its policy can be honoured and refused where it cannot (`FC-3`).
  - The location store retains a bounded newest-first change feed (`RG-13`), and REGISTER contact
    reconciliation parses stored contacts once instead of once per comparison (`RG-14`).

- **Connection faults in the deterministic harness now compose in either event order** (`CF-28`).
  A killed node can no longer become a stalled reader when `StopReading` fires later, and a kill
  now demonstrably discards bytes held by the old process. Restart clears that process's
  backpressure state without healing a partition or replacing any other independently scheduled
  link policy. Five fault-composition tests pin both kill/stall orders, the new-process buffer,
  pre-existing partitions and stall-before-link precedence without changing existing replay traces.

- **REGISTER admission now distinguishes the Request-URI authority from the address of record and
  authorizes the authenticated principal before any store access** (`RG-18`, validated synthesis
  **V-09**). An unserved Request-URI previously reached authentication and a served `To` could mask
  it; domain checks then split the canonical AoR string after admission and answered `403` instead
  of S1/S5's `404`. Worse, any valid credential could write or wildcard-remove any AoR in the
  tenant. Admission now injects one pure tenant policy for typed served authorities and exact
  `(principal, canonical AoR)` grants. Failing-first real-UDP tests show Alice's valid credentials
  could replace and erase Bob's binding; both attacks now return `403`, while Bob's binding and
  revision remain unchanged. Six LS-A vectors also pin open-tenant decisions, aliases, ports, host
  case, IPv6 and malformed schemes, taking the generated conformance report to 215/619 proved rows.

- **Story lifecycle typos now fail closed in the vector coverage gate** (`CF-27`). The gate accepts
  exactly `backlog | ready | in-progress | blocked | done`, refuses a complete story carrying
  `status: don` by filename, and no longer ingests `_TEMPLATE.md`'s `{{ID}}` as a live owner.
  Its executable-claim guard now sees parenthesized scenario names, and the
  registered-and-excluded contradiction covers design records as well as normative specs. The
  self-test reports all 53 pinned cases it replays instead of describing only its original
  PB-F-1 fixture.

- **A multi-contact REGISTER now reconciles every operation against the indices as they are, not
  against a snapshot taken once** (`RG-16`, validated synthesis **V-05**, four rounds). A removal
  used to leave later operations in the same request matching against positions that no longer
  existed — the failing-first vector shows a binding surviving the very request that removed it.
  Reconciliation now rides a `Reconciling` view whose slot identities are stable across
  mutations, the B8 net-effect question is settled by deferral on a reorder-proof slot id rather
  than by predicting the final view, and `RG-14`'s pre-reconcile quota check is deleted per
  location-service §5.5.1 with `RG-25`'s `max_contact_ops` bound doing the cost-limiting before
  any per-contact work. The spec's B6–B9 rows renumbered to `LS-R-26`…`LS-R-34` with `LS-R-35`
  added for the deferral, all proved on both store backends — the vector count moves to 167/596.

- **The published two-node k3d walkthrough now runs through the final audible call** (`DX-13`).
  With all four Deployments ready, the pinned `sipx 1.0.0-beta.4` phone registered the greeting
  through node-b and dialled it through node-a's static Service address. The call answered with
  24000 recorded samples, zero packet loss and `heard_audio: true`. Executing the command also
  exposed four fields its example output omitted — the advertised and bound media addresses,
  packet count and recording path — so the published block now reflects the current CLI output.

### Added

- **The membership, key and shard-map sections a node is handed now load and validate**
  (`DP-16`). `AF-6` specified `membership[]`, `keys[]` and `shardMap`, `DP-8` closed without
  owning them, and nothing loaded them — a document written to `cluster-membership.md` would not
  start a node. All three are now parsed and validated against their specs: a member on the call
  path must declare its `rpc` endpoint (`MB5`, so **an existing cluster document without one
  stops starting until it grows the field**), key windows must be well-formed instants, and a
  reload is judged as a whole — refused unless every rollout-class section is unchanged, the
  version advances, no id is re-pointed, and the incoming mint key's window covers the overlap
  bound the retiring key needs (`RL10`/`RL11`). Twenty-five of the conformance rows that had been
  deferred to a closed story are now proved and ten re-pointed at stories that will actually
  prove them, taking deferrals from 410 to 385. What these sections still do **not** do is reach
  the runtime — they are reported as unapplied rather than silently ignored, and `DP-17` owns
  closing that.

- **The deterministic harness can now injure a connection, not just a link** (`CF-26`). Three of
  `owner-rpc` §10's failure scenarios were unwritable because `fault.rs` offered five faults and
  none of them could drop and re-establish a connection, restart a node under a fresh
  incarnation, or stop a peer reading. All three now exist — plus `ResumeReading`, because a
  stall with no resume is indistinguishable from a client that never came back and the accounting
  bugs live in the flush — and the two backpressure rejection causes stay the two distinct rows
  §10 insists on: the per-flow bound and `T_write` expiry are reached by different paths. Nothing
  draws from the harness PRNG, so no existing seed changes meaning: verified by compiling one
  probe at two revisions and comparing rendered traces byte-for-byte, which is a stronger check
  than the suite's own run-to-run replays could make. `AF-7` can now write those scenarios
  against a real owner instead of restating them.

### Fixed

- **Every release-facing claim is now held to something that can be executed** (`DX-14`,
  validated synthesis **V-18**). The README badge and the site quoted conformance counts that
  had gone stale, said "Fifteen specifications" where the registry held thirteen, and described
  capabilities the released binary does not perform — matched CANCEL and Timer C as `today` when
  the driver drops the timer effect, and an `ACK` denial that `PX-13` had made false. Counts and
  the specification inventory are now derived from `check-vectors.py`'s own registries rather
  than copied, wherever the numbers appear including inside the shields.io badge URL; each
  governed capability is mapped to its owning story and gated on that story's status **in both
  directions**, so a claim cannot be promoted while its story is open and a denial cannot outlive
  its story closing. Dated release notes are deliberately not rewritten, and the count of lines
  skipped as history is printed on every run. The real-socket proof is described as a
  same-kernel, separate-process integration test — "independent implementation" is reserved for
  `CF-3`'s interop target.

- **A location store this node cannot read is no longer treated as an empty store** (`RG-17`,
  validated synthesis **V-08**). A failed or undecodable PostgreSQL read became empty revision
  zero, so a REGISTER query answered `200` — telling a UA it is registered nowhere while the
  durable state was in fact unknown — and a no-op deregistration reported success for a removal
  that never reached the store. `LocationStore::read` is now fallible, distinguishing absence
  from a database failure and from an undecodable row; REGISTER answers `503` for every failing
  shape before processing, and a failed lookup reaches the proxy as `TargetsUnavailable` → `503`,
  kept distinct from both the empty target set's `480` and proxy-behavior R8's `500`
  (`PB-F-11`). The new rows run identically against the in-memory double and a live PostgreSQL
  with deliberately corrupted rows — a backend that needed its own variant would have broken the
  contract. Operators should expect `503` where a flaky database previously produced a silent
  `200`/`480`.

- **A normative table that has stopped being a table is now a red gate** (`CF-23`). A Markdown
  table cannot survive a blank line, and in a repository whose rules are cited by ID out of table
  rows, a row that silently stopped being a row is a normative rule that silently stopped being
  readable — `RG-25` produced exactly that in location-service §5.7 while the sentence above it
  still claimed both rows and the gate reported the tree clean. `check-docs.py` now refuses three
  shapes of the same defect: an **orphaned row** (a `|` line no separator follows), a **split
  table** (a blank line between header and separator), and a **ragged row** (a cell count that
  disagrees with its header — GFM pads a short row and *silently drops* a long row's extras). The
  check immediately found a live second instance the motivating one had hidden: `e2e-probe.md`'s
  `A7` row grew a third cell from an unescaped `|` inside a code span and lost half its sentence
  on the rendered site. Every failure names the file, the line and the row.

- **An out-of-dialog request carrying a pre-existing route set keeps the callee's URI**
  (`PX-16`, found by `PX-13`). The proxy asked the location service about a request whose route
  set already determined where it goes, then overwrote the Request-URI with the answer — so the
  callee's address disappeared from the message, and with a strict router in the set it was the
  proxy's own resolved answer, not the callee, that got parked at the Route end. The decision
  path now tests the surviving route set **before** any address-of-record lookup (RFC 3261
  §16.4/§16.5): a loose `;lr` hop leaves the Request-URI untouched on the wire, and §16.6 step
  6's strict-router swap puts the real callee where the strict router will find it. `PB-F-4`'s
  expectation is rewritten (its old bytes dropped the callee entirely), `PB-F-9`/`PB-F-10` are
  new, and all retargeting now goes through the kernel's `Request::set_uri` so no stale start
  line is forwarded verbatim.

- **The devspace walkthrough's greeting has an address a phone can actually dial** (`KO-18`).
  Three constraints meet on one string: `sipx dial` sends to a literal and resolves no names,
  `FC-4` refuses a REGISTER outside the tenant's `domains`, and `FC-5` put that list in a
  document written before any pod exists. The address of record was a Service *name* — which
  satisfied the last two and no spelling of `dial` could reach, so the published §4 call step
  could not work. It is now node-a's **static `clusterIP`**, declared in the same manifest as
  the document that serves it, and `devspace_dialable.rs` holds all four artifacts that must
  carry it byte-identically — the tenant's `domains`, the greeting's AoR, the site's `dial`
  command and the two-node script — to that one string, plus the property that makes it usable.
  This unblocks `DX-13`.

- **A normative spec can no longer exist outside the vector gate's view** (`CF-25`). `AF-3`'s
  296-line owner-rpc spec sat under `docs/specs/` with thirteen §10 failure scenarios while
  `check-vectors.py` stayed green without reading one of them — the checker only ever looked at
  specs its own `SPECS` table named. Every file under `docs/specs/` (and every registered design
  table) is now either registered or **excluded by a named entry carrying a reason and a live
  story** — the exclusion list is ratcheted in both directions (a dead document, an empty reason,
  a dead story, or a registered-and-excluded contradiction all go red), a registered prefix with
  no tabulated rows is refused rather than read as covered, and scenario tables keyed by
  test-function names are visible to the unowned-row guard. The generated report now renders the
  registered inventory and an "outside the vector ledger" table from the registries themselves.

- **The Helm chart now says what installing it does not do, and a check holds it there**
  (`KO-16`, validated synthesis **V-19**). `helm show chart` used to promise the operator, CRDs
  and RBAC while the only template was one custom resource nothing serves. The description now
  leads with "non-operational schema preview", the rendered manifest itself carries an
  `# UNSERVED:` header so a saved copy keeps saying it, `NOTES.txt` names the blockers
  (`KO-2`/`KO-3`/`ET-4`) at the one moment an operator is guaranteed to be reading, and
  `deploy/helm/check-advertised.sh` holds the metadata to the rendered inventory — including
  `version`/`appVersion`, which move to the repository's release instead of a `0.1.0` nothing
  ever cut, so **a release cut that forgets `Chart.yaml` is a red check** (the check needs helm,
  so like `check-values.sh` it sits beside the gate rather than in it).

- **A deferred conformance row is now judged by its named story's status, not by the name's
  existence** (`CF-24`). 239 of 428 deferred rows — 56% — named a story that had already closed,
  so the report's "deferred with a reason" was a dead letter for more than half its rows.
  `check-vectors.py` now refuses a `[[deferred]]` or `[[unasserted]]` entry whose story is `done`,
  missing, or the story that merely *wrote* the spec (a spec story closes the day the spec lands,
  orphaning its own rows — that pattern alone accounted for 188 of the 239). Every dead letter was
  re-pointed at the live story whose Acceptance or spec text actually claims the row — `RT-2` 106,
  `ME-2` 90, `DP-16` 35, `DP-13` 5, `FC-1` 2, `PX-4` 1 — and the new gate ratchets: if one of
  those stories closes without proving its rows, they go red again at that close. The report's
  green line now reads "deferred with a **live owner**", which is the sentence the gate actually
  enforces.

## [0.13.0] — 2026-08-04

This release puts M2's defining invariant into executable form: an edge mints the affinity token
pair into `Record-Route`, a mid-dialog request reaches a different edge, and that edge verifies and
routes it with **zero cross-node dialog lookups**. The pure token library proves every byte vector,
while the membership, key-distribution, shard-map and connection-owner contracts state what still
has to reach configuration and the driver before one public address is deployable.

It also closes three ways the running node contradicted its own decisions. `ACK` and other
mid-dialog requests now follow the dialog rather than the registrar; a terminal branch result stops
the remaining target set; and declared roles reach dispatch, so a proxy-only node no longer accepts
registrations. Unsupported security controls fail at load instead of appearing applied. A single
REGISTER is bounded before reconciliation, and the gate now proves completed calls release their
transactions at the full RFC-derived `128·T1` bound rather than mistaking a legal lifetime for a
leak.

### Fixed

- **A declared `cluster.security` control now stops the node instead of loading as applied**
  (`FC-6`, validated synthesis **V-06**). `unknownSource`, `sanityCheck`, `userAgentDenyList` and
  `internalZone` were on the loader's allow-list, validated against nothing, and reached no
  `NodeConfig` field — so a document asking for any of them started a node serving the **opposite**
  posture to the one the key was written to produce. They are refused at load, one error per
  declared control naming its own path.

  **The refusal is per control, not per section.** A story that specifies a consumer for one of them
  removes its own row and leaves the rest refusing; an all-or-nothing refusal would have to be torn
  out wholesale by the first control to land. The message describes the decision the control *would*
  make and never echoes the configured value, which is `FC-8`'s rule respected ahead of `FC-8`
  landing.

  **The chart shipped the defect in its most consequential form.** `deploy/helm/values.yaml`
  declared all four, so the chart this project publishes would have rendered a document promising
  ingress controls nobody enforced — and a chart is what an operator trusts without reading the
  loader. Nothing caught it, because `deploy/helm/check-values.sh` needs helm and a built binary and
  is deliberately not a gate step. The chart now declares `security: {}`; empty is valid and carries
  §8 V6's fixed Max-Forwards, and the section staying present is what holds `cluster-config` §7's
  registry, the `SipxCluster` mapping table and the chart 1:1.

  `cluster-config` §12 gains `CC-V-13`, `CC-V-14` and `CC-V-15`, each with a test that executes it —
  they were registered by the interrupted first pass with nothing running them, which is exactly the
  `CF-12` defect. The report moves to **157 of 586 rows proved**.

### Added

- **The two review-confirmed kernel gaps now have upstream owners** (`CX-7`). sipx `T-28` owns a
  proxy-usable outgoing-CANCEL operation that preserves the INVITE transaction, target and RFC 3261
  §9.1 fields; `T-29` owns exact UDP-only, TCP-only and shared UDP+TCP listener selection. Both
  stories carry the minimal failure against the pinned `v0.10.0` surface and remain **filed, not in
  the pinned release**: `PX-12` and TCP-only service in `FC-1` stay blocked until a tagged kernel
  release carries them.

  The third suspected kernel gap was not filed. `v0.10.0` already exposes fallible typed `Expires`
  parsing, so validated finding V-13 is `RG-20`'s local consumer defect: reject malformed-present
  registration fields atomically instead of shadow-parsing them. The upstream ledger now records
  that negative decision explicitly.

- **The affinity token can be minted and verified** (`AF-4`). `crates/sipx-clstr-affinity` implements
  `affinity-token`'s mint/verify contract, and all **eighteen** of its §10 vectors now pass — the
  first coverage M2's defining subsystem has ever had, taking the report from 136 to **154 of 583
  rows proved**. M2's headline criterion is *mid-dialog requests route by token with zero cross-node
  dialog lookups*; this is the token that makes it possible, and `PX-13` already made in-dialog
  requests route by the `Route` set it will travel in.

  **The vectors were derived twice, independently, by parties that never met.** The implementor
  recomputed `AT-1`…`AT-18` against an independent ChaCha20-Poly1305 before writing any Rust; the
  review then recomputed nine of them from **§3's field table** rather than §10's printed hex, using
  OpenSSL rather than RustCrypto, and reproduced the plaintexts, tokens and base64url parameters
  byte-identically. That agreement is only possible if header composition, field order, endianness
  and the AAD all match — so the spec and the implementation were not bent toward each other.

  **Sans-IO is enforced by the dependency graph, not by review.** `default-features = false` on the
  AEAD is load-bearing: the default feature set pulls in `getrandom`, which would put an
  operating-system entropy source inside a crate whose whole contract is that randomness arrives as an
  *injected* value. `getrandom` reaches this crate for none of its three versions, checked again after
  the dependency moved to `[workspace.dependencies]`.

  **`CX-5`'s defect class is closed rather than avoided.** That finding — a nonce that is a pure
  function of the second, the realm and the secret, so two clients challenged in the same second
  collide — is the obvious failure mode for a minted token. The nonce here is 96 *injected* bits,
  never derived; `AT-1` and `AT-2` share claims and differ in every byte after offset 1; and §8 `S9`
  refuses a pair whose two entries share one.

  The AAD is the whole 14-byte header **including version and key id**, so a token cannot be
  relabelled to another key or replayed as a flow reference. Both AEAD failure modes collapse to a
  single `Reason::Tag` — there is no padding and no distinguishable branch, so no oracle — and
  `Reason` has neither a `Display` nor an `Error` impl, so it cannot be formatted into a response by
  accident. Verification is stateless *structurally*: `verify` takes `&KeySet` and the crate contains
  no interior mutability, so a replay ledger would not compile.

  Robustness was measured rather than sampled: 200 000 random buffers, 260 000 structured mutants at
  every facts length, and the `u32` corners, in **debug** with overflow checks on. No panic, and no
  random input ever verified.

- **Cluster membership, keys and the shard map are specified, and key rotation has a runbook**
  (`AF-6`). `docs/specs/cluster-membership.md` defines the `membership[]`, `keys[]` and `shardMap`
  sections, the reload contract, and the two-phase rotation an operator actually follows. It is M2's
  key-distribution contract: `AF-4`'s token library consumes `keys[]` as an input, and `KY1` freezes
  its six attributes and binds a change to a new `apiVersion`, so the interface cannot move under a
  proved surface.

  `cluster-config` keeps every rule of §1–§9 — only three *pointers* moved, which its own §10 `A6`
  authorises. Nothing is defaulted twice: `L`, `S`, `E_max` and the `keys` ceiling all cite their
  owners rather than restating them, per §8 `V3`.

  **`CX-5`'s defect class is closed rather than avoided.** That finding — the kernel's digest nonce
  being a pure function of the second, the realm and the secret, so two clients challenged in the same
  second collide — has an obvious analogue in key distribution: two nodes deriving the same material
  from the same inputs. `UQ2`/`UQ3` forbid derivation outright and make distribution a verbatim
  transport with no agreement step; `UQ4` tabulates every unique-required value with the point at
  which a duplicate is refused; and `UQ5` names `boot-second` as having *exactly* `CX-5`'s shape and
  points at its mitigation.

  **Retiring a compromised key is restart-class, and the spec says so** rather than implying
  otherwise: `cluster-config` §9.3 `RL11` has no in-document escape, so `RB9` activates the successor
  by reload everywhere and then rolls a restart with the compromised key removed, with exposure
  bounded by the roll. One security operation that configuration cannot perform, stated where an
  operator will meet it.

  Specified, not yet loaded: a document written to §3 will not start a node today — `rpc` and
  `incarnationSource` fall outside the loader's closed world and `keys`/`shardMap` are still deferred
  sections. `DP-16` owns closing that, and carries the four in-tree documents `MB5` will invalidate
  the day it does.

- **A transaction nothing will collect now fails the gate** (`CF-22`). No gate step started a node,
  completed a call, and watched the transaction accounting return to zero. `scripts/e2e-call.sh` did
  — and `CF-15` deliberately made it a separate CI job rather than a gate step, so that a red reads
  "the end-to-end call broke" rather than "the gate is red" and `gate.sh` stays runnable without a
  second checkout. A good decision, and this is the hole it left: the one check watching resource
  lifetime was the one contributors never ran.

  **The bound is `128·T1`, not `64·T1`, and the derivation lives in the code rather than in a comment
  beside a constant.** For a proxied non-INVITE to a silent next hop the two windows are additive and
  neither is optional: Timer F (`64·T1`, §17.1.2.2) concludes the client transaction — §16.8 confines
  Timer C to INVITE, so nothing proxy-level ends it sooner — and only then does §16.7 have a final
  response to forward, at which point Timer J (`64·T1`, §17.2.2) starts. Until then the server
  transaction sits in Trying/Proceeding, which §17.2.2 gives **no timer at all**.

  The failing-first proof is an **injected hold nothing answers**, and its permanent form asserts the
  hold is the same size twenty windows later — so it proves unboundedness rather than slowness. The
  bound is load-bearing rather than decorative, falsified twice: halving it to one window makes the
  RFC's own worst case fail.

  It also corrects **`scripts/e2e-call.sh`**, whose drain loop waited **50 s** — strictly between one
  window and two — under a comment that budgeted for one. That threshold reverted a correct fix; the
  loop now waits past `128·T1`, spells out the arithmetic, and stops calling a bounded drain a leak.

### Fixed

- **Every `ACK` was resolved as an address of record, and dropped when there was no binding**
  (`PX-13`, validated review finding `V-03`). An `ACK` went through a path that ignored the `Route`
  set, treated the Request-URI as an AoR, asked the location service, took the first registration, and
  **silently dropped the request when there was none**. An ordinary remote `Contact` is not the
  registered AoR, so a normal call's `ACK` was dropped. Other in-dialog methods were preprocessed
  correctly by the pure engine and then the driver resolved their next hop as an AoR lookup anyway —
  a global lookup on the signalling hot path, which non-negotiable #5 calls wrong *by definition*
  rather than merely slow.

  `ACK` is now split by semantics, specified as `proxy-behavior` §7.2: the kernel's generated
  downstream `ACK` for a non-2xx stays transaction-scoped, an upstream non-2xx `ACK` is absorbed by
  its server transaction, and a 2xx `ACK` is a **separately routed request that is never answered**.
  The two this does not touch were verified against the pinned kernel's own transaction code. In-dialog
  requests take the core's selected next hop, and an unaddressable branch settles as an explicit
  `BranchTransportError` — §16.9 `R10` → `R8` — instead of being skipped while the context waits for a
  response nothing will send.

  **Why the harness never caught it:** the simulation's `ACK`/`BYE` are AoR-shaped, so they resolved by
  accident. The proof asserts the negative *positively* — two trap sockets registered under the
  contacts' canonical AoRs must both stay at zero hits, and the pre-fix run **observes** the hit rather
  than inferring the defect from silence.

  It also narrowed `P1`, which turned out to be required: `is_ours` is host-scoped and port-agnostic,
  so on a loopback deployment `P1` fired on ordinary mid-dialog requests and replaced the remote target
  with our own `Record-Route`. Verified in both directions — it cannot consume a route it does not own,
  and still fires on every value this platform places.

  **This shipped, was reverted, and was re-landed unchanged.** It was reverted for failing
  `e2e-call.sh`'s drain check; `CF-22` then established by measurement that the *check* was the defect
  — the store drains at exactly `64.000 s = 128·T1`, which the RFC permits, and the script waited
  50 s. The old check passed on `main` only because `main` answered a dialog's `BYE` `480` itself
  instead of delivering it, which is this very defect. It rewarded the bug and punished the fix.

- **One REGISTER could buy a fifth of a second of a core** (`RG-25`). Nothing capped how many contact
  operations a single REGISTER may carry. Roughly 3500 fit inside the 64 KB message limit once
  `Address::parse_list` flattens comma lists, and reconciliation is quadratic in that count, so
  against an address of record at its quota one datagram cost **211 ms** where a bounded one costs
  1.15 ms. `REGISTER` is deliberately exempt from the node's admission bound — a registration storm
  *is* the overload, and shedding refreshes turns a spike into an outage — so nothing upstream limited
  it either, and the registrar is open.

  `location-service` **§5.5.1** now states the bound normatively as rules `Q1`–`Q5`, anchored into
  §5.1's step order as **S6.1**, with an over-limit request answered `403`. The refusal sits before
  the wildcard/explicit split, so no stored binding is read, cloned, parsed or compared — position is
  the rule rather than an optimisation, because a bound applied *after* reconciliation refuses the
  same requests and prevents nothing.

  **The failing-first proof is 65 removals**, and the choice is the point: a removal never grows the
  set, so §5.5's binding quota cannot refuse such a request however long it is. That is why a second
  bound has to exist at all, and why `RG-14`'s quota pre-check could never have covered this class
  wherever it sat. The proof is measured, not timed — a new `op_meter` counts the request's own
  operations, because the existing `parse_meter` counts *stored* contacts and so reads `0` against an
  empty address of record whether the request carries one operation or three thousand.

  The bound is a per-tenant policy field rather than a constant, since a flat constant would collide
  with a configurable `maxBindingsPerAor`; §5.5.1 states the consistency rule and `FC-7` enforces it
  at load, because enforcement belongs to a configuration surface and not to a pure decision function.

  **Considered for upstream and declined, with the reason recorded in the spec rather than only in a
  commit:** a parser-level element cap *is* protocol-generic, and it would buy nothing this does not.
  Flattening is linear and the kernel already bounds 64 KB per message, 8 KB per header and 256
  headers, so the amplification is entirely in our reconciliation. Independently, §5.7's
  `BeforeRegistrarUpdate` may *adjust* the contact operations after parsing — so a parser-only bound
  is one a module could walk past.

  This is `RG-14`'s Acceptance item 2, which never landed. `RG-14` is `done` with **zero** of its five
  boxes ticked and no changelog entry; its parse-once view genuinely shipped and this did not.

- **A node answered every method, whatever roles it was given** (`DP-13`, validated review finding
  `V-01`, filed as `FC-6` before the review backlog renumbered it). The projected role set was read,
  validated, and used to select listeners and the location store — and then **dropped** before
  `NodeConfig` was built, so dispatch was by method alone: `REGISTER` to the registrar, `ACK` to the
  stateless path, everything else to the proxy, on every node regardless of what it was started as. A
  node started as `inbound-proxy` accepted a `REGISTER` and **stored the binding**, reproduced against
  the real binary.

  Roles now become a `Capabilities` set derived from `ProjectedConfig.identity.roles` in
  `startup::node_config`, carried on `NodeConfig`, and consulted by dispatch. A capability set rather
  than the raw roles on purpose: `cluster-config` §4 `R3` forbids consulting a *role* when classifying
  a request, and carrying the set into the request path would invite exactly that. A role this build
  cannot serve — `echo`, `e2e-tester` — now raises `StartupError::RoleNotServed` and stops the node
  instead of being silently ignored, which is this epic's apply-or-refuse rule applied one section
  over.

  An `ACK` with no forwarding path is **dropped and logged**, not answered. That is not an improvised
  exception: `cluster-config` `V11` already makes the identical argument with the identical citation —
  an `ACK` for a 2xx has no response in SIP at all (RFC 3261 §17.1.1.3), so there is no status to send
  it.

  **This closes the fail-open and not the whole story.** `DP-13` stays open for three things this does
  not do: the refusal is `405 Method Not Allowed` with `Allow` where the story asks for `503` with
  `Retry-After` (and `481` for an unmatched CANCEL) — and the story is right, because `405` tells a
  client the method is permanently unavailable and it should stop, where `503` invites failover to a
  node that *does* serve the role; the `ACK` drop is logged but not counted; and the echo engine is
  not wired, which is blocked on `e2e-probe` §9 defining configuration fields it currently does not
  have. Ticking those without code is what the story record refuses to do.

- **An accepted call could start a new one** (`PX-14`, validated review finding `V-04`). A 2xx, a 6xx
  or an upstream `CANCEL` cancelled the branches that had been *launched* and left never-launched
  targets sitting in the queue. When a cancelling branch later settled, the generic final-response
  path saw a non-empty queue and called `fork_next_group` — originating a fresh `INVITE` to a
  lower-`q` target **after** the transaction had been accepted, globally rejected, or cancelled. The
  reproduction is three targets: A and B at `q=1.0`, C at `q=0.5`; A answers `200`, then B answers
  `487`, and C gets dialled.

  `conclude_target_set()` now clears the queue on all three terminal paths, and
  `may_fork_next_group()` replaces three bare `!queued.is_empty()` tests, additionally refusing to
  fork a context that is already `answered` or `finished` — so the rule is stated rather than implied
  by a queue that happens to be empty.

  **RFC 6026 is preserved, and that was falsified rather than assumed.** The clear touches `queued`
  only and never `branches`, so a late 2xx from a branch that really was launched is still forwarded —
  checked at review for both the "A 200 then B 200" and "6xx then late 200" orderings, and `pb_v_8`'s
  `Max-Breadth` serialization still holds on the Timer C and transport-error paths its own test does
  not reach.

  It carries a second defect found in passing: `finish_if_settled` read the same predicate, so **a
  6xx with a queue behind it could never terminate the context at all** — the base emits `[Respond]`
  and stays unfinished where this emits `[Respond, Terminate]`.

  The defect violated no written rule, because sequential forking was implemented and never
  specified. So `proxy-behavior` gains **§7.1**, naming the queue the rules needed to refer to, plus
  **R12** (§8) and **C7** (§9) bounding its lifetime — without §7.1 the new rules would have pointed
  at nothing. New vector family `PB-T`, registered in `check-vectors.py` in the same commit that
  writes the rows; the registration is load-bearing, and deleting it fails the gate.

### Added

- **Five ledger rows three specs promised, and a gate against the dead letter that hid them**
  (`CX-6`). `asserted-identity` §2, `number-normalisation` §1 and `proxy-behavior` §1 each named
  candidate upstream rows and deferred the filing to `CX-1` — which closed `done` without filing any
  of them, so three specs pointed at a ledger entry that did not exist and a closed story was the
  only thing standing behind them.

  **Five rows rather than three**, because two of those paragraphs say "*both* are candidate ledger
  rows": filing three would have left half of each pointing at nothing, which is the same dead letter
  one indirection down. Each is **decided rather than assumed**, per non-negotiable #6 and `CX-1`'s
  own precedent with `T-17` — four decided upstream and not yet filed, and one **declined**: the RFC
  5393 loop-detection branch cookie stays here, because it is a keyed MAC over the engine's own
  routing state with no kernel caller, and the row records the local plan and the three symbols that
  would move if that ever changes.

  Every citation was re-read in the pinned `v0.10.0` checkout rather than inherited. That is not
  ceremony: `CX-4` had to correct a row whose line numbers were **wrong when filed** rather than
  merely stale.

  The gate now refuses the shape that caused it. `check-docs.py` fails a `docs/specs/` paragraph that
  names a ledger row without linking the ledger, and fails one that defers filing to a *story* — a
  story closes, the ledger does not. The ledger's own opening sentence attributed all filing to
  `CX-1`, which was the root of it, and is amended.

- **One switch decides whether the deployment runs its own rtpengine** (`KO-15`).
  `deploy/helm/values.yaml` stated that fact **twice** — a `deployment.rtpengine.enabled` flag beside
  the media pool's own `mode` — so a chart could be installed saying both yes and no, and nothing
  objected. Resolved by **removal rather than derivation**: deriving the flag from the mode would have
  left a values key an overlay can still `--set`, which is still two places to write one fact. The
  media pool's `mode` is now the only switch, and `templates/_helpers.tpl` carries the one condition.

  Held mechanically by a fifth axis on `KO-1`'s `check-crd-drift.py` rather than a second checker: the
  `deployment:` half of the chart is now **closed and declared**, so a `deployment:` key with no
  `SipxCluster` field must be a chart-local one the spec declares *with a reason*, or it is a defect
  (`sipx-cluster-crd` §4 `M7`, table in §5). That turns a sentence the spec already contained into
  something a gate can enforce — the same declared-inclusion mechanism `KO-1` chose for the other four
  axes, and the reason it is not "the two keys must agree": an agreement rule is what the story
  forbade, because it keeps both spellings alive.

  The failing-first proof has two halves, and the second is the load-bearing one: the new axis is red
  on the duplication, **and the previous checker was green on it** — which is what shows that nothing
  in the gate had ever objected. Falsified again at integration by putting the duplicate key back.

- **A documented version banner is now held to the one the binary prints** (`CF-19`). `check-site.py`
  verified the *flags* a documented command names against `--help` and never what a command is shown
  as **printing**, so `website/docs/whats-new.md`, `reference/cli.md` and `getting-started.md` all
  carried `sipx-clstr 0.11.0 (sipx kernel 0.10.0)` after the workspace had moved to `0.12.0` — through
  a full green gate. It is the defect `DX-12` was written to close, one level down: `DX-12` held
  documented flags to what the binary *accepts*, and nothing held documented output to what it
  *produces*.

  The check reads **every fenced block** of every tracked `README.md` and `website/docs` page,
  whatever the info string — the banner is output, so it lives in exactly the `text` blocks the
  command check skips — and compares the whole line byte for byte, kernel half included, so a kernel
  pin bump that leaves the docs behind is red rather than silent.

  It keeps `cli_surface`'s two-reading discipline: `--version` from a built binary, and the banner
  composed from `Cargo.toml`'s `[workspace.package] version` plus the `sipx-sip` tag pin, so the
  `docs` workflow — which has no Rust toolchain — really checks rather than skipping. Both present and
  disagreeing is a failure. **Neither available while banners exist is also a failure**, rather than a
  green run that read nothing, on this repository's own rule that a check which silently narrows what
  it looks at is a lie. Every run prints which reading it used.

  This one is release-time by nature: the string only goes stale at a cut, and the site deploys *on
  release*, so the first reader of a wrong number is a public one. Falsified at integration as well as
  by the implementor — setting a page one version back turns the gate red and names the file, the
  line, the documented string and the actual one.

### Changed

- **The public capability matrix now describes the driver, not the engines** (`FC-6`, `PX-12`,
  `PX-13`, `PX-14`, `RG-16`). An [independent adversarial review](docs/reviews/00-validated-synthesis.md)
  of `86e6b10` — the `0.12.0` cut — three lenses filed without reading each other, then validated
  against the source by a coordinator — found five release blockers, all of the same shape: a pure
  decision core produces the right
  effects, and the real node does not perform them. The engines were never wrong; the **claims** were,
  because a vector row proves what the engine *emits* and nothing was checking that the driver
  performed it.

  What the README and [intro](website/docs/intro.md) had been calling current, and no longer do:

  - **Matched `CANCEL` and Timer C.** `driver.rs` matches
    `AnswerCancel | SetTimer { .. } | ClearTimer { .. } | Terminate => {}` — a literal discard — and
    says so in its own comment: on this driver "a Timer C is armed with the right value and never
    fires". `PX-6` is not a dishonest close; its Acceptance is engine-scoped and the deterministic
    harness *does* perform these effects, which is why `PB-C-5`/`PB-C-6` pass. Nothing owned the
    driver half, so `PX-12` now does.
  - **Role separation.** The projected roles pick listeners and the location store and are then
    dropped before `NodeConfig` is built, so dispatch is by method alone. A node started as
    `inbound-proxy` accepts and stores a `REGISTER` — reproduced against the real binary. `FC-6`.
  - **In-dialog routing.** Every `ACK` is resolved as an address of record and silently dropped when
    no binding exists; an ordinary remote `Contact` is not the registered AoR. The harness missed it
    because its `ACK`/`BYE` are AoR-shaped and resolve by accident. `PX-13`.

  Two engine defects were validated with reproductions and are filed rather than papered over: a
  terminal 2xx, 6xx or upstream cancellation leaves never-launched targets `queued`, so a later `487`
  starts a fresh INVITE after the call was already accepted (`PX-14`); and multi-contact `REGISTER`
  reconciliation resolves every operation against indices computed once, so a removal alongside an
  addition can commit the wrong binding set (`RG-16`). Single-contact `REGISTER` — every existing
  proof — cannot expose the second.

- **Release-facing counts were stale on the README** — the badge and the capability table still read
  `134/492 proved, 358 deferred` and "Ten specifications" after `CF-8`, `CF-12`, `EX-12` and `KO-1`
  had moved all four numbers. Now `125/549`, 19 shape only, 405 deferred, eleven specs. The site's
  copies were corrected in the `0.12.0` cut and the README's were missed, which is the same defect
  one file over: nothing in the gate reads a published number and compares it to the generator.
  `CF-19` covers the version string; this row is why it should cover the counts too.

## [0.12.0] — 2026-07-30

The release where the gate was pointed at itself, and did not come off well. A third of the entries
below are a check that could not see the thing it was believed to check: a conformance report printing 138
of the 533 rows it counted, thirty-one vector rows registered in a file the gate never opened, a
"runs in CI" claim held by a substring match, two checkers reading prose *about* a directive as the
directive, and a sweep that asked every `done` story whether the deltas another document named for it
had actually landed — the answer was no three times. None of these was found by a failure. Every one
was found by reading a checker and asking what it would have to miss for the report to still be
green, which is why they had all survived a release.

What that bought is narrower than it sounds and worth more: the numbers this project prints about
itself are now numbers it can defend. A proved row compares the value it claims (`CF-12`), a deferred
row names the story that will cover it, and the end-to-end call — the evidence behind the headline
claim that a call completes with audio — runs on every push against the kernel's own CLI rather than
being run by hand and cited afterwards (`CF-15`).

Beside that, three things a cluster operator can use: a node now bounds how much work it holds at
once and answers `503` with `Retry-After` rather than accepting without limit (`DP-11`); the pinned
kernel moves three releases forward to `v0.10.0`, bringing the declared Rust floor **down** to 1.91
(`CX-4`); and the `SipxCluster` custom resource is specified as one definition with the config schema
rather than a second copy of it, with a check that fails when the two drift (`KO-1`).

Still open and stated rather than implied: the digest replay window is `O(n)` per authenticated
request, and the nonce is not unique per challenge. Both live in one kernel file that is byte-
identical at every released tag and at kernel `main`, so neither is fixable from here and no bump
short of a new kernel release moves them.

### Added

- **The `SipxCluster` resource is specified, and it is the config schema rather than a copy of it**
  (`KO-1`). [sipx-cluster-crd](docs/specs/sipx-cluster-crd.md) defines the custom resource the
  operator will serve: its `spec` is [cluster-config](docs/specs/cluster-config.md) §7's `cluster:`
  tree **verbatim**, beside a closed operator half of exactly four fields — `image`, `roles`,
  `nodeSelector`, `tolerations`. The spec names sections and restates no field, type, default or
  ceiling, which is what makes it a single source rather than a second document to keep in step.

  The group and version are pinned as **`sipx.dev/v1alpha1`**, Namespaced, plural `sipxclusters`.
  That was already the string cluster-config §3 declares and the loader already implements as
  `API_VERSION`, so pinning it *removed* the word "provisional" from `deploy/helm/values.yaml`
  rather than choosing a new value — a new one would have broken the site, both node proofs and the
  loader constant to buy nothing a reader can see. `v1alpha1` carries no compatibility promise at
  all, and §3 says so rather than implying it.

  The single-source mechanism is **one shared definition with a declared inclusion, not generation
  in either direction**, with the reasoning in §4. Neither artefact is machine-readable today — §7
  is a prose registry whose owner column is the load-bearing part, and there is no CRD manifest
  until `KO-3` — so either direction of generation would have turned the file contributors actually
  read into derived output. Because the inclusion is total and verbatim, the only thing that *can*
  drift is which sections exist, and that is exactly what is now checked.

  `scripts/check-crd-drift.py` is that check, on five axes: the schema version's four spellings must
  be byte-identical; §7's sections and the spec's rows must match in **both** directions; the
  operator half must agree across the spec, `node-document.py`'s `OPERATOR_KEYS` and the keys the
  template writes into `spec`; and the `values.yaml` mapping must be 1:1 both ways. Each axis was
  demonstrated red before being made green, and it self-tests on every run. It reads **names only** —
  contents stay `deploy/helm/check-values.sh`'s, which feeds the rendered tree to the real loader.

  It is wired into `scripts/gate.sh` **and into `.github/workflows/docs.yml`**, because that workflow
  enumerates its checks rather than calling the gate: a step added only to `gate.sh` would never run
  on a pull request.

  Admission is specified in full as §8 (`A1`–`A10`), including the rules **no single node's
  projection can reach** — a zone with no edge is the case that proves the point, because a node
  projects its own zone and a zone with no edge is a fact about the set. `status` is specified as
  observation only (`S1`–`S9`): what the fleet is observed to be, never what was asked for. The
  sixteen `SC-*` vector rows are registered in `check-vectors.py` in the same commit that wrote them
  — `EX-12`'s lesson — and deferred, per row, to `KO-3`, which implements the webhook and the
  reconcile loop they execute against. Deferral rather than a weaker row: a vector asserting what a
  *document* says would test a file, and these rows exist to test an operator's verdicts.

- **A node bounds how much work it holds at once** (`DP-11`). The accept loop was
  `while let Some(arrival) = incoming.recv().await { … tokio::spawn(…) }` with no limit: the only
  backpressure was the kernel's *incoming queue*, which bounds arrivals rather than residency, and a
  node drains that queue as fast as it can while a proxied transaction lives until Timer B. So offered
  load and resident concurrency were the same number, and the ceiling was whatever ran out first.

  `AdmissionBound::admit` is now taken on the accept loop, before anything is cloned and before a task
  exists, and the knob is `cluster.admission.maxInFlightTransactions` — default **1024**, matching the
  kernel's own queue capacity so the two limits are one number rather than two that can disagree; `0`
  is refused, and the ceiling is 65536. Refusing over the bound is a SIP answer, not a drop: `503` with
  `Retry-After`, the same shape the kernel sends when *its* queue is full, so a client sees one
  behaviour whichever layer shed it.

  **`REGISTER` and `ACK` are outside the bound**, and that is the load-bearing decision rather than an
  omission. A registration storm *is* the overload, and a shed refresh is a phone that becomes
  unreachable — shedding REGISTER turns a spike into an outage. An ACK for a 2xx has no response in SIP
  at all (RFC 3261 §17.1.1.3), so "refusing" one could only mean dropping it, which is the call-leaking
  failure the kernel already counts apart. Every other gated method is subject to the bound, `BYE` and
  `CANCEL` included: exempting the requests that *end* work is tempting, because shedding them makes
  overload self-sustaining, but an unbounded method is an unbounded node, and a `503` with `Retry-After`
  to a `BYE` is a retry rather than a loss.

  The reasoning lives in [cluster-config](docs/specs/cluster-config.md) `V11`, not only in a doc
  comment, because the person who needs it is an operator reading the schema.

  The kernel's shed counters are also read for the first time: `Handle::shed()` separates shed requests,
  ACKs and unmatched messages, and nothing in this repository called it — `outstanding()` was the only
  kernel instrument used. `report_load` now samples all three beside `outstanding`, `in_flight`,
  `admitted` and `refused`. It is still a log line rather than a metric; a metrics endpoint is `DP-3`'s,
  and this is its input.

- **The end-to-end call proof runs on every push** (`CF-15`). `scripts/e2e-call.sh` — the evidence
  `website/docs/guides/registrations-and-calls.md` offers for the headline claim that a call
  completes with audio — now runs in CI, in its own `e2e` job, against the kernel's own `sipx` CLI
  built from the tag `Cargo.toml` pins. Reading the tag rather than writing it here means a kernel
  bump moves the phone with it, instead of leaving the job proving a call against a version nothing
  else uses; the CLI stays un-vendored, because the property that makes this a proof is that the
  client side is an independent implementation.

  `DX-12` had recorded this one as not-in-CI because the CLI comes from another repository. That
  premise was true and the conclusion was wrong: it costs a shallow clone and about forty seconds of
  `cargo build`, measured at both `v0.7.0` and `v0.10.0`. The cost of *not* running it was `FC-4`
  breaking this proof — and two of its siblings — for a whole release with nothing watching.

  It is a separate job rather than a gate step so a red says "the end-to-end call broke" rather than
  "the gate is red", and so `scripts/gate.sh` stays runnable without a second checkout.

  The proof needs `127.0.0.1:5060` literally, which is the port residue `CF-13` handed over. It is
  settled by constraining rather than varying: location-service §3.2 N7 makes an absent port and an
  explicit one distinct AoR keys (RFC 3261 §19.1.4), `sipx dial` has no `--target` so the call leg's
  address is the request-URI's and drags the domain with it, and `check-proof-domains.py` refuses a
  domain that is not a static literal — the check that makes `FC-4`'s `403`s impossible to repeat
  silently. So the script is one-run-per-machine by construction, `preflight` says so with exit 2 in
  0 s instead of failing halfway through, and CI is unaffected because each job gets a fresh VM.

  The three proofs that stay out of CI now name what would change that. `k8s-two-node-call.sh`: the
  phone *image*, not the cluster. `two-node-call.sh`: both of `DX-12`'s reasons are settled, so the
  honest reason is that nobody has written the job. `sip_demo.py`: it would have to start its own
  node, at which point it is `e2e-call.sh` with less coverage.

### Changed

- **The pinned sipx kernel moves three releases forward, to `v0.10.0`** (`CX-4`). All four `sipx-*`
  dependencies move together — a workspace holding two kernel versions is a protocol core disagreeing
  with itself — and the lockfile's package set is unchanged, so the bump pulled in no new transitive
  crate. What actually reaches this platform, named rather than left as a version number:

  - **The declared Rust floor moves *down*, 1.94 → 1.91, and its cause moves out of the kernel.**
    `v0.10.0` bounds `impl<K: Eq, I: Ord> Default for TimerQueue<K, I>`, which had been forcing 1.94 on
    every consumer for a `Default` impl rather than for anything the queue does. What holds the floor at
    1.91 now is **local**: `Duration::from_hours` in `sipx-clstr-proxy`, stable since 1.91. Both halves
    are evidenced rather than one — the workspace builds on 1.91 *and* its test suite passes on it — and
    the shipped image builds on it too. Carrying 1.94 forward would have left a true number attached to
    a false reason.
  - **The connection pool key now includes the WebSocket resource.** `Target` and `ConnectionKey` gain
    `path`, so two resources on one address are two pooled connections. Source-compatible here, and
    inert until M3 opens a WebSocket — the first WSS work that reaches the pool should re-read
    `ConnectionKey` rather than assume this.
  - **A breaking `sipx-ua` change in the range does not reach us.** `Config`/`Registration` dropped
    `outbound` for `instance`/`reg_id`/`gruu`, and `registrar::interpret` changed signature; this
    workspace consumes only `challenge`, `auth` and `Algorithm`, which is why the seam was drawn there.
  - **`v0.9.0`'s SRTP keying fix is wire-breaking between kernel versions and reaches nothing here** —
    no media crate is pinned and no media is relayed. Stated precisely so nobody reads "wire-breaking"
    and assumes it applies.
  - Additive and unused today: `gruu`, `push` (RFC 8599) and `update` in `sipx-sip`, RFC 8839 ICE
    attributes in `sipx-sdp`.
  - **Explicitly not fixed: `CX-5`'s nonce defect and `RG-15`'s replay-window scan.** This bump was
    expected to fix the first and does not. `sipx-ua/src/challenge.rs` is one blob — `30e1d290` — at
    `v0.7.0`, `v0.8.0`, `v0.9.0`, `v0.10.0` **and kernel `main`**, so the pin moved and the mint did not.
    Checked by blob hash at all five refs rather than read out of a release note.

  The [upstream ledger](docs/upstream.md) was re-read row by row against `v0.10.0` — every cited symbol
  opened in the kernel at that tag — because its own rule says to. One row closed, and one citation was
  corrected to say it was **wrong when filed** rather than that it drifted: `address.rs` is one blob
  across `v0.4.0`, `v0.7.0` and `v0.10.0`, and the old pair cited `Path` twice, so `Service-Route` was
  never cited at all. A correction that invents a drift to explain itself is the failure that ledger
  exists to catch.

  Verified at integration rather than on report: the end-to-end proof run against a `sipx` CLI **built
  from the same tag** (bob heard audio, 24000 samples, one socket on the node, transaction store
  drained), and `docker build` on the new floor producing an image that reports
  `sipx-clstr 0.11.0 (sipx kernel 0.10.0)`.

- **How many `done` stories closed with work another document had named them for: three** (`CF-16`).
  `EX-8` was named in writing as the owner of two changes, closed having landed one, and the orphaned
  half sat unowned for months while a real capability was missing. A story's acceptance is only ever
  checked against itself, so the board cannot tell "finished" from "finished the part it wrote down".
  This is the sweep that asks how many others there are, over all 81 `done` stories, with the number
  produced before anything was fixed:

  - `CF-8` registered seven vector prefixes and put **none of their thirty families** in the
    conformance report's render list, so **395 of the 533 rows the report counts appear in no table**
    — while `CF-8` ticked both "families named" and "the report regenerates". Filed as `CF-17`.
  - Three `hook-framework` rows still call the affinity-token budget "provisional until `AF-1`", and
    `AF-1` landed — `affinity-token` §3 made the 64-byte budget normative. `EX-8` diagnosed this in
    writing and recorded it as adjacent-not-mine; nobody owned it after. Filed as `EX-13`.
  - Three specifications each defer a gap "for `CX-1` to file", and `docs/upstream.md` has **no row
    for any of them**. Not `CX-1` failing — all three specs postdate its close. Filed as `CX-6`.

  **The checker was deliberately not built, and the measurement is the argument.** Sixteen
  delegation constructions over the corpus find 159 delegation-shaped mentions, 100 naming a `done`
  story, 3 real — 3% precision. The 97 false alarms are unreachable by a better pattern because the
  discriminator is *tense*, not vocabulary: "until `EX-12` landed the split" (resolved) and "until
  `CF-8`, this table is unenforced" (open) are the same shape. A check at that precision gets deleted
  by the third person it interrupts. Prevention is recorded instead, on `CX-6`: **a spec deferring
  work names a ledger row or an epic, not a story** — a story closes, a row does not.

  A separate defect family surfaced while measuring and is filed apart as `CF-18` rather than folded
  in, because it would **not** have caught `EX-8`: nine `done` stories are cited nowhere in this file,
  and three carry no ticked acceptance box at all.

- **A spec stops deferring to a story that closed** (`EX-13`, found by `CF-16`). `AF-1` landed and
  [affinity-token](docs/specs/affinity-token.md) §3 made the 64-byte module-fact sub-budget normative;
  three `hook-framework` rows went on calling it "placeholder until `AF-1`". §5 class (b) no longer
  restates the constant **at all** — a second copy of a value is what made the drift possible — `G5`
  cites §3 as the authority and says plainly that it owns the summation and not the bound, and `HF-7`
  keeps its `72 > 64` arithmetic with the 64 attributed rather than asserted. The number did not move
  and no row changed coverage class.

  `G5` also gains a forward-compatibility sentence, decided rather than defaulted: §3 explicitly keeps
  renegotiation open, so deleting the word "provisional" alone would have traded a stale-modality claim
  for a live contradiction — "not provisional" and "not renegotiable" are different claims. Deferring
  explicitly makes a future renegotiation a one-spec edit.

- **The deferral ledger says what it is** (`CF-16`). `docs/reference/vector-scope.toml` described
  itself as "the narrow, `PB`-only ancestor" of the conformance registry. That was true when `PB` was
  the only registered prefix, and it survived `CF-8`'s six registrations and `EX-12`'s seventh — by
  which point `PB` was the third *smallest* family in the file, which now carries 389 deferrals over
  eleven prefixes and 19 rows covered for shape only. A header describing a file as narrower than it is
  invites the next reader to add a row without reading the discipline above it, and that discipline is
  what keeps a coverage report honest.

- **Which failure a caller sees when a fork fails two ways** (`PX-11`). Forking to a contact that is
  busy and one that is gone used to return `404 Not Found`, because the best final was picked by
  numeric order. It now returns `486 Busy Here`.

  A branch's final is a statement about one *contact*; what the proxy sends upstream is a statement
  about the *address of record*. `404` says that address does not exist — which another branch
  answering has already falsified — and it tells the caller to give up rather than retry. RFC 3261
  §16.7 step 6 fixes the response *class* and then explicitly permits any response within it, so the
  RFC does not decide this; the specification now does, in a new §8.1, and cites what the RFC does
  and does not settle.

  Numeric order was also doing a job it cannot do: it let a `408` from a branch that **never
  answered** outrank a `486` from one that did, and would have let a downstream `400 Bad Request`
  outrank both — reporting a fault in our own message as the callee's status. Lowest code survives as
  the tie-break *within* a rank, which is a job it is good at.

  This was found because a vector row and its own test had asserted opposite outcomes for the life of
  the project — the row said `486`, the test asserted `404` — and both counted as proved until
  `CF-12` started requiring a proof to compare what its row claims.

### Fixed

- **The conformance report showed a quarter of the rows it counted** (`CF-17`, found by `CF-16`). Its
  denominator was every registered vector row — 533 — while it rendered only the five families in its
  render list, so **395 rows were counted and displayed nowhere**. Anyone reading the report to find
  out what is covered was reading a table that silently omitted `AI`, `AT`, `CC`, `FR`, `LS`, `MR` and
  `NN`. The registry and the deferral ledger were correct throughout; the *report* was partial.

  The defect was not the missing section titles. Registering a prefix in `SPECS` got its rows read,
  counted, deferred and gated — and getting them onto the page was a second step **nothing checked**,
  so `CF-8` could tick both "families named" and "the report regenerates" while three quarters of the
  corpus fell out of the document. `unrepresented_families` makes that a gate failure: red on 36
  `(prefix, letter)` pairs before it is green. And because extending the family list without a second
  guard would leave the class open, `render` now returns what it actually emitted, so the headline is
  checked against the rendered tables rather than against the script's own bookkeeping.

  Audited on the document rather than the script: 56 sections, 533 rows shown, 533 distinct, none
  shown twice, none counted and not shown. The `vectors:` line is byte-identical before and after —
  this story added no coverage and must not appear to.

- **"Runs in CI" was a substring match, and would have started passing for the wrong reason**
  (`CF-15`). `check-site.py` resolved whether a proof runs in CI with `name in text` over
  `scripts/gate.sh` and `.github/workflows/`. A commented-out line satisfied it, so did a step merely
  *named* after a proof, and so did a sentence in a comment explaining that the proof is **not** run.
  It was inert only because nothing in either file mentioned any proof at all — the same shape of
  false green that `DX-12`'s `not-in-ci:` directive was written to close, one level up.

  Demonstrated before it was fixed, on the real files: adding the single line
  `# scripts/e2e-call.sh` to `scripts/gate.sh` took the checker from `FAIL` / exit 1 to clean /
  exit 0, with nothing running the proof.

  The name must now appear where a shell would execute it: comments are stripped, a workflow is read
  only inside `run:` bodies (inline and block scalar), and the name has to stand in command position
  — after env assignments and wrappers like `timeout`, never as an argument to something else.
  Heredoc bodies and `\` continuations are not tracked, which is stated in the module rather than
  implied to be exact; both fail closed. A `self_test()` runs on every invocation, on
  `check-vectors.py`'s reasoning that a checker whose own defect was a false green has to carry the
  proof it is still closed — fifteen cases, including the exact `gate.sh` line above.

- **Thirty-one vector rows were invisible to the gate** (`EX-12`). The quirk-profile rows lived in a
  design record, and the checker reads rows only from the spec that *owns* a prefix — so no spec owned
  `QP`, and a fabricated row passed the check. They now live in `hook-framework` §9.1, which owns the
  prefix, and all 31 carry a deferral naming the work that would prove them rather than being counted
  as proved. The total goes up, not the proved figure.

  Registering a prefix is no longer something to remember: the checker now fails on any table row
  shaped like a vector whose prefix no spec owns, across both the spec and design trees. `QP` was the
  only unowned prefix in the repository, so that rule goes green exactly as this lands.

  The blocker underneath was a story that closed on half its scope. `EX-8` was named in writing as the
  owner of the `Replace`/`Field` body-claim split and rules `G9`–`G14`; its acceptance covered only
  the other half, and it closed `done`. Those deltas land here, which is also what makes it possible
  for a deployment to anchor media and run an SDP quirk at once instead of the two colliding.

- **Two checkouts could not be tested at the same time** (`CF-13`). The node's socket suites bound
  hard-coded ports, so concurrent runs collided with `Address already in use` — and the failure
  landed on whichever diff happened to be under test rather than on the one at fault. It cost two
  separate pieces of work real effort to prove a negative. Every node in the suite now binds port
  zero and is asked what it got, through the same `listening on` report the node already emits after
  binding, so no second readiness contract was invented.

  Four tests also waited on a fixed 1500 ms sleep for an event that usually arrived sooner. They now
  wait on the event: `startup_warns` went from 1.50 s to 0.02 s, and none of them can lose a race
  against a clock. A test that fails one run in five under load teaches people to re-run instead of
  read, which costs more than the test is worth.

- **The conformance report counted rows it could not vouch for** (`CF-12`). A row was called
  *proved* when a test **name** mapped to it, whatever that test asserted. Swept across the 140 rows
  that carried the label: 61 state a value in their expectation, **41 compare it, and 20 never do**.
  The published figure moves from 140 to **120 of 498 proved**, with 20 reclassified *shape only*.
  No test was changed to improve that number — the count is the finding.

  A proved row must now compare the value it claims. The checker reads the row's expectation minus
  its citations, and the *compared arguments* of the test's assertions — not comments, not the test's
  own name, not an assertion's message — so quoting a row near a test cannot buy a proof. Rows that
  state no value are unaffected: ordering, shape and refusal rows are not asked for a number they
  never had.

  Three of the twenty are worth naming. Five registrar-auth rows state `401` and none compares it, so
  a challenge wrongly issued as `407` would pass every one. `PB-F-1` — the row that started this — is
  *still* shape only, because its test compares the armed timer against the code's own constant
  rather than the value the row states, so moving the constant keeps the test green while the row
  goes quietly wrong. And `PB-R-5` says the best of a `486` and a `404` is `486` while its own test
  asserts `404`: a row and its proof that have contradicted each other for the project's whole life.
  That one needs a specification decision and is filed, not patched.

- **Two gate checkers read prose about their own directives as directives** (`CF-14`). Both took the
  first match anywhere in a file, so a comment *explaining* a directive was parsed as one — and a
  correct declaration could be outvoted by an earlier mention of the token. `check-proof-domains.py`
  failed closed, turning the gate red on a correct script and teaching contributors to avoid spelling
  the token. `check-site.py`'s was the same defect with the polarity reversed and therefore worse: a
  prose mention of `not-in-ci:` silently exempted a proof from being checked at all. Both directives
  are now a line whose entire content is the token and its value.

  The regression evidence is permanent rather than a test fixture: two proof scripts now carry
  comments that fully explain the directive and spell the token, one of them twice above its real
  declaration. If the anchoring regresses, the gate goes red on the repository's own documentation.

- **Three site properties held by hand are now checked** (`DX-12`). An authored page that no sidebar
  entry routes to, and a sidebar entry naming a page that does not exist, both fail the gate — the
  first is quieter than a broken link and therefore worse, because it does not 404, it is simply
  never reachable. `reference/cli.md` is verified against the binary in **both** directions, so a
  flag the page invents and a flag the binary grows without the page are each a failure; proved by
  temporarily adding a flag to the binary and watching the check catch it.

  The fourth property is the one worth reading: a script a page offers as **proof** must be run by
  the gate or by CI, or carry a recorded reason why not. An unverified proof and a deliberately
  unverified proof looked identical from the outside, and the first of those shipped a `403` for a
  release. All four proof scripts now carry that decision explicitly.

  The check reports what it could not run rather than narrowing quietly: eleven documented commands
  need Docker, a Kubernetes cluster or the `sipx` CLI, and it says so on every run.

- **An authentication decision left no trace** (`RG-15`). A `REGISTER` that was challenged, refused
  or admitted produced nothing an operator could see, so a credential-stuffing run against a tenant
  and a quiet night were the same log. Every outcome now produces exactly one record naming the
  tenant, the source, the status and the reason — and no credential, nonce, cnonce, response digest
  or presented username appears in any of them, which is enforced by the reason type being
  `&'static str` rather than by review.

  The record is owed to the **decision**, not to what follows it. A correct digest followed by a
  message that then fails to parse used to record nothing at all, because the outcome was read after
  the principal had been dropped — so a successful authentication was indistinguishable from silence,
  the exact failure this story exists to remove. Admission now returns the outcome alongside the
  decision, so no later failure can erase it.

- **The `timers` section was accepted, validated, projected — and armed nothing** (`PX-10`). An
  operator who set `timers.timerC` in the cluster document got the proxy crate's private 180 s,
  silently, because nothing ever assigned it: `grep timer_c` over the driver returned no matches.
  `timerC` is now wired from the document to the engine, and the four keys that still are not wired
  (`t1`, `timerB`, `timerF`, `maxCallDuration`) are reported as unapplied instead of dropped.

  `proxy-behavior.md` F11 carried the same self-refuting default `DP-12` had just removed from the
  configuration schema — a floor of `≥ 180 s` under an RFC bound that is strictly greater, with the
  default sitting exactly on it — and cited §16.8, which states no bound at all. The bound is
  §16.6 step 11. Both specs now say the same thing, which is what stops this from happening a third
  time.

  **The caveat, because it qualifies the headline:** the driver still performs no timer effect —
  `SetTimer` and `ClearTimer` are dropped in the same arm as `CancelBranch`, as they always have
  been. What changed is the deadline the engine *puts in* the effect. A Timer C is now armed with the
  right value and still does not fire; making it fire belongs with branch cancellation, since a fired
  Timer C's first act is to cancel the branch.

- **The two-node cluster proofs answered `403`** (`FC-5`). `FC-4` made a `REGISTER` for a domain the
  tenant does not serve a `403` — closing a real hole — and `scripts/e2e-call.sh` was updated with
  it. The two-node proofs were not: they registered in `127.0.0.1` and a Service IP against documents
  declaring `example.test` and `cluster.local`. So the evidence for this project's headline claim was
  red, and `README.md`'s statement that `two-node-call.sh` "proves it locally" was false. It passes
  again.

  The class is closed rather than the three instances: `scripts/check-proof-domains.py` reads every
  proof under `scripts/` and `deploy/`, resolves the address-of-record it registers through the
  script's own shell assignments, and compares it against the `tenant.domains` of the document that
  governs it — running in both the gate and CI. It refuses the three cheap ways to be vacuously
  green: `domains: []` fails (that is the fail-open `FC-4` removed), a domain only known at runtime
  fails, and a proof that ships no document and names none fails.

- **A configuration document could be refused for a value the loader itself supplied** (`DP-12`).
  The schema declared Timer C with a default of 180 s and, in the same rule, `MUST be > 3 minutes`.
  180 s *is* three minutes, so any document carrying a `timers:` section without naming `timerC` was
  rejected at startup against a default no operator wrote. RFC 3261 §16.6 step 11 is a MUST over a
  strict inequality with no rounding language, so the rule stands and the **default moved to 240 s** —
  the smallest whole minute above the floor, stated in the unit the RFC states the floor in.

  The floor is now checked against whatever value stands, written or defaulted. Previously the check
  was skipped entirely when a document carried no `timers:` section, which is what let a
  self-refuting default ship invisibly; a compile-time assertion makes a recurrence a build failure
  rather than a refusal in every operator's startup. The rule's own vector turned out to be
  *deferred* rather than proved — the other half of how this survived — and is now proved.

- **`timers.maxCallDuration` and `locationStore.ha` were accepted and silently discarded** (`DP-12`).
  Both are declared by the schema, read by nothing, and were not reported. They now appear in the
  loader's unapplied list, so setting one tells you it will not take effect instead of implying it
  did.

- **The published documentation described a binary that no longer exists** (`DX-13`). The site and
  `README.md` still taught the three provisional flags that `0.11.0`'s configuration document
  replaced, and still gave "there is no configuration path" as the reason authentication is off —
  which stopped being the reason when `FC-3` landed. The real one is that there is no credential
  store yet, and the two states are distinguishable by running the binary, so the pages now say which
  is which. Nine status tables were re-derived from the binary and the scripts rather than edited by
  hand, `run-a-node.md` gained an explicit warning on the public-listener case, and the `Dockerfile`'s
  inverted feature claim was corrected.

  The README quick start also could not have worked: it declared a tenant serving `example.test`
  while the demo it tells you to run registers `alice@127.0.0.1`, so the registrar correctly answered
  `403`. It now declares the domain it actually registers in, matching `scripts/e2e-call.sh`.

- **A rule the extension design asserted turns out to be false** (`EX-11`). The design said the
  composed quirk set for one attachment point is every trunk-bound and domain-bound profile's rules
  taken together, and `QP-C-2` was written to match. Derived from the premises the surrounding specs
  already fix — a quirk applies only on an egress leg toward one peer, and that peer is identified by
  the route, which names a trunk **or** a domain and never both — the two sets **never intersect**.

  This mattered in practice, not only on paper: under the union reading, one domain-bound profile and
  one trunk-bound profile writing the same header produced a startup error naming both, and the only
  documented repair was rejected by the rule governing repairs, because neither binding had a contest
  at its own attachment point. A two-line configuration was both illegal and unfixable. A new rule
  `G14` settles what a trunk-valued rule means at a domain binding, and the case the old reading
  rejected now has a vector.

- **The default Helm chart could not start a single node** (`KO-14`). Its `values.yaml` predated the
  configuration schema, and nothing checked one against the other: rendering the chart and loading
  the result produced **22 refusals for every one of the six roles it deploys**. A `media:` block the
  loader has no key for, listeners keyed `role:` instead of `roles:`, a `transport: http` listener,
  and a `security.maxForwards: 10` that RFC 3261 §16.6 step 3 fixes at 70 and does not offer as a
  knob. Four of the six roles had no listener at all, which the schema refuses outright.

  The values are now expressed in the schema's own vocabulary, and `deploy/helm/check-values.sh`
  renders the chart, rebuilds the node document from the rendered resource, and loads it through the
  **real loader** once per role — so this class of drift fails loudly next time instead of at the
  first `helm install`. Media is declared where the media-relay spec actually puts it, per trunk and
  per pool.

- **A dead contact no longer holds a live contact's answer for half a minute** (`PX-9`). Forking to
  two registered contacts at equal `q` drove the branches *in order*: a branch's response stream only
  ends when its transaction does, so a device that never answers held the request task until the
  kernel's Timer B — 64·T1, about thirty seconds — while the other device's `200 OK` sat unread in a
  stream nobody was polling. The ordinary two-phone registration is the case that hits it.

  Branches are now read concurrently and reduced serially: a `JoinSet` per fork group yields one
  event at a time into the proxy engine, so the sans-IO core still sees a single total order of
  inputs and RFC 3261 §16.7 response selection is untouched. Measured on the failing case, the
  answer reaches the caller in 1.4 ms rather than 32.0 s.

  One consequence is worth stating because it is observable: aggregated finals are now ordered by
  arrival rather than by drain order. The selected status is a minimum over the set and so cannot
  change, but *which* of two equal-status branch responses is relayed, and the order of aggregated
  `401`/`407` challenges, now depend on which branch answered first. §16.7 prescribes no order among
  equal-status finals, and preserving the old order would mean preserving the head-of-line blocking
  that was the defect.


### Known gaps

- **The digest replay window is `O(n)` per authenticated request** (`RG-15`). Measured on the pinned
  kernel: 7.5 µs against a one-entry window, 19.2 µs against its full 4096 — about 11.6 µs of pure
  scan, 2.5× the cost of the verification itself, and it runs under the node-wide authenticator lock,
  so it is a ceiling on the node rather than on one request. The window is a private field of the
  kernel's `Authenticator`, and a nonce replay window is an authentication primitive, so building a
  second one here would shadow-implement kernel logic. Filed in the upstream ledger with a
  reproduction instead.

## [0.11.0] — 2026-07-30

The release where "cluster" stops being a design document. Two nodes sharing one PostgreSQL location
service: a user who registers through one node can be called through the other, with audio. Scripted
both ways — two local processes and two pods on Kubernetes — and both scripts print what they do not
prove, because the thing they do not prove is the next piece of work.

### Added

- **A cluster, in the smallest honest sense of the word** (`DP-9`). Two nodes, one configuration
  document, one PostgreSQL location service: a user who registers through one node can be called
  through the other, with audio. Proved twice — `scripts/two-node-call.sh` for two local processes and
  `scripts/k8s-two-node-call.sh` for two pods in a local Kubernetes cluster.

  The in-cluster run reads its evidence back out of the cluster rather than assuming it: two pods on
  distinct IPs, both logging `store="postgres"`, one ConfigMap resolved per node through `${POD_IP}`,
  **two bindings in one database written by two different pods**, and then a call from alice that
  node-a forwarded to bob — whose REGISTER it never saw — with 24000 samples recorded and
  `heard_audio: true`. Media went directly between the phones, because there is no relay to go
  through.

  Both scripts print what they do **not** prove: each node record-routes its own address, so the route
  set names a node. Put a single Service or VIP in front of the two and in-dialog requests will spread
  across both, which is exactly the case affinity tokens exist for and they are not implemented.

- **The cluster configuration schema is executable** (`DP-8`). `DP-1` specified one cluster-scoped
  document and nothing read it, so the binary still had the three provisional flags its own source
  calls placeholders. `load(bytes, identity, env)` is now a pure function returning either a config
  or **every** error ordered by path, and `project` turns a cluster document into one node's view of
  it. This is the keystone for a multi-node deployment: roles, listeners, tenants and the location
  store were all unreachable without it.

  It is deliberately not a `derive(Deserialize)`. §8 V1 wants every error, not the first, and serde
  stops at the first and reports it as a message rather than a path plus a rule id; V2's closed world
  needs the same shape, because you cannot ask serde which keys it did *not* recognise. The document
  is walked by hand over a generic value tree. YAML and JSON go through one parser, so the two
  encodings cannot drift apart — a test asserts the same cluster loads identically as both.

  Scope is stated rather than implied: ten sections are validated, the other seventeen of §7's
  registry are *recognised but not descended into* and reported in `Config::deferred`. A section
  silently ignored would be configuration nobody applies with nothing saying so, which is the
  failure V2 exists to prevent, one level up.

- **A node is configured by that document at startup, and the provisional flags are gone** (`DP-10`).
  `DP-8` could read a cluster document and `RG-12` could act on one, and `main.rs` still built its
  config from three flags, so neither was reachable from a running node. `startup.rs` is the seam: it
  reads the file, reads the environment, and resolves the references the document deliberately does
  not contain, so the loader stays pure. The flags were **replaced, not extended** — a transition
  period where both work is the second configuration surface being removed — and `clap` replaces the
  hand-rolled parser, whose own comment called the surface "deliberately tiny and provisional", which
  expired when it grew node-identity semantics.

  Identity comes from **outside** the document (cluster-config §5 P1): node id, zone and role set, from
  the downward API in Kubernetes or the command line on a host. A document that could name which node
  is reading it would not be one document for the cluster. `dsnRef` is resolved here rather than in the
  loader (§8 V9 — resolution is IO), and a reference that does not resolve is a start-up failure naming
  the reference, never a quiet fall back to the in-memory store. Every loader error is printed, ordered
  by path, before the process exits `2`.

  TOML became a third encoding in the same change, converted into the same value tree YAML and JSON
  produce so validation has exactly one path, detected from the bytes rather than the file name, with
  cluster-config D3 corrected rather than left claiming two. A test asserts the whole `Config` is equal
  across all three.

  Two defects surfaced by running the node rather than reading it. It printed `listening on` — the
  documented readiness signal every proof script waits on — and *then* exited when the store was
  unreachable, so a script proceeded against a dying node; everything that can refuse to start now
  refuses before anything announces. And logging the store choice would have printed the resolved DSN,
  password included, undoing V9 in the artefact most likely to be pasted into an issue.

  It also found that `RG-4`'s store could not be driven from a node at all, which its own synchronous
  tests could not see: the blocking client builds a runtime and `block_on`s it, so the first real call
  panicked with "Cannot start a runtime from within a runtime", and `block_in_place` turned that into a
  connection nobody was driving rather than into an error. It is opened on a thread tokio has never
  touched, behind a `BlockingStore` adapter; the honest fix is `tokio-postgres`, which changes the trait
  for every backend and is its own story.

- **A node can be pointed at a shared location store** (`RG-12`). `RG-4` built the PostgreSQL
  location service and proved it satisfies the store contract; nothing made it reachable, because
  `driver::run` opened `InMemoryStore::new()` unconditionally. Two nodes were therefore two islands,
  each answering only for whoever happened to reach it. `NodeConfig` now carries a `StoreChoice` and
  the driver opens what it names.

  A configured store that cannot be reached **stops the node**, connected eagerly at startup rather
  than lazily on the first `REGISTER`. Falling back to memory would produce a registrar that comes up
  healthy, answers `200` to everything, and serves bindings no peer can see — with nothing saying so.

  The failing-first test demonstrates both halves rather than asserting one: it opens two in-process
  stores, writes one, reads the other, and requires the second to be **empty**; if that ever passes,
  the cross-node half has stopped discriminating. It then writes through one PostgreSQL handle, reads
  it back through a second independently-opened one, and checks a stale-revision write is refused
  rather than merged. Run against a real database, not skipped.

### Known gaps

- **Authentication is accepted and not applied.** The document's `tenant[].auth` section loads without
  error and changes nothing, so the node is an open registrar regardless of what the document says.
  Eighteen sections of the schema are in that state; the node logs which ones at startup. Making it
  refuse a document it cannot honour, rather than accept it silently, is the `fail-closed-config` epic.
- **One address in front of the cluster does not work.** Each node record-routes its own advertised
  address, so in-dialog requests must return to the node that forwarded them. A single Service or VIP
  will send a `BYE` to whichever node the balancer picks. Affinity tokens are what fix this.
- **The config loader pulls `unsafe-libyaml` into the tree**, through `serde_yaml_ng`. Non-negotiable
  #3 forbids `unsafe`, and the workspace lint enforces it for crates *in* this workspace, so the gate
  passed without comment. The exposure is an operator-supplied document rather than network input,
  which is the weaker of the two, but the property is stated about the platform and this weakens it.
  Undecided on purpose; a pure-safe-Rust parser behind the same value-tree walk is the alternative.

### Fixed

- **The doc gate no longer depends on what happens to be sitting under the repository** (`CF-10`).
  `check-docs.py` chose its files with `ROOT.rglob("*.md")`, so it descended into
  `.claude/worktrees/agent-*/` — full checkouts of this repository. It saw **1331** markdown files
  against **169** tracked. The count was meaningless, and worse, the verdict depended on transient
  state: a sibling agent's half-written doc could turn the gate red on a diff that never touched it,
  and a second copy of a file could resolve a link that was broken in the one that mattered. The
  file set now comes from `git ls-files`, because the walk needed a skip-list and a skip-list is a
  denylist that grows an entry every time a tool invents a directory. If git cannot run it exits
  loudly rather than falling back — a gate that checks nothing because it could not decide what to
  check is a failure this file has already shipped once.
- **The link checker no longer reads code samples as links** (`CF-10`). Fenced blocks and inline
  spans are stripped before links are looked for. This was found the only way it could be: quoting
  a real "broken link …" error message inside the story documenting the defect made the gate fail on
  that story. Indented blocks are deliberately left alone, since four-space indentation is also how
  this repository wraps continuation lines that do carry real links.

## [0.10.0] — 2026-07-29

One change, and it is about who the documentation is for. Until now the published site was a
verbatim view of `docs/`: the docs plugin read `../docs` and excluded only the story board and the
archive. Everything else went out — the roadmap's milestone tables, thirteen design records of
which nine still said `proposed`, ten normative specs, a generated conformance report — and the
landing page still told visitors that **nothing forwards a SIP message yet**, which M1 disproved
three releases ago. There was no install page, no quickstart, no configuration guide and no CLI
reference. A stranger evaluating this project was handed our planning material and none of the
answers.

There are now two documentation trees. `docs/` is internal and is published nowhere.
`website/docs/` is twenty-two hand-authored end-user pages, and the site is only that.

### Added

- **The end-user documentation site** (`DX-1` … `DX-11`), a ladder rather than a pile: what this
  is, a first forwarded call, guides for what actually ships, then clustering and operations marked
  `(preview)`, migration concept maps, and reference. Unshipped sections carry their status in the
  sidebar label *and* restate it in their own words, against a closed five-value vocabulary. The
  whole ladder was authored up front rather than grown, so no URL has to move when a preview
  section fills in.
- **Getting started reaches a forwarded call with no third-party tooling** (`DX-3`) — a Rust
  toolchain and standard-library Python. The two facts that actually bite a new operator are on the
  first page rather than in a footnote: the shipped binary is an **open registrar**, and bindings
  live **in memory only**, both because the configuration surface is three flags.
- **A CLI reference built by running the binary** (`DX-5`). Every flag, refusal message and exit
  code on the page was produced and pasted, including the three only a wrong invocation reaches. It
  found one: `run --version` prints `--version needs a value`, not "unknown option", because `run`
  parses flag-and-value pairs and checks for the value first.
- **A conformance page that publishes the method and refuses to copy the table** (`DX-6`). The
  generated report is linked, not duplicated, so the numbers cannot drift into disagreement with a
  claim — and the page says outright that six of ten specs are not registered with the vector
  checker, so the report is silent rather than reassuring about them.

### Changed

- **`docs/` is no longer published**, and both trees say so — `docs/README.md`, `AGENTS.md` and the
  site's own "public docs vs project docs" section. Site pages reach specs, designs and the roadmap
  by absolute GitHub URL.
- **The doc gate was inverted rather than dropped** (`DX-1`). Its third check read the `exclude`
  globs out of `docusaurus.config.js` and forbade a published doc from linking into an excluded one.
  With that key gone `site_excludes()` returns `[]` and the check returned `[]` with it — **green
  because it stopped looking**, which is worse than no gate. The rule now runs the other way: no
  page under `website/docs/` may relative-link out of the published tree, and the failure names the
  GitHub URL to use instead. Proved by making that link and watching it go red.
  `onBrokenMarkdownLinks` was also raised from `warn` to `throw`.
- The published pages carry **no internal work-tracking identifiers**. They cite RFCs, the
  normative specs and honest gaps; the backlog is not the reader's problem.

### Fixed

- **The 0.9.0 site deploy is unblocked.** `asserted-identity.md` used bare `<br>` inside table
  cells; MDX v3 parses HTML as JSX, so an unclosed tag is a hard error — *"Expected a closing tag
  for `<br>` before the end of `tableData`"*. The `website` workflow failed on both the push to
  `main` and the `v0.9.0` tag while `ci` and `docs` stayed green, because nothing between the doc
  gate and the release checked that the site still builds. All 14 are now `<br/>`.
- **`--help` no longer contradicts the software.** It claimed "No roles are implemented yet",
  stale since M1, and never documented `--tenant`. Nothing asserted the string, which is why it
  survived four releases.

### Known gaps

- `check-docs.py` walks whatever sits under the repository root, including agent worktrees — it
  reported 1328 markdown files during this work instead of 166, so a sibling checkout can turn the
  gate red for reasons unrelated to the diff. Already filed as `CF-10`.
- Nothing yet fails when an authored page is absent from `website/sidebars.js`: it builds fine and
  is reachable by nobody. `DX-12` is filed for that, and it supersedes the site half of `CF-11`,
  whose two "unreachable specs" are no longer published at all.
- Preview pages age into lies the day the thing they describe ships. Closing an `AF`, `RT`, `ME` or
  `KO` story now has to include the page that says it does not exist yet.

## [0.9.0] — 2026-07-29

Three specifications, and a theme that is really one question: **does the document say what the
system does?** Two of the three were returned for rework before they were allowed in, and both
times the defect was the same shape — a contract that claimed more than it delivered, in prose
confident enough that only reading it against the RFC caught it.

`RT-7` is the one worth reading. It went two rounds. The first draft declared the `user` privacy
level *performable*, and performed one of the eleven header treatments RFC 5379 §4.1 Table 1 asks
for under that level. Nothing was wrong in a way a test would catch — there is no implementation
yet — but a deployment reading that spec would have advertised a privacy guarantee to a caller and
delivered a third of it. The fix was not to weaken the claim: `A33` now enumerates the whole column,
nine performed and two declined **with reasons**, and `A35` performs the response-side deletions
that Table 1 marks reachable on a response. The second round also removed a false statement the
*first* round introduced — pointing operators at `RT-5`'s egress allowlist for a guarantee it
cannot give, since that allowlist is scoped to application-prefixed headers and
`P-Asserted-Identity` is not one. `§14` now records that gap as a gap, and `RT-11` asks whether the
platform should close it at all.

`DP-1` replaces four disagreeing config dialects with one schema, reconciled against the Helm chart
and the node rather than invented beside them — and found, in the process, that the chart's shipped
defaults declare a media policy the platform would refuse to boot on (`KO-14`). `EX-8` turns the
async query declaration into normative text and brings the `HF` family under the vector gate.

Nothing here is executable. All three are specifications, and the honest caveat is that the vector
gate cannot yet see most of what they wrote: `AI` (97 rows) and `CC` (48) join `LS`, `MR`, `NN`,
`AT` and `FR` as families the checker has no registration for, so roughly 145 normative rows are
unenforced prose. That was demonstrated rather than assumed — a fabricated row passes the gate
untouched — and `CF-8` is now `ready` at priority 1 to close it.

### Added

- **Asserted identity is a per-trunk policy, and privacy is performed or declined** (`RT-7`) —
  [asserted-identity](docs/specs/asserted-identity.md): `TrunkIdentityPolicy { trust, assert,
  privacy }`, rules `A1`–`A35`, gates `G-A1`–`G-A8` and 97 `AI-*` vector rows, covering
  `P-Asserted-Identity` (RFC 3325), `Privacy` (RFC 3323), trust domains (RFC 3324) and the
  anonymous-`From` form (RFC 5379 §5.1.4). The emission gate is total over trust × privacy: a
  three-valued `PaiRequest { Withhold, Preserve, Unspecified }` derived from the raw header, so an
  unregistered priv-value cannot bypass a deployment's declared fail-closed posture.
  The story took **two rework rounds**, and the finding that drove both is worth stating: the spec
  declared the `user` privacy level *performable* while performing part of it. RFC 5379 §4.1
  Table 1's `user` column has eleven entries; the first draft handled one. `A33` now enumerates the
  column — nine performed, two declined **with reasons** (`Call-ID` is a `MAY` that §5.3 puts behind
  a B2BUA; `Referred-By` carries Table 1's circumstance asterisk) — and `A35` performs the five
  response-side deletions, gated on the *response's own* `Privacy` header, because a `Server` or
  `Warning` header names the answering party rather than the caller. Declining a level is honest;
  advertising it and performing a third of it is not.
  Two things are deliberately **not** claimed: `Privacy: header` (`A29` refuses it rather than
  half-performing it — RFC 3323 §5.1 puts it in a B2BUA), and any guarantee that a carrier never
  receives a `P-Asserted-Identity`. Nothing in the platform provides the latter today; §14 records
  it as a known gap and `RT-11` asks whether it should exist at all, since it would mean overriding
  a caller's explicit `Privacy: none` against RFC 3323 §4.2's `MUST NOT`.
- **One config schema, not a fourth dialect** (`DP-1`) — [cluster-config](docs/specs/cluster-config.md):
  role selection `R1`–`R7`, the error contract `V1`–`V10` (report *all* errors rather than the
  first; closed world; refusal is the only failure mode), the reloadable subset with drain-then-switch
  for the shard map, and 48 `CC-*` vector rows. Written as a **reconciliation** of the three
  artefacts that already asserted a schema — the chart's `cluster:` tree, the node's provisional
  `NodeConfig`, and the specs that fix vocabulary — rather than invented beside them. Where the
  chart and a spec disagreed the spec won, and the differences are tabulated in
  [deployment](docs/designs/deployment.md): `numbering` becomes named `normalisation` profiles bound
  per scope and per trunk, per-tenant registrar values move to `tenant[]`, `shards` becomes a
  `shardMap` that can express ownership, and a top-level `quirkProfile` contradicts `EX-10`'s rule
  that bindings bind. `security.maxForwards` becomes **70**: RFC 3261 §16.6 step 3 makes it the
  value inserted when a request carries none, not a hop budget, so 10 silently shortened every path
  arriving without the header.
- **The async query declaration is a contract** (`EX-8`) — carried `EX-6`'s accepted design into
  [hook-framework](docs/specs/hook-framework.md) as normative text: `QueryOutcome` and `Disposition`
  closed, `E4a`/`E4b` pinning that `Query` is the last effect of its invocation and that the outcome
  is decided exactly once, `E7` making `QueryDeadline` engine-owned (because `E6` forbids a *module*
  timer from altering a transaction's outcome and this one does), and `G7`/`G8` joining the startup
  rules with `G1`–`G6`'s fails-deployment-never-a-call discipline. Registering the `HF` prefix
  brought the whole family into the vector gate's view: **77 of 98 rows proved, 21 deferred**, each
  with its own reason rather than a group waiver.

## [0.8.0] — 2026-07-29

Three stories, all of them consequences of the last release rather than new ground: the two defects
`0.7.0`'s own review left filed, and the build claim that building `0.7.0`'s container image
exposed. The theme is the same one — a statement the project had been making without checking.

The MSRV is the consumer-visible part. It moves 1.88 → 1.94, and it moves because the old number
was never true, not because anything got stricter.

### Changed

- **The minimum supported Rust version is 1.94** (`CF-9`), up from a declared 1.88 that never
  worked. This is a corrected claim rather than a new restriction: on 1.88 the workspace did not
  compile at all, so nothing that previously built stops building. The floor was established by
  bisecting — 1.88, 1.92 and 1.93 fail; 1.94 and 1.95 pass — rather than by reasoning about it.
  The constraint is the kernel's, not ours: `sipx-transport`'s `impl<K, I> Default for
  TimerQueue<K, I>` calls `BinaryHeap::new()` on an unbounded type parameter, and rustc relaxed
  `BinaryHeap::<T>::new`'s `T: Ord` bound in 1.94 — so every consumer inherits 1.94 for the sake
  of a `Default` impl. Filed as a row in [upstream](docs/upstream.md); bounding it there would let
  both projects go back down.
- `scripts/check-msrv.sh` now builds the workspace on the declared floor, and both `gate.sh` and a
  parallel CI job run it, so the number and the truth cannot drift apart again silently. It reads
  the floor out of `Cargo.toml`, so the version is never written twice. A warm run costs 0.08s.
- Raising the floor un-gated 23 `duration_suboptimal_units` lints — clippy reads `rust-version` as
  its MSRV and suppresses lints whose suggested API is newer. Three sites became `from_hours(1)`;
  twenty carry a **targeted** allow with a one-line reason, because SIP measures time in seconds
  and these values are checked against specs and vectors that state seconds — `proxy-behavior`
  §F11 says "Default 180 s", so `Timer C` stays `from_secs(180)`. The lint is deliberately **not**
  allowed workspace-wide: that would blind the same check at every future floor raise, which is
  the inverse of this story's purpose.

### Fixed

- **A contested target is resolved where the contest is** (`EX-10`). `overrides` — the only
  construct that decides which of two quirk profiles wins when both write the same thing — was
  required by `G10`, asserted by a vector, cited in Alternatives, and present in **no schema**. It
  is now declared at the **binding**, not in the profile: a profile does not know where it applies,
  so it cannot know which profiles it composes with, and a semver'd catalogue entry cannot name one
  deployment's other profiles. `target` and `winner` are separate fields, because a profile id and
  a target are two things — which is why the old single-list form could not be made coherent in
  either reading. New rule `G13` makes an override self-invalidating: the target must be contested
  *now* and the winner must be one of the contesters, so an override that outlives its contest
  fails the boot instead of silently doing nothing. Overrides apply at composition, at startup, so
  the set reaching the hook phases really is disjoint and the design's idempotence and confluence
  claims keep their premise. `QP-G-15` pins that the escape cannot silence a boot check: it deletes
  rules, never assertions.

### Added

- **`RA-R-7` is proved** (`RG-11`) — 77 of 85 rows, 8 deferred. It shipped in 0.7.0 asserted but
  unproved, because `RG-10` was a documentation pass. The test drives the replayed-credential
  admission path and location-service §W3's wildcard removal as **one request**, where each half
  had a test and nothing drove them together. It asserts the store rather than the status — a `200`
  would be equally true of the `400` path and of a no-op — and asserts that two commits happened,
  which is what separates "every binding was removed" from "there was never anything to remove".
  It pins an exposure `registrar-auth` §7.3 accepts on purpose, and its name says so.

## [0.7.0] — 2026-07-29

Nine stories: one behaviour change, four specifications, three corrections that independent review
found in those specifications, and the first tooling that runs any of it outside a test.

The behaviour change is the one that matters for anyone trying to run this. Until now a node
advertised the address it **bound**, so a container or a cloud host binding `0.0.0.0` told every
peer to answer `0.0.0.0`. That is also why the tooling and the fix arrive together: the container
image exists in this release because `DP-5` made a containerised node reachable, and building that
image is what proved the platform does not compile on its own declared `rust-version`.

The three corrections are worth reading as a group. Each specification was reviewed after merge by
a context that had not written it, and each review found the same *kind* of defect — a claim
stronger than the mechanism behind it. A digest section that called an exposure additive when it
can empty an address of record; a totality rule that did not hold for a leading `+`; a seam
written against a type its counterpart never landed. None was caught by the gate, because a gate
checks that references resolve, not that sentences are true.

### Added

- **A listener binds one address and advertises another** (`DP-5`) — `Listener { transport, bind,
  advertise }`, validated as a set, with `sent_by()`, `record_route_uri()` and `contact_uri()` as
  pure functions of it. The advertised address now reaches **`Via`** (RFC 3261 §18.1.1), not only
  `Record-Route`: the driver maps onto the kernel's own `bind` vs `sent_by`/`sent_by_port` split
  rather than shadowing it. `Record-Route` is built from the listener a request *arrived on*,
  because a node may advertise one address on UDP and another on TLS. The identity set is every
  listener's advertised host, so any edge still recognises any edge's `Route`.
- **A node that would advertise an unspecified address refuses to start.** `sipx-clstr run --listen
  0.0.0.0:5060` with no `--advertise` now exits 2 naming the address, where it previously started
  and advertised `0.0.0.0`. A user-visible break in the CLI's default path, and deliberate: the
  failure it replaces was silent and only visible from the far end.
- **Carrier quirks are a bounded profile, not a scripting hook** (`EX-7`) — a closed op vocabulary
  over a catalogue of headers and SDP fields, with composition, an anti-catalogue naming what is
  deliberately inexpressible, and 22 vectors.
- **Number normalisation is a closed rule vocabulary** (`RT-6`) — a new
  [spec](docs/specs/number-normalisation.md): profiles, four transforms, digit guards with
  fallback, a stated termination bound, and 45 vectors.
- **Codec and SRTP are a declared per-trunk policy** (`ME-6`) — media-relay §13:
  `TrunkMediaPolicy { codecs, transcode, srtp }`, twelve rules, six startup checks, the policy →
  NG-key mapping, and 24 vectors including three byte-exact blocks. The rule that ties it to
  `EX-7`: a quirk profile may **require** an SRTP mode and may never **assign** one, or SRTP
  selection would go back to being a consequence of which pattern matched.

### Changed

- **What digest authentication actually protects is written down** (`RG-9`) — registrar-auth §7.
  Under `qop=auth` the digest covers method and Request-URI and **not** `Contact`, `CSeq`,
  `Call-ID`, `Expires` or the body (RFC 7616 §3.4.3), so a captured `Authorization` reattached to a
  REGISTER differing in those fields hashes identically and is accepted. Verified against the
  pinned kernel rather than inferred. The decision is to **accept**, bounded by the nonce lifetime,
  with each rejected alternative recorded and reasoned. No production code changed — the mechanism
  was never wrong, only the description of it. `RA-R-6` pins the behaviour twice, including what
  ends up in the location store.

### Fixed

Every defect independent review found in the four specifications above was fixed before this
release rather than shipped documented — the specs and the corrections land together.

- **Registrar auth §7 stated its own exposure too weakly** (`RG-10`). §7.2 described the accepted
  digest replay as strictly *additive* — the attacker's contact joins the victim's, "the phone that
  owns the AoR keeps ringing". That is false for one variant. The digest binds neither `Contact`
  nor `Expires` nor `Call-ID`, so the same captured `Authorization`, reattached to a REGISTER
  carrying `Contact: *`, `Expires: 0` and a fresh `Call-ID`, takes
  [location-service](docs/specs/location-service.md) §W3's **removal** branch instead:
  `wildcard()` rejects only when the `Call-ID` matches and the `CSeq` does not advance, which a
  fresh `Call-ID` makes false for every stored binding. Every binding on the AoR goes in one
  request — loss of service, not unwanted company, and RFC 3261 §26.1.1's registration hijacking
  exactly. §7.2 now names it, §7.3 re-argues the accept decision against the corrected impact
  rather than carrying the fork-only argument over, and the operator-facing sentence says the thing
  an operator can act on. `RA-R-7` pins it, registered **deferred** rather than claimed proved —
  this was a documentation pass and no test drives the two halves together; `RG-11` closes that.
- **Number normalisation was not total, though it said it was** (`RT-10`). `N12` claimed every
  transform maps a digit form to a digit form or leaves it unchanged; the transforms were undefined
  for an input carrying a leading `+`, so `add_prefix: "0"` on `+4930` produced `0+4930` and
  `add_prefix: "+"` produced `++4930` — neither a digit form nor unchanged. `+` is
  `user-unreserved`, so both serialise into a Request-URI and leave the platform. §5 now defines
  `plus(x)` and `digits(x)` and states `T2`–`T4` in terms of them: prepending targets the digit run
  and a leading `+` is never duplicated, displaced or counted as a zero. `N30` restates the totality
  claim in the form that holds. Also settled, each as a **rule** rather than as a vector asserting
  an outcome nothing produces: `N31`, a guard on an `Absent` or `NotANumber` field never holds (the
  counterpart to `N10` for transforms); and `N32`, substitution rewrites an existing URI and never
  creates a field — so a guard on an absent `P-Asserted-Identity` skips rather than fabricating one,
  which keeps `RT-7`'s scope out of this spec. `Literal` now admits the empty value its own shipped
  profiles were already using. Seven new vectors, `NN-T-9`…`12` and `NN-G-9`…`11`.
- Two citations in that spec were repaired rather than trusted: RFC 3261 §17.1.1.3 takes the ACK's
  `To` from the **response being acknowledged**, not the request, so `To` did not belong in the
  byte-for-byte list — corrected here and in [routing-trunks](docs/designs/routing-trunks.md), which
  had copied it. And the E.164 claims now cite §6.1 and §6.3 separately, where one §6.2.1 reference
  had been carrying both and covering only geographic numbers. `N21`'s termination bound was
  miscounted as four field-instances; `N4` allows `P-Asserted-Identity` two, so it is five — 25
  steps, still finite and constant.
- **The quirk-profile media seam named a type that does not exist** (`EX-9`). `EX-7` and `ME-6`
  were written concurrently and agreed on the direction of their seam but not its vocabulary:
  `MediaAssertion` was declared as `Srtp(SrtpMode)` with a `Required` variant, while `ME-6` landed
  `SrtpPolicy` — `Disabled`, `Sdes { suites }`, `DtlsSrtp { role }` — with no required/optional
  axis at all. `MediaAssertion` is now `{ Srtp }`, satisfied by any variant but `Disabled`. A
  mechanism-specific assertion was considered and **rejected**: it would let a profile select the
  keying method by which profile matched, which is the defect `ME-6` exists to remove. The
  duplicated `G-M5`/`G11` startup check collapses to one rule with one error content, `MP11` is
  cited where the direction is stated, and new rule `G12` (vector `QP-G-11`) forbids a domain-bound
  media assertion, which had been undefined rather than decided.

### Added — tooling

- **The platform runs outside a test** (`KO-13`) — a `Dockerfile` and a devspace profile that stand
  a node up and prove it: two users register and the proxy forwards an INVITE between them, scripted
  in `scripts/sip_demo.py` over raw UDP with nothing but the standard library. Deliberately **not**
  the operator path — `KO-1` has not pinned the custom resource and `KO-3` has not implemented the
  reconcile loop, so `deploy/helm/` still renders a `SipxCluster` nothing serves. The `Record-Route`
  it produces carries the *advertised* address, which is `DP-5` demonstrating itself outside its own
  tests.

### Known gaps

- `ME-6`'s fourth acceptance item is **specified, not executed**: the assertion is pinned as vector
  `MR-P-1` against a byte-exact block, but the `MediaRelay` types are a spec contract and `ME-2` is
  the story that lands the Rust. The box is deliberately left unticked.
- `KO-13`'s fourth acceptance item — the run against a node **scheduled in Kubernetes** — is
  likewise unticked. The image and manifests are proved in containers; the pod stays `Pending`
  behind a host disk-pressure taint. Tolerating that taint was tried and reverted: the taint and the
  eviction manager are separate mechanisms, so it schedules the pod and kubelet evicts it at once.
  The manifest records that where the toleration would go.
- The workspace does not build on its own declared `rust-version = "1.88"` (`CF-9`) — the kernel's
  `sipx-transport` needs a `BinaryHeap` bound relaxed in a later release. Unfixed here because the
  real floor wants establishing by building rather than by reasoning.

## [0.6.0] — 2026-07-29

M1's one known defect is closed, and M2's two defining subsystems — cluster affinity and media
control — now have specifications to build against. Five stories, of which one changes behaviour and
four are the specs and decisions that let the implementation stories start.

### Fixed

- **A retransmitted REGISTER is a retry again, not a `500`** (`RG-8`) — M1's one known defect, and
  the plainest event on a UDP network reached it. [location-service](docs/specs/location-service.md)
  §5.3's B4 tested idempotency against an **absolute deadline**, so it held only for a retry
  arriving in the same nanosecond as the original; a phone retransmitting half a second later after
  a lost `200` fell through to B5 and was refused. §5.3.1 now makes the **granted duration** the
  normative base — `refreshed_at → expires_at` compared against the lifetime this command grants —
  and `process::already_holds` follows it.
- The rejected reading is recorded in the spec with its reason, because the next reader will ask:
  carrying the originating `now` with the command cannot answer the case the story exists for. A UA
  retransmission arrives as fresh bytes and carries no field of ours, so the edge stamps its own
  `now` and the two deliveries differ again — the defect would have survived the fix while
  `RegisterCommand` and every re-presenter changed. Comparing durations also answers the cluster
  case for free, since a second node computing the same duration agrees.
- **B4.3 says how far the no-mutation guarantee reaches**, which the first draft of §5.3.1 got wrong
  by over-promising. B1–B5 are decided **per contact against the binding it matches**, so a request
  that replays a spent token *and* carries a contact matching no stored binding still adds it and
  commits at a bumped revision. That is not a widening: the same UA can send a request carrying only
  the new contact, which B1 accepts identically, so the ordering token never gated an addition —
  what bounds additions is the authenticated principal (§5.1 S3/S4) and the per-tenant quota (§5.5).
  Vector `LS-R-23` pins it, and §5.3's "a no-op rather than a second write" is now qualified with
  *"to the bindings that token wrote"*.
- A binding is still never **extended** by a retry (B4.2): a retry that refreshed the deadline would
  make one ordering token spendable more than once. `LS-R-3` now states the 500 ms elapsed between
  the original and the retry, so "identical outcome" stops being satisfiable only by a zero-latency
  retry, and `LS-R-22`'s cross-backend check now asserts the revision did not move rather than only
  the status.

### Added

- **Flow references and connection ownership are specified** (`AF-2`,
  [affinity-token](docs/specs/affinity-token.md) §11–§14). A `flow_ref` is a signed, AEAD-encrypted
  reference to one client connection — node, incarnation, table slot, generation, transport and
  tenant — stored in a location binding, so a mid-dialog request finds the socket its client is on
  without a cluster-wide lookup. Fixed at 49 B encrypted / 45 B authenticated-only, sharing the
  14-byte header, key set and AEAD call site `AF-1` already fixed rather than duplicating them, with
  21 byte-exact vectors.
- **A reference carries no expiry, deliberately** — it names an object that exists, and that
  object's death is a tighter bound than any clock, needing no clock to enforce. Slot reuse is
  closed by a generation bump on reconnect and by an `incarnation` (the owner's boot second) that
  the design's original `signed(node_id, connection_id, generation)` sketch did not have: without
  it a restarted node re-issues tuples a live reference already names.
- **That choice reaches back into key rotation.** Because a reference leaves circulation only when
  the binding holding it refreshes, §6's rotation step K4 **and** the mint-key window now cover
  `max(L, E_max) + S`, where `E_max` is the tenant's maximum registration expiry. The second is an
  independent defect the first would not have caught: with `E_max > L` a single configuration and
  **no rotation at all** would let a mint key expire out from under references it minted.
- **A binding must not outlive the flow it names** (`BI6`). The idle timer closes a connection at
  `T_idle`, but E5 grants a registration up to the tenant maximum — and a registered client is idle
  *by design*, since refreshing is the only traffic a registration generates until RFC 5626
  keepalives arrive. So an entirely default deployment would close the connection and leave the
  binding pointing at it for ~23 hours, answering `480` for every call toward that client. The fix
  clamps the granted expiry to `T_idle − M` for connection-bound registrations rather than inflating
  the timer past a day, which would stop it being a resource bound at all.
- **`MediaRelay` and the rtpengine NG adapter contract are pinned** (`ME-1`,
  [media-relay](docs/specs/media-relay.md), 573 lines). The platform's only view of media is a
  trait — offer/answer/update/delete/query with a state table and a `NullMediaRelay` whose
  pass-through behaviour is specified rather than assumed — and everything version-specific about
  the integration target sits behind it. **SDP stays opaque bytes end to end**, which is the
  load-bearing choice: it means neither this repo nor the kernel needs an SDP model to build `ME-2`.
- **The NG binding is specified to the byte.** Framing, cookie, a canonical bencode encoder whose
  dictionary keys are emitted in ascending raw-byte order, the command set, a timer budget, the
  error taxonomy and the health signals — with vector tables covering the trait, the null relay,
  the encoding, the exchange, faults and health. Sorted keys make the encoding a *function* of the
  value, which is what lets `ME-2` assert byte equality instead of semantic equivalence.
- **Version drift is a contract, not a hope.** The decoder must not assume sorted keys, must ignore
  unknown ones, and accepts both spellings of the `query` timestamp key. A node lacking the
  `load limit` extension degrades to a rejection rather than breaking. The tested baseline is
  rtpengine **`mr13.0.1.10`** (series `mr13.0`), matching the series `deploy/helm/values.yaml`
  already pins, so chart, CI container and vector bytes name one version; "baseline" is defined as
  *tested*, not *minimum*, and version-string detection is forbidden.
- **The external routing consult becomes a suspension, not a blocking call** (`EX-6`, design
  accepted in [extension-framework](docs/designs/extension-framework.md)). A deployment currently
  picks the egress pool and asserted caller identity with a synchronous HTTP lookup on the INVITE
  path, which breaks non-negotiable 2 twice — the forwarding decision reads a socket, and it
  therefore cannot run under the deterministic harness at all. What replaces it is **nothing new in
  the pipeline**: `EX-1` already specified `Query` as a suspension, so the consult lands at H7
  `BeforeTargetResolution` as an ordinary module, and the thing that was special about it — the
  blocking call — stops existing. H8 and H9 read the answer as a published request-scoped fact and
  never query, because H9 fires per branch and querying there would multiply external load by the
  fork width.
- **Timeout and failure handling become data.** `QueryDecl` gains `timeout`, `retries`, `on`,
  `defaults`, `cache` and `limits`, where `on` is a **total** map over a closed outcome enum. Two
  new startup rules carry the decision: G7 (every outcome has an arm; per-query and per-request
  budgets hold) and G8 (every `Proceed` names a startup-validated default; every `Reject` is in the
  closed set `{403, 404, 480, 500, 503}`). **6xx is forbidden** — RFC 3261 §21.6 makes it a claim
  about the user everywhere, and §16.7 makes an upstream forking proxy cancel every other branch on
  one; a routing-policy failure cannot know that.
- **Fail-closed by construction, and fail-open only when declared in advance.** A missing outcome
  arm is a *startup* error, never a runtime fallback — a framework-wide "on error, continue" is the
  coded policy `EX-6` exists to delete, moved one level up and made invisible. This deliberately
  tightens the deployment's current behaviour, where a 5xx from the oracle is tolerated: that
  tolerance stays available, but as `Proceed(fallback_pool)` in a manifest, checked at startup and
  covered by a vector.
- **The transaction's timers and capacity are decoupled, testably.** No engine timer's duration or
  arming point is a function of query latency: the request is promoted to stateful, so the INVITE
  server transaction emits `100 (Trying)` and absorbs retransmissions while suspended, and Timer C
  is armed after `Forward` — at `t_forward + 180 s`, never `t_invite + 180 s`. Capacity is bounded
  by a declared in-flight cap, a queue sized **0** (shed rather than queue), a per-node breaker that
  costs nothing while open, and a cache that follows RFC 2308 §5's distinction — "no route" is
  cacheable, "could not ask" never is. Eleven named harness scenarios cover slow, timing-out,
  failing, flapping, malformed and CANCEL-during-suspension, all in virtual time.
- Follow-up filed as `EX-8`: the design is accepted, but `docs/specs/hook-framework.md` does not yet
  say it. *Considered for upstream: no, cluster-specific* — the declaration surface is bound to this
  platform's manifest and pipeline; the two protocol-generic pieces it leans on are already in the
  kernel and are consumed rather than re-made.
- **Topology hiding: decided out of scope for v1** (`PX-8`, decision recorded in
  [proxy-engine](docs/designs/proxy-engine.md)). The finding that settles it is that there is
  nothing of the platform's own topology on the wire to hide: a surface-by-surface reading shows
  the cluster contributes exactly **one** Via — an advertised identity, never a bind address or a
  node name (`DP-5`) — and **one** Record-Route pair naming a cluster-wide service identity
  ([affinity-token](docs/specs/affinity-token.md) §5), with shard, edge and media ids living only
  as ciphertext inside the token body (§3, §4). One Via is a north-star property rather than a
  convenience — a cluster that contributed several would be distinguishable from one correct proxy
  by reading the message — so the decision rests on a commitment the platform already holds.
- The **rejected** option is recorded with its reason: the per-dialog header store that strips Via
  and Record-Route, persists them, and restores them on each later message. It is the hot-path
  dialog lookup non-negotiable 5 forbids by definition, and keyed by the minting node it *is* the
  non-interchangeable-replica defect the story was filed against. RFC 3323 §5.1 describes that
  construction and concedes it "requires the privacy service to keep a pretty significant amount of
  state on a per-dialog basis" — the citation argues against itself.
- The **unbuilt but admissible** option is written down so a reversal inherits it rather than
  rediscovering it: RFC 3323 §5.1's own stateless alternative — removed Via entries encrypted under
  the token key family into a `via-extension` parameter of our own Via, returned verbatim per
  RFC 3261 §8.2.6.2 and restorable by any node holding the cluster key. Not built in v1 because it
  enlarges the message it exists to shrink (RFC 3261 §18.1.1), it cannot be a hook module
  (hook-framework §3 keeps Via engine-internal), and Record-Route privacy has no restorable form in
  the direction usually wanted.
- What remains of the demand is per-carrier RFC 3323 header privacy, and it is already owned:
  `RT-5` (egress header allowlist) and `RT-7` (`Privacy`/`P-Asserted-Identity` policy). *Considered
  for upstream: no* — trust-boundary policy over cluster-owned identities and keys is orchestration;
  the one protocol-generic piece either mechanism would need landed as sipx `S-15` (`PX-3`).

## [0.5.1] — 2026-07-29

### Fixed

- **The documentation site could not deploy**, and had not since `v0.4.0`. `docs/designs/cluster-viz.md`
  relative-linked into five story files; `docs/roadmap.md` picked up a sixth in `0.5.0`. Those paths
  resolve perfectly on disk — which is why the gate passed them — but `docusaurus.config.js`
  excludes `stories/**` from the published site, so each one is a broken link there, and Docusaurus
  fails the build rather than shipping one. Rewritten as absolute GitHub URLs, which is what the
  rest of `docs/` already did.
- **The gate had the matching hole**, which is the actual defect: a check that says "every relative
  link resolves" is not the same as "every published page resolves", and the difference is exactly
  the set of files the site excludes. `scripts/check-docs.py` gained that third rule, and it reads
  the exclude globs **from the site config** rather than restating them — a check that keeps its own
  copy of the list is one that eventually disagrees with the thing it checks. Per AGENTS.md, green
  locally and red in CI is a bug in the gate, so it was fixed there rather than worked around.

## [0.5.0] — 2026-07-29

### Added

- **M1 is complete** (`RG-2`, the fourteenth story) — **server-side digest, wired**. The decision
  core and its 22 `RA` vectors were already in; what landed here is the seam and the proof.
  - **`registrar::parse::admit` is where authentication happens**, and
    [registrar-auth](docs/specs/registrar-auth.md) §2's ordering is now structural rather than
    conventional: it runs `TenantAuth::decide` and builds a `RegisterCommand` **only** on
    `Proceed`, so a challenged request never exists as something `process` could store. It returns
    `Admission::{Command, Challenge, Reject}` rather than a `Result` — a challenge is not a
    failure, it is the first half of a round trip the client is expected to finish, and typing it
    as an error would put it on the same footing as a malformed message.
  - **`EdgeContext::principal` is removed.** An identity is not something an edge *knows*, it is
    what a decision *produced*; a settable field would have let a driver assert an identity nobody
    proved, which is the one mistake in this area no downstream test could catch. `admit` is now
    the only way a principal is ever attached, and `register_command` is explicitly the open-tenant
    path — it yields `principal: None`, which §3 A1 requires to be a *recorded* fact so the audit
    trail can say "unauthenticated" rather than fail to say anything.
  - **The command's tenant comes from the authenticator, not the edge.** §5 shapes the principal as
    `<tenant>:<username>` because a username is unique only within a tenant; sourcing the two from
    different places would let a misconfigured listener write tenant A's bindings under a principal
    naming tenant B. One source, so they cannot disagree.
  - **`Timestamp::as_secs`** is the clock seam — the location service counts nanoseconds and digest
    counts seconds. Truncating, deliberately: a nonce then expires a moment late rather than a
    moment early, so nobody is told `stale` for a nonce that was still good.
  - **The node driver can authenticate**, via `NodeConfig::auth` (realm, nonce secret, credentials),
    open by default — a default that quietly required credentials would make a node that answers
    nothing look like a node that is up. One `Mutex<TenantAuth>` per process, because the replay
    window is the thing it holds and a per-request authenticator is a window that never says no.
  - **`sipx-clstr-sim/tests/register_auth.rs`** — the harness scenario M1's fifth exit criterion
    asks for, in eight cases: the challenge/answer round trip, the principal reaching the stored
    binding, `RA-R-1`'s retransmission, `RA-R-2`'s forged twin, `RA-D-4`'s foreign realm refused
    `403`, a wrong password binding nothing, an open tenant recording `None`, and a byte-for-byte
    replay sweep under jitter. The phone's half is `sipx_ua::auth::respond` — the kernel's own
    client — so what it proves is that the two halves of digest agree, not that this file agrees
    with itself.

### Known issues

- **A retransmitted REGISTER authenticates and is then refused `500`** (`RG-8`, `ready`, priority
  1). The authentication half is correct — same nonce, same nonce-count, same digest, admitted
  twice, exactly as `RA-R-1` requires. What refuses it is
  [location-service](docs/specs/location-service.md) §5.3 B5, because B4's idempotency test reads
  "same granted expiry base" as the same **absolute deadline**, which is true only for a retry
  arriving in the very nanosecond of the original. Over UDP a lost `200` produces this every day.
  - `RG-3` had recorded the question and deferred it to the cluster stories, framed as a
    *re-presentation at a second node* stamping its own `now`. The harness reached it with one
    node, one phone and no cluster at all, which is what moved it from M2's problem to M1's.
  - Left unfixed here on purpose: §5.3 is normative, the reading changes there before the code
    does, and reversing a decision that is on the record belongs to the story that owns it.
    `a_retransmission_that_authenticates_is_still_refused_by_the_ordering_rule` pins the current
    behaviour meanwhile, so the defect is something a build fails on rather than a paragraph.

## [0.4.0] — 2026-07-29

### Added

- **M1's own end-to-end proof** (`CX-3`) — `scripts/e2e-call.sh` brings up one node and two real
  `sipx` CLI phones, registers both, places a call from one to the other over UDP, answers it and
  hangs up. Every step asserts on the CLI's `--json` output and its exit code. **Media is proved,
  not assumed**: alice plays a three-second 440 Hz tone and bob records 24 000 samples — exactly
  three seconds at 8 kHz — while the script asserts the node holds exactly *one* UDP socket, so the
  RTP bob heard came from alice and nowhere else. The CLI is deliberately not vendored: the value
  of the test is that the client side is somebody else's implementation of RFC 3261.
  - Four defects only a real network found: the node announced its address *before* binding, so a
    failed bind looked like a successful start; the exit code was swallowed by a pipe; the
    transaction gauge logged per completed request, so it could never observe the store draining
    after the last one; and `tracing`'s ANSI codes sat between a field name and its value, defeating
    the `grep` that read the gauge.
  - Two things that look like bugs and are not: the store takes ~32 s to drain, because RFC 3261
    keeps a concluded transaction alive for 64·T1 to absorb retransmissions — asserting "empty
    immediately" would be asserting a bug — and a contact naming a *host* rather than an address is
    not routable until `RT-1` brings RFC 3263 resolution.
- **The constellation's first feed** (`VZ-1`, [design](docs/designs/cluster-viz.md)) — the
  cluster, watchable: a dev adapter (`cargo run -p sipx-clstr-sim --example viz`) paces a seeded
  scenario against the wall clock and streams it live over SSE, and a canvas page renders the
  stage — messages as particles colored by method, drops/duplicates/breaks as visible faults,
  timers as sweeping rings, and the DP-3 invariant counters as a HUD that must not move. The
  stream is a serialization of the harness trace and nothing else: the frame vocabulary lives in
  the library (`sipx_clstr_sim::viz`) behind an exhaustive match, so a new trace variant breaks
  the build rather than rendering as nothing, and uninstrumented counters render as
  *uninstrumented* rather than as a pretend zero. std-only HTTP, localhost-bound, `serde_json`
  as a dev-dependency — no new runtime surface, and nothing a deployment could ship. The
  end-to-end smoke test (`cargo test -p sipx-clstr-sim viz_smoke`) spawns the real server and
  proves `/healthz`, the page, and the live frame stream — including backlog resync for a late
  client — and the runbook lives at `crates/sipx-clstr-sim/examples/viz/README.md`.
- **The Cargo workspace** (`CX-2`) — five crates drawn along the sans-IO boundary rather than by
  subject matter, because that boundary is what the deterministic harness depends on:
  `sipx-clstr-proxy` (RFC 3261 §16 forwarding), `-registrar` (bindings and REGISTER), `-sim` (the
  harness), `-probe` (the e2e-tester) and `-node` (drivers, roles, the `sipx-clstr` binary).
  **`tokio` is a dependency of `-node` and of nothing else** — the rule made mechanical, so a
  violation shows up as a dependency-graph change rather than as a code review someone has to do.
- **The kernel pinned to a released tag.** sipx is not on crates.io, so a git dependency is the
  only honest way to depend on it; `tag = "v0.2.1"` rather than a branch is what makes "which
  kernel version is this claim true of?" a question with an answer. `sipx-clstr --version` reports
  it, and a test asserts the constant it reports matches the tag the workspace actually pins —
  otherwise a bump with a forgotten constant leaves the binary confidently wrong about the one
  thing it is asked during an incident.
- **The gate** (`scripts/gate.sh`), which CI now runs step for step: fmt, clippy `-D warnings`,
  tests, the feature matrix, provenance, and documentation consistency.
  - `check-provenance.sh` carries **the integration carve-out**: `scripts/provenance-allow.txt`
    names interop targets that may appear anywhere, in the repository with a reason per line —
    not a contradiction, since a term we are willing to write down is by definition not one we
    refuse. Matching is exact rather than prefix, or one allowed name would silently permit a
    family of denied ones, and the script refuses to run if the allowlist swallows the whole
    denylist.
  - `check-features.sh` builds each crate with its optional features **off**. `--all-features`
    hides a crate that does not compile without one, and that stays invisible until it is in a
    release.
  - `check-docs.py` moves the documentation checks out of the CI workflow into a script both the
    workflow and `gate.sh` call, so there is one implementation instead of two that drift.

- **The echo endpoint** (`ET-3`), and with it **the first scenario in which every component is
  real** — the probe engine, the registrar, the forwarding core and the echo, with only a driver and
  a network supplied by the test. The separation §9 demands is structural rather than promised: a
  test reads the manifest and fails if this crate ever depends on the proxy, the registrar, the node
  crate or `tokio`. An unmarked call is refused `403` rather than `404`, because the
  address-of-record exists and the refusal is policy; `OPTIONS` is answered and unknown methods get
  `405`, because silence would make the echo look like the dead listener the probe exists to detect.
- **The probe engine and scheduler** (`ET-2`) — `sipx-clstr-probe`, 40 tests plus 5 harness
  scenarios, and **all 19 `EP-*` vectors proved**. No clock, socket or sleep in the crate: `now` is a
  parameter and jitter is an injected closure, so a failure scenario is a seed rather than a flake.
  One `ProbeRun` record, which `ET-4`'s API and `ET-5`'s metrics read rather than each inventing a
  view of it. The rate bound *defers and counts* rather than dropping — a skipped target is a blind
  spot, a delayed one is only late — and the matrix's first runs are spread across one interval,
  because a rollout starts many nodes at once.
- **The probe contract** (`ET-1`, [spec](docs/specs/e2e-probe.md)) — eleven normative sections and 19
  vectors, so the engine, the echo endpoint and the trigger API are derived from one contract rather
  than three opinions. `inconclusive` is why the verdict taxonomy is three-valued: a probe that could
  not run is not an outage, and conflating them trains operators to ignore the alert. A shed probe is
  its own cause, because silence means the listener is gone and shedding means the platform is
  protecting itself. The marker rides in `Subject`, which no intermediary has a reason to alter, so a
  marker that does not come back is evidence about the *path* — and it must not be derivable from the
  `Call-ID`, or something that merely saw the request go past could reflect it. Retries are explicitly
  not the probe's job: a probe that retried would mask the marginal loss it exists to report.
- **Decided: the echo is the same binary in `echo` mode**, a role rather than a service — one artifact
  to build, version and secure for something whose whole job is to answer `200` and copy a header. The
  constraint that made it a real question is absolute and is enforced at *load*: no proxy role ever
  links a UAS, so a configuration asking for both is refused where a human is still watching.
- **The `PostgreSQL` location store** (`RG-4`), verified against a real PostgreSQL 16. "Passes the
  identical suite" is now literally true: `run_location_store_suite` takes a `&dyn LocationStore` and
  both backends call that one function, because a suite copied per backend drifts and the copy that
  drifts is the one that stops catching things. Serializability is a **revision predicate**, not an
  isolation level, so K1 holds wherever a single statement is atomic; the first write is its own
  compare-and-swap, because a `SELECT` then `INSERT` would let two nodes that both think an
  address-of-record is new through — and a cold start is exactly when both think that. The backend
  lives in the driver crate, since it is IO, which also keeps `tokio` a dependency of that crate and
  of nothing else.
- **A measured registration storm**: 500 devices at 716/s on first registration and 661/s on refresh —
  flat, which is the finding, because one REGISTER costs one read and one write however large the
  estate. The test asserts the *ratio* rather than a rate, so it catches an accidental O(n) read path
  without encoding one machine's disk into the suite. Refresh writes are deliberately not coalesced:
  ~14 seconds for ten thousand devices is not a reason to trade away the guarantee that the `200`
  follows the commit.
- **The PB vector table as a generated, checked report** (`PX-7`) —
  `docs/reference/proxy-conformance.md`: 34 of 42 rows proved, 8 deferred with a reason and a story.
  Coverage is derived from **test names**, never a hand-maintained list, so deleting a test deletes
  the claim rather than leaving it standing. The check fails three ways, and the third is the one
  that matters: a *stale deferral* — a row marked "not yet" that is in fact covered — is how a
  coverage report starts lying about what it proves.
- **Adversarial schedules over a pinned seed corpus** (`PX-7`) — 35 % loss, 30 % duplication,
  1–120 ms jitter and a retransmission loop. A retransmitted INVITE never forks twice, asserted on
  the branches the engine created rather than on messages observed, because the network legitimately
  duplicates messages and only the branch count is the property. `HARNESS_SEED` replays any failure;
  a malformed one panics rather than silently running the corpus, since otherwise a developer would
  think they had reproduced something they had not.
- **The registrar and the proxy meet** (`RG-6`) — a location lookup becomes forking targets, behind
  the proxy's optional `registrar-targets` feature. Optional on purpose: the forwarding core also
  forwards to trunks and to plain Request-URIs, so it must not *depend* on the registrar, and a
  feature keeps the coupling visible in the manifest. Ordering is not redone — the lookup's order is
  already a pure function of the set and `now`, so every node computes the same one, and a second
  opinion here would be one too many. The scenario is one node running both real halves: two
  endpoints register, one calls the other, and the call forks to both of the callee's devices.
- **CANCEL and Timer C** (`PX-6`) — §9's C1–C6, with the `PB-C` vectors plus five harness scenarios
  in which the **real** engine runs behind a driver for the first time. `AnswerCancel` is its own
  effect because a CANCEL is its own transaction and its `200` is unconditional — it acknowledges the
  CANCEL, not the cancellation. A branch that has produced no provisional gets its CANCEL *queued*
  until one arrives, because a CANCEL that overtakes its INVITE cancels a transaction that does not
  exist yet while the INVITE proceeds. Timer C with provisionals seen cancels; Timer C with total
  silence concludes as `408`, since there is nothing at the far end worth cancelling. Verified across
  24 seeds with jitter and duplication, and in virtual time — the same Timer C assertion against a
  real clock would take three minutes a run.
- **Stateful forwarding with forking** (`PX-5`) — `sipx-clstr-proxy`, 47 tests, running every
  `PB-V`, `PB-P`, `PB-F` and `PB-R` vector. Validation, route preprocessing, the §16.6 forwarding
  edits, parallel and sequential forking, response aggregation and best-response selection.
  `PB-F-1` asserts the effect **sequence**, not just its contents: a Timer C armed before its INVITE
  went out would measure the wrong interval, and only an ordered assertion catches that.
- The rules that are easy to get subtly wrong, each pinned by a vector: `Max-Forwards: 0` refuses
  every method including OPTIONS (a proxy answering on the target's behalf leaks topology); a branch
  `503` becomes `500` upstream (the caller must not be told the destination is unavailable when what
  happened is that we could not reach it); a second `2xx` for one INVITE is forwarded too, because
  RFC 6026 makes that a fork rather than a bug; a re-INVITE is not Record-Routed (RFC 6141 — it
  cannot alter an established route set, so it is pure cost); `401`/`407` aggregate every challenge
  from every challenging branch.


- **The location service** (`RG-3`) — `sipx-clstr-registrar`, 78 tests, plus 5 harness scenarios.
  **Every vector table in the specification is executable**: 22 canonicalization rows, 21 REGISTER
  rows, 6 consistency rows, 8 lookup rows, 3 shard-key rows.
  - **Canonicalization is a total function, and §19.1.4 stays a comparison.** RFC 3261's URI
    equivalence is non-transitive by its own example, so it can compare two contacts but can never
    key a hash — two spellings of one address-of-record would land on two shards. The canonical
    form is an injective printable encoding of §10.3 step 5's URI; the kernel's `equivalent` is used
    only for matching within one address-of-record, where it is a linear scan.
  - **The whole request commits or none of it does.** Expiry selection runs for every contact
    before the first mutation, because one too-brief contact fails the entire REGISTER — deciding
    per contact as you go leaves the first one committed. The quota is checked against the committed
    outcome rather than the request, so a refresh or a removal can never trip it.
  - **CAS serialization is proved under the harness rather than asserted at a store.** A synchronous
    read-then-commit cannot race — the scheduler runs one input to completion — so the scenario's
    registrar edge is two-phase, the way a driver awaiting a database is: a REGISTER reads and arms
    the round trip, and the timer commits. Two edges given the same address-of-record at the same
    instant both read revision *n*, one wins, the loser re-reads and commits at *n+2*. The test
    asserts the race **happened**, not just that the result looks right: a serialization test that
    silently never raced would pass for the wrong reason. It also asserts the ordering nobody thinks
    to check — the `200` follows the commit, because a UA told it is reachable before the write
    landed has been told something false.
- **The deterministic cluster harness** (`CF-5`) — `sipx-clstr-sim`, 49 tests. Virtual time as a
  discrete-event queue where advancing the clock *is* popping the queue; equal deadlines broken by
  insertion sequence so the event order is a function of (scenario, seed) rather than of hash
  iteration; timer cancellation by generation counter. Two link kinds whose difference is
  load-bearing: a datagram link gets its reordering from latency jitter the way a wire does, and a
  stream link is FIFO however the jitter lands, never loses a message however lossy the policy,
  and fails by *breaking* rather than by silence. Messages are serialized and re-parsed across
  every link, so a node that builds a message it cannot write out is a bug the harness catches,
  and an unparseable arrival is traced rather than dropped — a serializer bug and an idle network
  look identical from the receiving end.
  - **The generator is written out rather than imported.** `rand` documents that its
    general-purpose generators may change output between versions, and a cargo update that
    silently reshuffled every recorded seed would break the one property the harness exists for.
    Pinned by `SplitMix64`'s published vector, by xoshiro256\*\* from state `[1, 2, 3, 4]` worked
    out by hand, and by a composite.
  - **The first implementation of that generator was wrong, and the hand-computed vector caught
    it.** xoshiro's state update is sequential — each step reads what the one before it wrote —
    and the natural Rust spelling, a single array literal, computes every element from the *old*
    state and yields a different, weaker generator that passes every smell test. One assertion
    separates them.
  - A livelocked scenario fails with a virtual timestamp instead of hanging CI.
- **The proxy transaction driver is designed** (`PX-2`,
  [design](docs/designs/proxy-transaction-driver.md)) — the seam `PX-5` implements against: one
  task per proxied request owning its response context, branch streams in a `FuturesUnordered` so
  responses, timers and an upstream CANCEL become one input at a time, no locks on the signalling
  path, and the three places backpressure actually lives.

- **The RoutePlan and the resolver decision** (`RT-1`, design accepted). The epic's headline
  question — build an async shared-cache resolver here, or get one upstream — is settled
  **upstream**: sipx `T-17` shipped it in `v0.4.0`, including the `_sip._ws`/`_sips._wss` prefetch
  this story was written to chase, so no caching layer is built here.
  - The seam that made it possible is `Prefetched`: RFC 3263 selection is pure computation over
    records, so the kernel awaits first and hands the answers to a synchronous `resolve` — which is
    how a proxy and a UA share selection logic without either becoming async.
  - What stays local is the *plan*, not the resolution. `RoutePlan` is an ordered list of
    `Attempt`s that **wrap** the kernel's `Target` rather than restating it, adding provenance and
    trunk context. A second address type here would eventually disagree with the kernel about which
    host a TLS certificate must be valid for, and that disagreement is a silent downgrade.
  - Every await lands in the driver by construction: the sans-IO core emits the `ResolveTargets`
    effect it already has, and the plan comes back as one input — so `RT-4`'s failover vectors are
    ordinary harness scenarios rather than tests that need a nameserver.
- **Failure is a scripted input** (`CF-4`). `sipx-clstr-sim::fault` adds `Fault` — `KillNode`,
  `Partition`, `Heal`, `SetLinkPolicy`, `TimerSkew` — and `Schedule`, both plain values, so a
  scenario can generate, mutate or fuzz its weather without touching scenario logic, and composing
  two schedules is concatenating two lists.
  - **No third mechanism.** Every fault is an override of what the link layer already models; a
    partition is `partitioned` on the crossing links and a kill is that on all of a node's. Two
    mechanisms that can both drop a packet eventually disagree about whether one was dropped.
  - **Faults are queue entries, not a side channel**, so they interleave with deliveries and timers
    under the same insertion-sequence tie-break and a scheduled run replays byte for byte from its
    seed — asserted both ways: same seed gives the same trace, a different seed gives a different
    one, or the faults would not be seeded at all.
  - **A kill is not an isolation.** `KillNode` drops the node's timer generations as well as
    cutting its links, so nothing fires afterwards; a node that keeps timing out while unreachable
    is a different failure, spelled `Partition` over all of its links.
  - A fault at time zero lands *after* nodes are started, so a `TimerSkew` there does not affect
    the timer a node arms in reaction to `Started`. Weather arrives at a running scenario.
  - The sim-vs-real fidelity criterion moved to `CF-3`, which is the story that builds the sockets
    to compare against.

### Changed

- **The harness keeps the kernel's timer queue instead of its own** (`CF-7`). sipx `v0.7.0` made
  `TimerQueue` generic over its **instant**, so `impl Add<Duration> for SimTime` is the entire
  adapter and `sipx-clstr-sim` hands the kernel its own clock — no runtime, no wall time.
  `queue.rs`'s `Generations<K>` is deleted; `SetTimer`/`ClearTimer`/`KillNode` are now `set`,
  `clear` and `forget_matching`.
  - **The tie-break was the part that mattered.** Timers live in the kernel's queue and
    deliveries, breaks and faults in this crate's, because those carry payloads. At an equal
    instant the scheduler drains a *fault first*, then timers, then the rest — exactly what the
    single queue's insertion-sequence rule produced, since faults are scheduled during setup. The
    first attempt drained timers first and moved two scenarios by one ping each, because a kill at
    T raced the timer at T instead of stopping it.
  - **No seed changed meaning and no expectation was edited.** The story's escape hatch for
    recording deliberate seed changes was not needed.
  - The loopback link is **decided local** rather than adopted, per `CF-1`'s per-component split:
    the kernel's is two-party where the harness needs an N-node mesh, has no stream class for
    §16.9, and owns its own generator — adopting it would change what every seed means.
  - Cost, stated plainly: `sipx-clstr-sim` now links `tokio` transitively. The harness never
    builds a runtime, and the decision crates depend on the harness rather than the reverse, so
    the registrar's sans-IO guard is unaffected.

- **The kernel pin moves `v0.2.1` → `v0.7.0`**, in two steps and for two different reasons.
  `v0.4.0` brought both sides of digest (`S-16`, `X-20`) and cleared every filed row in the
  [upstream ledger](docs/upstream.md) at once. `v0.7.0` then closed the one row that reopened on
  contact with the code: `TimerQueue` is now `TimerQueue<K, I = Instant>`, generic over its
  **instant** rather than only its key, so a virtual clock can drive it — which `CF-7` needs and
  `X-14` had not delivered. The default type parameter means no existing caller changed.
  - Everything in the suite passes at the same seeds against `v0.7.0`; three kernel releases of
    drift produced no behavioural change here.
  - `KERNEL_VERSION` moves with it, and `kernel_pin.rs` fails the build if it ever does not —
    an operator reading `--version` during an incident gets the tag the binary was actually
    compiled against.
- **`Path` is read as a typed header** (kernel `T-14`). `HeaderName::Other(b"Path")` matched
  nothing once the kernel started typing it, so a REGISTER carrying a `Path` registered an *empty*
  route set and the branch toward the callee silently lost its route. Caught by
  `a_registered_path_becomes_the_route_set_the_invite_carries`, fixed at the lookup rather than by
  re-recording the expectation.
- **Via surgery is one upstream call instead of a collection rebuild** (`PX-3`). sipx `S-15`
  shipped `Headers::remove_first` in `v0.4.0`, so the two sites that built a fresh `Headers` and
  copied every header except the topmost `Via` into it — the proxy's `pop_via` and the harness
  stub's `pop_top_via` — are each a single call. `remove_all` in `forward.rs` stays: it means
  *every* occurrence, and adoption ends where the semantics differ.
  - `tests/header_surgery.rs` pins what the rebuild was silently guaranteeing. The existing `PB-R`
    rows only check that our own `Via` is gone, which is equally true of a response whose remaining
    stack came back shuffled; the new rows use a three-`Via` stack and assert arrival order
    survives, and that headers either side of the stack are untouched.
  - One test reads the crate's source rather than its behaviour, because "uses the upstream API
    *exclusively*" is not a behavioural claim — a reintroduced rebuild would pass every other
    assertion in the file.
- **The upstream ledger grew a state for "written but not released", because that is where `RG-2`
  is.** The kernel work M1's last story needs now exists — `S-16` (server-side digest: nonce
  minting, challenge emission, verification, a bounded replay window) and `X-20` (which makes those
  primitives reachable from a crate with no async runtime, since `sipx-ua` pulled `tokio`
  unconditionally and a sans-IO registrar cannot). Both are commits on a **local** `main` — sipx's
  `origin/main` is behind them and no tag contains them. This workspace pins a git revision from
  the public URL so that "which kernel version is this claim true of?" has an answer, and a commit
  no remote has is not a revision it can name. `RG-2`
  therefore stays `blocked` and M1's fifth exit criterion stays unproven — recorded rather than
  worked around, because the two available workarounds were writing digest a second time (the thing
  `S-16` exists to prevent) and `[patch]`ing to a local checkout (an unreproducible build that
  hides the dependency from the ledger tracking it).
- **The proxy driver is built on the kernel's endpoint, not beside it.** The epic design argued
  that `sipx_transport`'s API was "UA-shaped: one transaction, one target" and that a forking
  proxy therefore needed its own socket loop. Reading the released kernel rather than remembering
  it found that wrong where it counts: `Handle::send` creates one client transaction per call, N
  calls fan out over one transaction layer, and `send` inserts a `Via` only when one is absent —
  so a proxy that pushes its own keeps control of the branch, and that branch *is* the transaction
  key. The alternative would have meant reimplementing framing, three handshakes, NAT `Via`
  rewriting, the pool and the retransmission driver, which is the rule against shadow-implementing
  kernel logic in as many words.
- **M1 is defined** rather than described: fourteen stories in a fixed order, each with what it
  adds and the vectors that prove it, plus exit criteria that are commands and an explicit
  out-of-scope table with reasons. The board reflects it — only stories with no unmet dependency
  are `ready`, so the top of `Next` is always genuinely takeable.
- **M1 does not block on the kernel.** Three of the four filed sipx stories would make M1 code
  nicer, not possible; only server-side digest (`RG-2`) genuinely waits, and it sits ninth.

### Fixed

- **The probe's requests carried no `Via`** (found by `ET-3`'s end-to-end scenario). RFC 3261
  §8.1.1.7 makes `Via` how a response finds its way back, so a compliant proxy refused the INVITE as
  unanswerable and the probe reported a timeout it had caused itself. 24 unit vectors and 5 harness
  scenarios had all agreed the engine was right; the first component that actually *validated* a
  request disagreed within one run.
- **The probe engine matched responses to whatever step was outstanding** (`ET-2`), with no
  correlation at all. Under the duplication UDP produces routinely, a second `200` to REGISTER
  arrived while the probe awaited INVITE and was read as the INVITE's answer — reporting a
  `MarkerMismatch` the probe had manufactured. Found at seed 5 of a 16-seed sweep; responses are now
  matched by the `CSeq` of the request that provoked them. Fixing it exposed that the unit fixtures
  built responses with **no `CSeq`**, which is precisely why they could not have caught it; they now
  play a peer that echoes it, as a real one does.
- **The probe's rate bound did the opposite of bounding** (`ET-2`): `now.saturating_sub(per)` clamps
  to zero, so the entry recorded at time zero was trimmed immediately and a second run went straight
  through.
- **Two races in the `PostgreSQL` backend, both flaky rather than broken** — the dangerous kind, since
  each passed on a re-run. `CREATE TABLE IF NOT EXISTS` is *not* atomic against a concurrent `CREATE`,
  so nodes starting together — a cold deployment, every rollout — collide on
  `pg_type_typname_nsp_index`; the DDL now runs under an advisory lock. And the tests shared one
  tenant while running concurrently, so one truncated another's rows mid-run and a commit came back
  reporting the row had vanished. Each test now owns a tenant, which is the contract's own boundary.
  Verified by ten consecutive runs against a freshly dropped table.
- **`PB-V-9`'s test had been deleted and nothing noticed** — it went with the block rewritten to fix
  the loop-detection cookie, and the suite stayed green because 45 other tests still passed. The new
  coverage check flagged it on its first run, which is the entire argument for deriving coverage from
  the spec rather than from the suite's own opinion of itself.
- **`Max-Breadth` was not being honoured** (`PB-V-8`). It bounds *parallel* fan-out; with a budget of
  1 and two targets the code forked both. RFC 5393 §5.2 requires the surplus to be serialized behind,
  not truncated — truncating silently loses a device the user registered, and nobody notices until a
  call does not ring on one phone.
- **Every registered `Path` was silently dropped** (found by `RG-6`). `forward()` implemented every
  §16.6 step except step 6 — applying the target's route set — so a registration's stored path never
  reached the wire. RFC 3327 §5.3 makes that path the route toward the contact, and losing it makes a
  UA behind a proxy unreachable in exactly the deployments Path exists for. Invisible to all 40
  `PB-*` vectors because every one of them uses an empty route set: the defect lived in the gap
  between two crates that were each correct alone, which is the argument for the integration
  scenario. Now applied in stored order, ahead of any `Route` that survived preprocessing, with bare
  URIs bracketed — because `sip:p;lr` unbracketed makes `;lr` a *header* parameter and loses
  loose routing.
- **Two CANCEL tests were passing for the wrong reason** (`PX-6`), and asking *why* they passed is
  what surfaced it. The harness driver silently dropped every CANCEL — `self.perform(self.context.on_input(…))`
  borrows `self` twice, and the stub helper written to dodge that returned no effects — and the
  adversarial sweep passed anyway, because `run_until_idle` drains the queue past Timer C at 180 s, so
  every seed reached its `487` through the timer rather than through the CANCEL. The scenarios now
  advance a bounded 10 s of virtual time, and the constant that bounds them explains why.
- **The loop-detection cookie could not detect a loop, and PX-5 proved it.** `proxy-behavior` §6
  listed the topmost incoming `Via` among the cookie's fields. A looping request arrives carrying
  *our own* `Via` on top, so the recomputed cookie can never equal the one we minted — every loop is
  misjudged a spiral and forwarded, round the cycle, until `Max-Forwards` expires at every node on
  it. The topmost `Via` decides where the *response* goes, not where the *request* is routed; RFC
  3261 §16.6 step 8 recommends it as **entropy** for transaction uniqueness, which is a different
  job. The spec is corrected, the cookie now covers routing state only, the `Via` feeds the branch's
  unique part, and `PB-V-6` asserts an actual `482` end to end — which it could not have done
  before.


- **Two rows of the upstream ledger were wrong** (`CX-1`). TLS, WebSocket and WSS were recorded as
  unreleased kernel work; they shipped in sipx 0.2.0, with interop runs, and `tls`/`ws`/`wss` are
  default features. Nothing was blocked on a release that had already happened. The resolver row
  assumed its own answer, so the story filed for it is written to *settle* upstream-vs-local
  instead. The ledger gained a rule: a row can be wrong — re-read the kernel before believing one.
- **The kernel gaps are filed** (`CX-1`): six stories in the sipx repository, in its conventions,
  each naming a failing-first test and each earning its place in the kernel on its own rather than
  only as a downstream ask. `PX-2` added two more, both found by reading the endpoint's dispatch:
  a response matching no client transaction is **dropped**, though RFC 3261 §16.7 requires a
  stateful proxy with no response context to forward it statelessly; and incoming requests are
  delivered with `try_send`, so under backpressure they are lost silently, with no counter. A
  dropped INVITE is a missed call the peer retransmits — a dropped 2xx ACK is a call that never
  ends. Neither blocks M1; both must close before M2 claims a node can be killed.

## [0.3.0] — 2026-07-28

### Added

- **A front door.** `README.md` explains the project to a person arriving cold: the problem (a
  five-node SIP proxy has behaviour no RFC describes), the answer (no shared call state — a signed
  token in the message carries what routing needs), and where this actually is. The status is
  stated in the first screen rather than discovered on the third: four specs written, no Rust yet.
- **A logo** (`docs/assets/logo.svg`) — the sipx crab, kept identical for family resemblance,
  shrunk inside a three-node mesh. At favicon size what survives is an orange body in a dark
  triangle, which is the distinction worth keeping: sipx is a phone, sipx-clstr is a cluster of
  them.
- **A published documentation site** at
  [codewandler.github.io/sipx-clstr](https://codewandler.github.io/sipx-clstr/) — Docusaurus
  reading `docs/` directly rather than copying it, so there is one set of words. Curated sidebar
  (the story board and the archive stay working material), offline search, mermaid diagrams, and a
  palette taken from the logo. It deploys on published releases only, so the public site follows a
  tag rather than the last hour's work.
- **CI for the documentation gate** (`.github/workflows/docs.yml`), making executable what
  AGENTS.md describes: relative links resolve, every `epic:` slug has a design doc, every `design:`
  path exists. It joins a build job when `CX-2` lands the Cargo workspace rather than being
  replaced by one.
- **MIT and Apache-2.0 licenses**, matching sipx.

### Changed

- **AGENTS.md gained a map** — where each kind of document lives, which are generated, and the
  state of play in one line — plus the publishing rule and two additions to the gate.
- **The downstream boundary is deployment-agnostic.** The gap stories filed from a consuming
  deployment carried that deployment's name through their notes and prose; a platform repo that
  names one consumer invites requirements shaped for that consumer. Traceability is kept by citing
  the ledger entry rather than the repo.

### Fixed

- **Links that only worked for the author.** `../../sipx` was a path to a sibling checkout;
  `../AGENTS.md`, `../CHANGELOG.md` and the board pointed outside what gets published. All are now
  absolute URLs that resolve in the repository and on the site alike.
- Contact-set notations in the location-service spec are code-spanned, matching the convention
  already used by their neighbours in the same table.

## [0.2.0] — 2026-07-28

### Added

- **The M0 load-bearing specs land** — written concurrently and cross-reconciled. The proxy
  behavior spec (PX-1): RFC 3261 §16 amended by RFC 5393 as a sans-IO engine with 42 vectors.
  The location service spec (RG-1): a canonical AoR byte form, the per-AoR CAS contract, the
  PostgreSQL and in-memory mappings, forking-ordered lookups. The affinity token spec (AF-1):
  byte-level layout with direction and a 64 B module-facts region, AEAD by default, worst-case
  157 B against the 200 B URI-parameter budget, stateless replay semantics — and a settled
  no-mid-dialog-refresh rule (route sets are fixed at establishment) now reflected in the
  media-control reselection risk and KO-9. The hook framework spec (EX-1): thirteen phases
  aligned to the proxy pipeline, closed per-phase effect sets, a manifest whose state-key
  domain makes dialog-keyed module state unrepresentable. The harness design (CF-1): a
  discrete-event clock, links-with-policies with faults as scheduled overrides, scenarios as
  code over declarative schedule values, a per-component sipx-testkit upstream split, and the
  conformance registry as a requirement-grain local extension of the kernel's per-RFC registry,
  kernel rows inherited by reference. (PX-1, RG-1, AF-1, EX-1, CF-1)
- **The operator epic starts moving and the backlog deepens.** The Helm chart scaffold under
  `deploy/helm/` (KO-2, in progress), the active-drain story (KO-9), and fifteen new stories
  across routing (egress allowlist, number normalisation, asserted identity, source-IP
  admission, scoped routes), deployment (split listeners, CDRs, capture), extensions (async
  external routing hook, carrier quirk profiles), media (per-trunk codec/SRTP policy), and the
  operator (naming contract, anti-affinity, OCI chart).

## [0.1.0] — 2026-07-28

### Added

- **Design scaffold (M0).** The track backlog framework, the vision and roadmap, the upstream
  dependency ledger, eleven epic design docs (proxy engine, registrar & location, cluster
  affinity, routing & trunks, media control, extension framework, conformance harness,
  deployment, the deferred B2BUA services placeholder, the end-to-end call probe, and the
  Kubernetes operator), architecture charts, and a 60-story backlog: a six-story ready queue for
  the M0 specs plus the implementation backlog toward M1 and M2 (M3/M4 stories are seeded when
  their milestones near). No code yet — the Cargo workspace arrives with `CX-2` as the first act
  of M1.

### Fixed

- **Review findings resolved across the design layer.** The ICE stance no longer lets anchored
  calls negotiate around the relay; the affinity token gained its missing direction field and
  honest, stateless replay semantics; transaction affinity is now an explicit dataplane
  requirement instead of an unstated assumption; previously unowned work got owning stories
  (CF-5 harness implementation, CF-6 conformance-registry seeding, AF-7 connection ownership,
  ME-5 media-anchoring module); M1's stateless-mode promise, the sipx transport-milestone
  attribution, an RFC 3263 §4.3→§4.4 miscitation, and the impossible "answered-then-cancelled →
  487" wording were corrected; blockers and upstream markers moved into board-visible `note:`
  fields, and the charts were rewired to match the designs.
