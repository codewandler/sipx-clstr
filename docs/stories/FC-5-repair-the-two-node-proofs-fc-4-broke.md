---
id: FC-5
title: Repair the two-node proofs FC-4 broke, and make a mismatched domain impossible to ship
pillar: Foundation
status: in-progress
priority: 1
epic:
areas: [scripts, deploy, ci]
note: the repository's headline cluster proof answers 403 — the script registers in a domain its own document does not serve
---

# Repair the two-node proofs FC-4 broke, and make a mismatched domain impossible to ship

## Goal

`FC-4` gave the node a tenant `domains` list and made a `REGISTER` outside it a `403` — closing a
real authorization hole, where `alice@attacker.invalid` against `domains: [example.test]` was
answered `200`. `scripts/e2e-call.sh` was updated with it. **The two-node proofs were not**, and they
are the evidence for the `0.11.0` release's headline claim.

Both are one word wrong:

| Script | Registers in | Document serves | Result |
|---|---|---|---|
| `scripts/two-node-call.sh:65` | `127.0.0.1` | `example.test` (`:108`) | `403` |
| `scripts/k8s-two-node-call.sh:90` | `$NODE_A` (a Service IP) | `cluster.local` (`node.yaml:53`) | `403` |
| `deploy/devspace/manifests/node.yaml:367` | `sip:hello@$A` | `cluster.local` | `403` |

Reproduced on `DX-13`'s branch:

```
── alice registers through node A, bob through node B
{"status":"unauthorized","error":"rejected: 403 Forbidden"}
[FAIL] alice could not register through node A
```

So `README.md`'s claim that `two-node-call.sh` "proves it locally" is currently false, and
`website/docs/getting-started.md` §3–4 tells a reader to run three things that cannot pass. `DX-13`
found this, could not fix it — `scripts/**` and `deploy/**` were outside its write set — and
deliberately left the README sentence alone rather than soften a claim that becomes true again the
moment this lands.

## Acceptance

- [x] **Failing-first**: run `scripts/two-node-call.sh` at the merge base and capture the `403`
      verbatim. It is red today; this is the rare story where the failing test already exists and
      only has to be *run*.
- [x] `scripts/two-node-call.sh` passes end to end — both registrations, the cross-node call, and
      audio. **Caveat on the last word**: this script asserts signalling only. It never recorded or
      checked media, unlike `e2e-call.sh` and `k8s-two-node-call.sh`, which both assert
      `heard_audio`. Adding that assertion is a change to what the proof proves and is left for its
      own story rather than smuggled in here — see the note below.
- [x] `scripts/k8s-two-node-call.sh` and `deploy/devspace/manifests/node.yaml` agree with each other
      on the domain, including the greeting pod's `sip:hello@…`. If a cluster is not available to
      run it, say so explicitly rather than claiming it passes.
      **It was not run.** The only cluster reachable here is a long-lived shared one unrelated to
      this project, carrying live namespaces; standing this profile up in it is not a side effect to
      take unasked. The agreement is machine-checked instead, by the item below.
- [x] **The class is closed, not just the three instances.** A script whose `DOMAIN` is not served by
      the document it ships alongside must fail something before it reaches a release. A check that
      reads each script's document and its registered AoR and compares them is enough; a comment
      asking the next person to remember is not.
- [ ] `website/docs/getting-started.md` §3–4 runs as written afterwards, which is the one acceptance
      item `DX-13` could not tick. §4's dial was updated to the greeting's new address-of-record, so
      the page is consistent with the manifests again — but §3–4 need a Kubernetes cluster to
      execute and none was available, so this is *edited to be true*, not *observed to be true*.
- [x] `scripts/gate.sh` green.

## Progress

- **Failing-first, run rather than written.** `scripts/two-node-call.sh` at the merge base
  (`e61308e`), verbatim:

  ```
  ── alice registers through node A, bob through node B
  {"status":"unauthorized","error":"rejected: 403 Forbidden"}
  [FAIL] alice could not register through node A
  ```

  and after the fix, the same script: `RESULT: PASS` — both registrations, two binding rows written
  by two different nodes, and the cross-node call completed.

- **The three instances.** `two-node-call.sh` now declares `domains: [127.0.0.1]`, the domain it was
  already registering in — `e2e-call.sh`'s spelling, so the two local proofs agree. The Kubernetes
  pair moved the other way: the AoR domain became node-a's **Service name**,
  `sipx-clstr-node-a`, in the script, in the document's `tenant.domains`, and in the greeting pod's
  `sip:hello@…`. It could not stay an address. `domains` is a literal list in a ConfigMap written
  before any pod or Service has an IP, so a runtime address can never appear in it; a per-node
  ClusterIP name is stable, resolvable from any pod in the namespace, and names exactly the pod the
  IP used to. Routing is unaffected — the proxy keys a location lookup on the AoR string
  (`driver.rs` `ResolveTargets`), and `ProxyConfig::identities` is loop detection, not locality.

- **The class.** `scripts/check-proof-domains.py`, wired into `scripts/gate.sh` and the `docs`
  workflow. It reads every tracked file under `scripts/` and `deploy/` that registers or dials a
  `sip:` URI, resolves the AoR domain through the script's own shell assignments, and compares it
  byte-exactly against the `tenant.domains` of the document that governs it — embedded, or named by
  a `proof-document:` comment. Verified red on the merge base, where it reports all three instances:

  ```
  deploy/devspace/manifests/node.yaml:367: the domain of `sip:hello@$A` is `$A`, which is only known
  at runtime. …
  scripts/k8s-two-node-call.sh: registers an address-of-record but it embeds no cluster document and
  names none …
  scripts/two-node-call.sh:145: registers in `127.0.0.1`, which scripts/two-node-call.sh does not
  serve — it declares `domains: ['example.test']`. That REGISTER is answered 403
  proof domains: FAIL — 5 problem(s)
  ```

  Three rules make it hard to satisfy vacuously: `domains: []` fails (it is the fail-open `FC-4`
  removed, and would otherwise be the cheapest way to turn this green); a domain that is not a
  literal fails, because a runtime value cannot be shown to be in a document written earlier; and an
  address-of-record whose governing document cannot be found fails, so a new script cannot escape by
  embedding no document.

- **Not done here.** The Kubernetes proof was not executed — see the acceptance item. And
  `two-node-call.sh` asserts no media at all, which the acceptance's "and audio" assumes it does;
  that is a real gap in what the headline proof proves, but closing it changes the proof rather than
  repairing it, so it wants its own story.

## Notes

- **This is a regression the integration pass introduced**, not a latent bug: `FC-4` updated the one
  proof script it was looking at and not the other two. Worth stating plainly in the story, because
  the lesson is about blast radius rather than about domains — the same integration also had to fix
  `scripts/e2e-call.sh` after `DP-10` removed the flags it used, which was on a written blast-radius
  list and still got missed.
- Do not close this by widening the tenant to `domains: []` ("any"). That is the fail-open `FC-4`
  exists to remove, and it would make the proof prove less than it did before.
- `e2e-call.sh` is the worked example of the fix: it declares `domains: [127.0.0.1]` and registers
  there. `DX-13` used the same spelling for the README quick start so the two agree.
- The k8s path has a second wrinkle worth checking while in there: the greeting pod registers
  `sip:hello@$A` where `$A` is an address, so whatever domain the document declares must be the one
  the pods actually use, not merely a name that parses.
