---
id: ET-7
title: Make the probe establish and tear down a real SIP dialog
pillar: Platform
status: backlog
priority: 1
design: docs/designs/e2e-tester.md
epic: e2e-tester
areas: [probe, harness, dialog]
note: V-03 · blocked by PX-13 for real-node acceptance; the current probe invents AoR-shaped ACK/BYE and proves no dialog route
---

# Make the probe establish and tear down a real SIP dialog

## Goal

Make the probe use the sipx dialog layer to retain the INVITE 2xx's remote target, tags and route set,
then construct the ACK and BYE for that actual dialog so a passing run proves dialog routing and
cleanup rather than a second AoR lookup.

## Acceptance

- [ ] **Dependency:** `PX-13` lands first; real-node acceptance must exercise the corrected direct
      Route/remote-target path rather than encode the current AoR workaround into the probe.
- [ ] The successful INVITE 2xx is consumed through sipx's dialog layer. The probe retains Contact as
      remote target, the To-tag, Call-ID, local/remote tags, ordered Record-Route route set, and dialog
      sequence state.
- [ ] The 2xx ACK uses the INVITE CSeq number and dialog route/remote target and is sent before BYE.
      BYE advances the local dialog CSeq and uses the same route set. Neither request is addressed to
      `config.echo_aor` merely to make registrar lookup succeed.
- [ ] Cleanup after marker mismatch, timeout after answer, cancellation and ordinary success sends at
      most one dialog-correct BYE. A failed teardown is recorded without changing an already-decided
      verdict, preserving S3/V6.
- [ ] The echo or protocol-correct test peer verifies dialog identifiers and route/remote-target
      behavior rather than answering any BYE unconditionally.
- [ ] **Failing-first tests:** an INVITE 2xx supplies a Contact different from the echo AoR and two
      Record-Route values; exact ACK and BYE Request-URI, Route order, tags and CSeq are asserted.
      A real-socket run through PX-13 must pass and the AoR-shaped implementation on `86e6b10` fails.
- [ ] The e2e-probe spec's dialog-layer dependency and EP vectors are updated/proved together;
      `scripts/gate.sh` is green.

## Progress

- (not started; blocked by `PX-13`)

## Notes

- Validated synthesis finding [**V-03**](../reviews/00-validated-synthesis.md#v-03--ack-and-in-dialog-requests-are-routed-as-registrar-lookups). PX-13 fixes the node path; ET-7 fixes the proof instrument that currently masks it.
- `ProbeEngine::send_bye` currently calls `simple_request` twice with `echo_aor`, and the echo answers
  any BYE while retaining no dialog. That sequence demonstrates reachability, not dialog cleanup.
- **Upstream boundary:** dialog construction/state is protocol-generic and must use sipx's dialog
  layer unmodified; probe scheduling, correlation, cleanup obligations and verdicts stay here.
