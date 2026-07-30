---
id: CF-14
title: A checker must not read prose about its own directive as the directive
pillar: Foundation
status: ready
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

- [ ] **Failing-first**: add the explanatory comment to a proof script, watch the gate go red, and
      quote it. Reproduction is above and takes one line.
- [ ] The directive is recognised only where it is meant — an anchored form, a designated header
      region, or the same code-stripping `check-docs.py` already does. Choose one and say why in the
      script.
- [ ] A proof script can contain a comment that fully explains the directive, including the literal
      token, and still pass. That is the acceptance test: the checker must survive being documented.
- [ ] The same question is asked of the other repo-consistency checkers rather than assumed —
      `check-site.py` (`DX-12`) resolves script references by *mentioning*, which its own RISKS note
      says a commented-out invocation would satisfy; `check-vectors.py` reads rows out of spec
      tables. Report what you find for each even where nothing needs changing.
- [ ] `scripts/gate.sh` green.

## Progress

- (running log)

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
