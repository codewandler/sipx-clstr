#!/usr/bin/env python3
"""Every proof registers in a domain the document it ships with actually serves.

`FC-4` gave each tenant a `domains` list and made a `REGISTER` for an address-of-record outside it a
`403` (location-service §5.1 S1). That closed a real hole — `alice@attacker.invalid` against
`domains: [example.test]` used to be answered `200` — and opened a way to break every end-to-end
proof at once, because a proof script and the cluster document it ships alongside are two files and
nothing made them agree. They then disagreed in three places, and this repository's headline cluster
proof answered `403` while the gate stayed green. `FC-5` is the cleanup; this is the part that keeps
it from happening again.

So the agreement is checked rather than remembered. A **proof** is any tracked file under `scripts/`
or `deploy/` that runs `sipx register` or `sipx dial` against a `sip:` URI. For each one this reads:

- the **served domains** — every `tenant.domains` entry of the cluster document that governs it,
  which is either embedded in the file or named by a `proof-document: <path>` comment occupying a
  line of its own in it;
- the **registered domains** — the host of every address-of-record it registers or dials, with shell
  variables resolved back to the literal they are assigned;

and fails when a registered domain is not a served one. Comparison is byte-exact, because the
registrar's is: `driver.rs` compares `served == domain` with no case folding, on §4's AoR rule.

Three further rules, each closing a way this check could be true and useless:

1. **`domains: []` is a failure, not a pass.** Empty means "any", which is precisely the fail-open
   `FC-4` exists to remove. A proof that runs green against it proves less than it did before, so
   widening the tenant is not an escape from this check.
2. **A domain that is not a literal is a failure.** `DOMAIN="$NODE_A"`, where `NODE_A` comes from
   `kubectl get pod -o jsonpath=...`, cannot be shown to be in a ConfigMap written before any pod
   had an address — and in the case this was written for, it was not. A proof whose AoR domain is
   decided at runtime is not statically provable, and is refused rather than skipped.
3. **An address-of-record with no governing document is a failure.** Otherwise a new script escapes
   the check by the simple method of not embedding a document.

Exit 0 when clean, 1 otherwise. Run from anywhere; paths resolve against this file's parent.
"""

from __future__ import annotations

import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent

# Where proofs live. Prose that merely *quotes* a REGISTER — the README, the website, a story — is
# out of scope: it ships no document, and holding a worked example in a tutorial to a tenant list it
# never had would be noise, not a gate.
PROOF_ROOTS = ("scripts", "deploy")

# `register "sip:alice@$DOMAIN"`, `dial sip:bob@127.0.0.1`. The command may be `sipx`, `"$SIPX"`, or
# wrapped in `timeout 15 ...`; all that is needed is the verb immediately before the URI.
AOR = re.compile(r"\b(?:register|dial)\s+[\"']?(sip:[^\"'\s]+)")

# `VAR=value` at the start of a line, optionally `export`ed or `local`. Deliberately anchored: a
# `--flag) VAR="$2" ;;` inside a case arm is argument plumbing, not the value the script runs with.
ASSIGN = re.compile(
    r"""^\s*(?:export\s+|local\s+)?([A-Za-z_][A-Za-z0-9_]*)=
        (?: "([^"]*)" | '([^']*)' | ([^\s;#]*) )""",
    re.VERBOSE,
)

VAR = re.compile(r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}|\$([A-Za-z_][A-Za-z0-9_]*)")

# The document a proof names rather than embeds — anchored to a **whole comment line**: the token, a
# path, and nothing else on the line.
#
# Anchored rather than searched for anywhere, because a checker has to survive being documented.
# Unanchored, the first `proof-document:` in a file won, so a comment *explaining* the directive was
# read as the directive: one sentence about it in `k8s-two-node-call.sh` aimed this at a backtick and
# turned the gate red on a script that was correct (`CF-14`). That taught the opposite of what it
# should — the next person needs the description, and the checker was training people not to write
# it.
#
# `check-docs.py` met the same class with links inside code fences and answered it by stripping code
# before scanning (`prose`). Anchoring is chosen here instead for two reasons: a shell comment has no
# fence or code span to strip, and this states the rule positively — the directive is a *declaration*
# occupying its own line, and every other appearance of the token is prose. Renaming the token to
# something prose is unlikely to contain was the third option and is the worst of them: it trades one
# silent failure for a rarer one and makes the directive harder to write about, which is the defect.
#
# The comment marker is optional so a Python proof can declare it in a module docstring, the way
# `sip_demo.py` carries its `not-in-ci:`.
DOC_DIRECTIVE = re.compile(r"^[ \t]*#?[ \t]*proof-document:[ \t]*(\S+)[ \t]*$", re.M)
DOMAINS = re.compile(r"^(\s*)domains:\s*(.*)$")


def tracked_files() -> list[pathlib.Path]:
    """Every file this repository tracks under `PROOF_ROOTS`.

    Asked git rather than walked, for the reason `check-docs.py` documents at length: agent
    worktrees under `.claude/worktrees/` are full checkouts, so a walk finds every proof script
    many times over and the verdict starts depending on which sibling worktrees happen to exist.
    """
    out = subprocess.run(
        ["git", "-C", str(ROOT), "ls-files", "-z", "--", *PROOF_ROOTS],
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    return [ROOT / name for name in out.split("\0") if name]


def uncommented(text: str) -> list[tuple[int, str]]:
    """Numbered lines, minus whole-line comments — a commented-out command is not a proof."""
    return [
        (n, line)
        for n, line in enumerate(text.splitlines(), 1)
        if not line.lstrip().startswith("#")
    ]


def assignments(text: str) -> dict[str, str]:
    """Every literal shell assignment in a file, last one winning."""
    found: dict[str, str] = {}
    for _, line in uncommented(text):
        match = ASSIGN.match(line)
        if match:
            value = next(
                (group for group in match.groups()[1:] if group is not None), ""
            )
            found[match.group(1)] = value
    return found


def resolve(value: str, known: dict[str, str], depth: int = 0) -> str | None:
    """Expand a shell value to a literal, or `None` if it is only known at runtime.

    Fail-closed at every turn: a command substitution, an unknown name, a form this does not
    understand, or any surviving `$` all return `None`, which the caller reports as a failure. The
    alternative — treating "I could not work it out" as "probably fine" — is the behaviour that let
    a `403` ship.
    """
    if depth > 8 or "$(" in value or "`" in value:
        return None
    match = VAR.search(value)
    if match is None:
        return None if "$" in value else value
    name = match.group(1) or match.group(2)
    if name not in known:
        return None
    inner = resolve(known[name], known, depth + 1)
    if inner is None:
        return None
    return resolve(value[: match.start()] + inner + value[match.end() :], known, depth + 1)


def served_domains(text: str) -> tuple[list[str], bool]:
    """The `tenant.domains` entries of a cluster document, and whether it declared any at all.

    Both YAML spellings, because the schema accepts both: `domains: [a, b]` and a block sequence.
    The second return value distinguishes "declared, and empty" — the fail-open — from "no tenant
    section here", which for a file holding an address-of-record means the document is elsewhere.
    """
    domains: list[str] = []
    declared = False
    lines = text.splitlines()
    for index, line in enumerate(lines):
        match = DOMAINS.match(line)
        if match is None:
            continue
        declared = True
        indent, rest = match.group(1), match.group(2).strip()
        if rest.startswith("["):
            inner = rest[1 : rest.index("]")] if "]" in rest else rest[1:]
            domains += [
                item.strip().strip("\"'") for item in inner.split(",") if item.strip()
            ]
            continue
        for follow in lines[index + 1 :]:
            item = re.match(r"^(\s*)-\s*(\S+)", follow)
            if item and len(item.group(1)) > len(indent):
                domains.append(item.group(2).strip("\"'"))
            elif follow.strip() and not follow.lstrip().startswith("#"):
                break
    return domains, declared


def host_of(uri: str) -> str:
    """The host of a `sip:` URI — the domain the registrar keys the binding under."""
    body = uri[len("sip:") :].split(";")[0].split("?")[0]
    return re.sub(r":\d+$", "", body.rsplit("@", 1)[-1])


def governing_document(path: pathlib.Path, text: str) -> tuple[pathlib.Path, str] | str:
    """The document a proof runs against, or a string saying why there isn't one.

    An explicit `proof-document:` beats an embedded one: a script that deploys a manifest and also
    happens to quote some YAML should be held to what it deployed.

    Only a line that is *nothing but* the directive counts — see `DOC_DIRECTIVE`. A file that talks
    about the directive without declaring one therefore falls through to the embedded document, or
    to the "names none" failure below, rather than being pointed at a fragment of the sentence.
    """
    directive = DOC_DIRECTIVE.search(text)
    if directive:
        named = ROOT / directive.group(1)
        if not named.is_file():
            return f"its `proof-document: {directive.group(1)}` does not exist"
        return named, named.read_text(encoding="utf-8")
    if DOMAINS.search(text) or any(DOMAINS.match(line) for line in text.splitlines()):
        return path, text
    return (
        "it embeds no cluster document and names none — add a `proof-document: <path>` "
        "comment, on a line of its own, pointing at the document it registers against"
    )


def check() -> tuple[list[str], int, int]:
    problems: list[str] = []
    proofs = 0
    addresses = 0

    for path in tracked_files():
        try:
            text = path.read_text(encoding="utf-8")
        except (UnicodeDecodeError, OSError):
            continue  # An image or a binary is not a proof.

        aors = [
            (number, match.group(1))
            for number, line in uncommented(text)
            for match in AOR.finditer(line)
        ]
        if not aors:
            continue
        proofs += 1
        where = path.relative_to(ROOT).as_posix()

        document = governing_document(path, text)
        if isinstance(document, str):
            problems.append(f"{where}: registers an address-of-record but {document}")
            continue
        doc_path, doc_text = document
        doc_where = doc_path.relative_to(ROOT).as_posix()

        domains, declared = served_domains(doc_text)
        if not declared:
            problems.append(
                f"{where}: its document {doc_where} declares no `tenant.domains`, so the tenant "
                f"serves any domain — the fail-open `FC-4` removed"
            )
            continue
        if not domains:
            problems.append(
                f"{where}: its document {doc_where} declares `domains: []`, which means \"any\". "
                f"That is the fail-open `FC-4` exists to remove; name the domain the proof uses"
            )
            continue

        known = assignments(text)
        for number, uri in aors:
            addresses += 1
            raw = host_of(uri)
            host = resolve(raw, known, 0) if "$" in raw else raw
            if host is None:
                problems.append(
                    f"{where}:{number}: the domain of `{uri}` is `{raw}`, which is only known at "
                    f"runtime. {doc_where} is written before then, so nothing can show the tenant "
                    f"serves it — use a literal that is in `domains: {domains}`"
                )
            elif host not in domains:
                problems.append(
                    f"{where}:{number}: registers in `{host}`, which {doc_where} does not serve "
                    f"— it declares `domains: {domains}`. That REGISTER is answered 403"
                )

    return problems, proofs, addresses


def main() -> int:
    problems, proofs, addresses = check()
    for problem in problems:
        print(problem)
    if problems:
        print(f"\nproof domains: FAIL — {len(problems)} problem(s)")
        return 1
    print(
        f"proof domains: clean ({addresses} address(es) of record "
        f"across {proofs} proof(s) checked)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
