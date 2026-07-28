---
id: CX-3
title: Prove M1 against real phones
pillar: Platform
status: ready
priority: 14
design: 
epic: 
areas: [build, proxy, registrar]
note: M1 #14 · the exit proof — two sipx CLI phones, one node, a real call
---

# Prove M1 against real phones

## Goal
Close M1 with the demonstration its definition names: two `sipx` CLI phones register through one
sipx-clstr node, call each other through it, and hang up — over real sockets, with media flowing
directly between the phones.

## Acceptance
- [ ] A scripted run brings up one node and two `sipx` CLI phones, registers both, places a call
      from one to the other, answers it, and ends it with BYE. Every step asserts on the CLI's
      `--json` output and its exit code, not on log scraping.
- [ ] **Media flows directly between the phones**, and the run proves it rather than assuming it:
      the SDP each phone receives names the other phone's address, not the node's.
- [ ] The node's own record of the call agrees with what happened — the call is visible, and the
      transaction store is back to empty afterwards. A proxy that leaks one transaction per call
      is a slow outage, and this is the cheapest place to notice.
- [ ] The run is one command, and it fails loudly. Anything that needs a human to interpret it is
      not an exit criterion.
- [ ] Failing-first test: `two_phones_call_each_other_through_one_node`.

## Progress
- (not started)

## Notes
- This is deliberately the *last* M1 story: everything it exercises is proved in the deterministic
  harness first (`PX-7`, `RG-3`), and this run exists to catch what a simulation cannot — real
  sockets, real DNS, a real independent implementation of the client side.
- `CF-3` generalizes this into the full interop harness (SIPp, third-party endpoints, rtpengine).
  This story is only the milestone's own proof, and should stay small enough to keep running.
- The phones are `sipx`'s CLI (`sipx register`, `sipx dial`, `sipx answer`), which shipped in that
  project's M4. They are an independent implementation of the client side, which is what makes
  this worth more than a self-test.
