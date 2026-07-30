---
id: DX-14
title: Hold release claims to executable evidence
pillar: Foundation
status: ready
priority: 1
design: docs/designs/docs-site.md
epic: docs-site
areas: [docs, gate, release]
note: V-18 — generated counts and the real driver disagree with release-facing capability claims
---

# Hold release claims to executable evidence

## Goal

Make the README and public site describe the behavior the released binary performs, not behavior a
pure engine can emit or a stale copied number once described, and make the gate fail when either kind
of release claim drifts again.

## Acceptance

- [ ] The README badge and status table are checked against the generated conformance result: proved,
      shape-only, deferred, and total counts have one source. A fabricated one-row denominator change
      makes every stale public copy fail with its file, line, expected value, and actual value.
- [ ] The public specification count and inventory are derived from the checker's registered spec
      set. They include all eleven current specs and the `SC` family from `sipx-cluster-crd.md`; adding
      a twelfth registered spec cannot leave “eleven” or an incomplete inventory green.
- [ ] `README.md` and the public capability surfaces distinguish **decision-core behavior** from
      **real-driver behavior** for each validated gap: role dispatch (V-01/DP-13), matched CANCEL and
      Timer C (V-02/PX-12), ACK and in-dialog routing (V-03/PX-13 and ET-7), and outbound transport
      resolution (V-12/RT-12). None is labelled “working” or “today” until its real-socket acceptance
      proof passes.
- [ ] The role explanation no longer says roles wire decision paths in the released node while the
      driver drops them. It states the schema rule separately from current runtime support and links
      the implementation claim to DP-13.
- [ ] The real-socket proof is consistently described as a **same-kernel, separate-process integration
      test**. “Independent implementation,” “independent parser,” and equivalent claims are reserved
      for CF-3's independent interop target.
- [ ] The pass covers, at minimum, `README.md`, `website/docs/intro.md`,
      `website/docs/clustering/how-it-works.md`, `website/docs/operate/deploy.md`,
      `website/docs/reference/cli.md`, `website/docs/reference/conformance.md`,
      `website/docs/guides/does-this-fit.md`,
      `website/docs/guides/registrations-and-calls.md`, both migration pages, and
      `website/docs/getting-started.md`. The gate scans the whole README/site for the governed claims;
      this list is the failing-first blast radius, not a permanent allowlist.
- [ ] Capability language can be promoted automatically or by an explicit checked mapping only after
      the owning story's real-driver proof exists. Closing an engine-only vector must not silently
      promote a wire-level claim.
- [ ] Demonstrated red first for both classes: restore one stale conformance number and one prohibited
      “today” claim, and show the gate name each mismatch before the corrected docs pass.
- [ ] `scripts/gate.sh` and the site build are green with no orphaned page or broken public link.

## Progress

- (not started)

## Notes

- Filed from the validated adversarial review of `86e6b10` (`v0.12.0`), synthesis finding **V-18**.
- At review time the generated truth was 125/549 proved, 19 shape-only, and 405 deferred, while the
  README still said 134/492 and 358 deferred. The checker registered eleven specs while public prose
  said ten. The first capability-narrowing pass corrected the headline numbers and several tables;
  this story closes the remaining claims and makes both forms of drift mechanically red.
- V-18 consolidates documentation consequences of higher-severity runtime findings. DX-14 owns the
  truthful public boundary and its checker; DP-13, PX-12, PX-13/ET-7, and RT-12 own the behavior and
  proofs that can later move that boundary.
- Considered for upstream: **no.** These are this repository's release claims, site inventory, story
  ownership, and generated conformance report. No SIP-generic behavior or reusable kernel primitive
  is implemented here.
