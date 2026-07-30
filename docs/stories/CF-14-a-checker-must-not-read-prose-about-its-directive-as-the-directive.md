---
id: CF-14
title: A checker must not read prose about its own directive as the directive
pillar: Foundation
status: done
priority: 2
epic: conformance-harness
areas: [ci, build]
note: documenting check-proof-domains.py's directive inside a proof turns the gate red on a correct script
---

# A checker must not read prose about its own directive as the directive

## Goal

`scripts/check-proof-domains.py` finds the document a proof registers against with

```python
DOC_DIRECTIVE = re.compile(r"proof-document:\s*(\S+)")   # :65
directive = DOC_DIRECTIVE.search(text)                    # :172
```

`search` takes the **first** match anywhere in the file, so a comment *explaining* the directive is
read as the directive. Reproduced by adding one explanatory comment to `scripts/two-node-call.sh`:

```
scripts/two-node-call.sh: registers an address-of-record but its `proof-document: <path>` does not exist

proof domains: FAIL — 1 problem(s)
```

The script was correct; the comment about the checker broke the checker. `DX-12` hit this for real
while documenting its own work and had to write around it by not spelling the token — which is a
checker training people to avoid describing it, and the description is exactly what the next person
needs.

It **fails closed** (a false red, never a false green), so this is a usability and trust defect
rather than a hole. That is why it is `priority: 2` and not `1`. But a gate that goes red for a
reason that is not a defect is the fastest way to teach people that red means "re-run it".

This is a known class in this repository: `check-docs.py` had the identical bug and fixed it by
stripping fenced and inline code before scanning for links (`prose()`). The fix exists; it was not
reused.

## Acceptance

- [x] **Failing-first**: add the explanatory comment to a proof script, watch the gate go red, and
      quote it. Reproduction is above and takes one line.
- [x] The directive is recognised only where it is meant — an anchored form, a designated header
      region, or the same code-stripping `check-docs.py` already does. Choose one and say why in the
      script.
- [x] A proof script can contain a comment that fully explains the directive, including the literal
      token, and still pass. That is the acceptance test: the checker must survive being documented.
- [x] The same question is asked of the other repo-consistency checkers rather than assumed —
      `check-site.py` (`DX-12`) resolves script references by *mentioning*, which its own RISKS note
      says a commented-out invocation would satisfy; `check-vectors.py` reads rows out of spec
      tables. Report what you find for each even where nothing needs changing.
- [x] `scripts/gate.sh` green.

## Progress

- **Fixed by anchoring.** `DOC_DIRECTIVE` is now
  `^[ \t]*#?[ \t]*proof-document:[ \t]*(\S+)[ \t]*$` with `re.M`: the directive is a line whose
  *entire* content is the token and a path. Chosen over `check-docs.py`'s code-stripping because a
  shell comment has no fence or code span to strip, and because it states the rule positively — a
  declaration occupies its own line, every other appearance of the token is prose. The reasoning,
  including why renaming the token was refused, is in the script above the constant.
- The optional `#` lets a Python proof declare it in a module docstring, the way `sip_demo.py`
  carries its `not-in-ci:`.
- **Two cases, both now in the tree as permanent regression evidence.** `scripts/two-node-call.sh`
  gained a paragraph that fully explains the directive and spells the token (it embeds its own
  document and declares nothing). `scripts/k8s-two-node-call.sh` spells the token *twice in prose
  above its real directive* — the shadowing case, where the correct declaration was being outvoted
  by an earlier mention. On the merge base those two files produce two failures; after the fix the
  real directive at `k8s-two-node-call.sh:37` resolves and the checker is green.
- **`DX-12`'s write-around is gone.** `k8s-two-node-call.sh` carried a paragraph saying it
  "deliberately does not spell the directive's name". That paragraph was the defect's visible
  cost; it is now a comment that names the directive plainly.
- **`check-site.py`: the same class, fail-**open**, and fixed.** `PROOF_DIRECTIVE`
  (`not-in-ci:`) was searched unanchored over a proof script's whole text, so a *description* of
  the convention — or a quotation of the script's own error message, which spells the token —
  counted as a declaration and silently exempted the proof. All four proofs it governs
  (`sip_demo.py`, `two-node-call.sh`, `k8s-two-node-call.sh`, `e2e-call.sh`) rely on it entirely:
  none is named in `scripts/gate.sh` or `.github/workflows/`. Anchored the same way; all four still
  declare.
- **`check-site.py`'s other mention-test: assessed, deliberately not changed.**
  `check_proofs_are_gated` resolves "is this proof in CI" with `name in text` over `gate.sh` and the
  workflows, so a commented-out or merely-mentioned invocation would satisfy it — its own `RISKS`
  note said so. It is currently inert: no offered proof appears in either source at all, so every
  one goes down the directive path. Fixing it properly means parsing invocations out of shell and
  YAML, which is a different piece of work; filed as a finding rather than done here.
- **`check-vectors.py`: assessed, nothing to change, and not edited** (`CF-12` owns it). Its
  `ROW`/`TEST_NAME`/`COVERS` scanning is the same *shape* — regexes over text that talks about the
  thing being scanned — but it is already immune by construction: `spec_rows` reads a prefix only
  from the spec that owns it, so a design doc or its own docstring citing `PB-F-1` cannot invent a
  row. `TEST_NAME` requires `fn <name>_`, and `COVERS` requires a `// covers:` comment. Prose
  mentioning a row ID is not scanned in any of the three positions.

## Notes

- Found by `DX-12` while writing a comment about `FC-5`'s checker, and reported rather than silently
  worked around — though it did have to work around it to land. Reproduced independently at
  integration.
- The general shape is worth naming, because this is its third appearance: **a tool that scans text
  for a marker will eventually scan text that is about the marker.** `check-docs.py` met it with
  links inside code fences; `check-proof-domains.py` meets it with its own directive;
  `check-site.py`'s "is this script referenced anywhere" test is the same shape waiting to happen.
- Do not fix this by choosing a more obscure token. That trades one silent failure for another and
  makes the directive harder to document, which is the problem.
