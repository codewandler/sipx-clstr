---
id: FC-5
title: Repair the two-node proofs FC-4 broke, and make a mismatched domain impossible to ship
pillar: Foundation
status: ready
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

- [ ] **Failing-first**: run `scripts/two-node-call.sh` at the merge base and capture the `403`
      verbatim. It is red today; this is the rare story where the failing test already exists and
      only has to be *run*.
- [ ] `scripts/two-node-call.sh` passes end to end — both registrations, the cross-node call, and
      audio.
- [ ] `scripts/k8s-two-node-call.sh` and `deploy/devspace/manifests/node.yaml` agree with each other
      on the domain, including the greeting pod's `sip:hello@…`. If a cluster is not available to
      run it, say so explicitly rather than claiming it passes.
- [ ] **The class is closed, not just the three instances.** A script whose `DOMAIN` is not served by
      the document it ships alongside must fail something before it reaches a release. A check that
      reads each script's document and its registered AoR and compares them is enough; a comment
      asking the next person to remember is not.
- [ ] `website/docs/getting-started.md` §3–4 runs as written afterwards, which is the one acceptance
      item `DX-13` could not tick.
- [ ] `scripts/gate.sh` green.

## Progress

- (running log)

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
