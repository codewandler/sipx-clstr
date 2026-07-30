---
title: "Conformance"
description: "How correctness is measured here: numbered vectors per normative rule, proved by tests or deferred with a reason."
---

# Conformance

Most projects tell you what they support. This one tells you what it has **proved**, row by row,
and names the work that will close everything it has not.

The mechanism is simple enough to audit in an afternoon: every normative rule in a specification
carries a numbered vector row, every row is either executed by a test or deferred in writing, and
a script in the repository decides which — not a person, and not a table someone maintains by
hand.

## How a rule becomes a measurement

**1. The rule gets a numbered row.** Each specification that carries normative behaviour ends in
vector tables, and each table row has an ID: `PB-V-8` is the eighth request-validation row of the
proxy behaviour spec, `RA-D-4` the fourth registrar-auth decision row. Some tables number by
family and one numbers straight through, so `HF-9` is a row too. **A row ID is a citation** —
specs, design records and commit messages all quote them, which is why the tables are not
renumbered once published.

**2. A test proves it by name.** Coverage is derived from **test function names**:

```rust
fn pb_v_8_a_max_breadth_of_one_is_not_a_fork() { … }
```

That function proves `PB-V-8`. Nothing registers it and nothing lists it. A test that covers a row
but wants a different name says so in a comment instead:

```rust
// covers: PB-R-4
```

The reason coverage is read out of names rather than a checked-in list is that a list rots and a
name cannot: **deleting the test deletes the claim.** A hand-maintained index would keep asserting
coverage that no longer exists, and it would be right often enough that nobody re-reads it.

**3. Anything unproved is deferred, in writing, with an owner.** A row that no test covers must
appear in
[`docs/reference/vector-scope.toml`](https://github.com/codewandler/sipx-clstr/blob/main/docs/reference/vector-scope.toml)
carrying **both** a reason and an owner — the work that will close it. One without the other fails the
gate. "Not yet" is an acceptable answer here; "not yet" with nobody attached is not.

**4. The report is generated, and staleness is a build failure.**
[`scripts/check-vectors.py`](https://github.com/codewandler/sipx-clstr/blob/main/scripts/check-vectors.py)
reads the row IDs out of the specs, reads coverage out of the workspace, and writes the report.
Run with `--check` — which is how the gate runs it — it fails if the committed report differs from
what the code and specs say it should be. The published numbers therefore cannot be edited into
agreement with a claim.

## Three ways it fails, and the third is the point

1. A row is in a spec, covered by no test, and not deferred.
2. A deferred row has no reason, or no owner.
3. **A deferred row is covered.**

The third one looks like good news and is treated as an error. A row marked "not yet" that some
test has quietly started proving is a **stale deferral**, and it is how a coverage report begins
lying about what it measures: the row stops being watched, so when the real gap reopens — the test
renamed, the assertion weakened — nothing notices, because the report was already saying "not
proved" about it.

The same logic runs in the other direction: a test claiming a row that exists in no spec is also
an error, because it is a proof of something nobody specified.

**The report is a measurement, not a claim.** It is allowed to say no, and it is designed so that
saying yes takes more work than saying no.

## The legend

| Status | What it means |
|---|---|
| **proved** | At least one test in the workspace executes this row. The report names the file. |
| **deferred** | No test covers it. The report names the work that will, and quotes the reason. |

There is no third status. A row cannot be partly proved, and there is no "supported" that is not
one of these two.

## The report itself

**[docs/reference/conformance.md](https://github.com/codewandler/sipx-clstr/blob/main/docs/reference/conformance.md)** —
every row, its status, and the test file or the open work that accounts for it.

That table is deliberately **not** reproduced on this page. It is regenerated from the code on
every gate run, and a copy here would be a second set of numbers to keep in sync — which is the
failure this whole mechanism exists to prevent. Read it at the link, where it is checked.

## Every specification is under the gate

All ten specifications are registered with the checker: proxy behaviour (`PB`), the end-to-end probe
(`EP`), registrar auth (`RA`), the hook framework (`HF`), location service (`LS`), media relay (`MR`),
number normalisation (`NN`), affinity token (`AT`, `FR`), cluster config (`CC`) and asserted identity
(`AI`).

That was not always true, and how it failed is worth knowing, because it is the failure mode this whole
page exists to guard against. Six of those specs used to carry vector tables the checker had **no
registration for** — roughly 340 normative rows that were cited, reviewed and executed by nothing. The
gap grew on its own: every release that shipped a specification added rows the gate could not see.

It was demonstrated rather than argued. A fabricated row was added to an unregistered family and the
gate passed it untouched, exit `0`. After registration the same fabricated row fails. That is the only
kind of evidence worth having about a checker: not that it passes, but that it can fail.

**Registering them moved the numbers a long way, in both directions.** The denominator quadrupled,
because rows that were invisible are now counted. The proportion proved fell, because most of those
rows describe subsystems that do not exist yet — trunks, media control, sharding, affinity. And the
*numerator rose too*: several dozen rows turned out to be already covered by tests the report simply
could not credit.

A falling percentage that reflects reality is worth more than a high one that was measuring a subset.

**How a deferral for an unbuilt subsystem stays honest.** Every one of those rows is deferred with a
reason that says *what is specifically missing* and the story that closes it — "there is no trunk
object to route through", not "not implemented". They are grouped by that story, so when it lands its
rows move from deferred to proved as a set, and a deferral that outlives its subsystem is visible as a
group rather than buried as one line among hundreds.

## What this does not measure

- **It does not measure this platform against the RFCs.** It measures the platform against its own
  specifications, which is a different thing. The specs are where RFC sections are cited and
  interpreted; if a spec reads an RFC wrongly, every vector derived from it will agree with the
  mistake. That is why the specs are published in the repository and worth reading directly:
  [docs/specs](https://github.com/codewandler/sipx-clstr/tree/main/docs/specs).
- **It does not measure interoperability.** A vector is a unit of decided behaviour, not a phone.
  Interop is proved separately, by
  [`scripts/e2e-call.sh`](https://github.com/codewandler/sipx-clstr/blob/main/scripts/e2e-call.sh),
  which runs real client software against a real node over UDP.
- **A proved row is not a shipped feature.** Rows exist for behaviour that is specified but has no
  runtime yet, and those are deferred against the work that will build it. Read
  [Does this fit?](../guides/does-this-fit.md) for what actually runs.

## Related

- [Does this fit?](../guides/does-this-fit.md) — the capability statement in prose.
- [What's new](../whats-new.md) — where the project stands, release by release.
- [The specifications](https://github.com/codewandler/sipx-clstr/tree/main/docs/specs) — normative,
  RFC-cited, and the source of every row ID on this page.
