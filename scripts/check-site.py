#!/usr/bin/env python3
"""Every authored page is reachable, and every command the site prints is one that exists.

Two properties of the public site were held by hand until `DX-12`, and both had already failed.

**Reachable.** `website/sidebars.js` is curated on purpose — the ladder from "what is this" to
autoscaling is an argument, and its order lives in one file rather than in twenty
`sidebar_position` fields. The cost of curating it is that a page can be authored, committed and
published while being linked from nothing: Docusaurus builds it, serves it at its route, and no
reader ever arrives. The mirror defect is a sidebar entry naming a doc id that does not exist,
which fails the site build — late, and somewhere else.

**Real.** `check-docs.py` strips fenced blocks and inline spans before it looks for links, and its
reasoning is sound: a story quoting a real `broken link ...` message must not turn the gate red.
The consequence is that **no documented command was ever read by the gate**, so when `DP-8`
replaced `--listen`/`--advertise`/`--tenant` with a configuration document, roughly thirty
commands across the README and six site pages went on telling readers to pass flags the binary had
stopped accepting — through a release, with every job green. `DX-13` fixed the words. This is what
stops it happening again, and it reads exactly what the link checker deliberately ignores.

## What is checked

1. **No orphan page.** Every tracked `website/docs/**/*.md` is reachable from `sidebars.js`.
2. **No dangling id.** Every doc id in `sidebars.js` resolves to a page.
3. **The CLI reference matches the binary, both directions.** Every flag `reference/cli.md` names
   is one the binary accepts, *and* every flag the binary accepts is named on the page. One
   direction is not enough: a page documenting a subset is how a flag goes undocumented for three
   releases, and it is the direction a human proof-reader never runs.
4. **Every documented `sipx-clstr` invocation parses.** Command lines in fenced blocks across
   `README.md` and the site are held to the same flag set, and a repository path a command names
   (`scripts/…`, `deploy/…`, `Dockerfile`) has to exist.
5. **Every documented version banner is the one the binary prints**, byte for byte, kernel half
   included. `CF-19`. Checking a command's *flags* says nothing about what it is shown as
   *printing*, so three pages went on quoting `sipx-clstr 0.11.0 (sipx kernel 0.10.0)` after the
   workspace moved to `0.12.0`, with the full gate green. It is a release-time defect specifically —
   the number only goes stale at a cut, and the site deploys *on* a cut, so the first reader of the
   wrong version is a public one. Every fenced block is read, not the command ones only: the banner
   is output, so it lives in exactly the `text` blocks item 4 skips.
6. **Every published conformance count is the generated one.** `DX-14`. The badge, its alt text
   and the status table each carried a hand-copied triple, and at `v0.12.0` they read `134/492`
   and `358 deferred` while the generator said `125/549` and `405`. The counts are read out of
   `docs/reference/conformance.md` — the file `check-vectors.py` writes and the step before this
   one has just re-checked — so there is one measurement and every public copy is compared to it,
   by file, line, expected and actual.
7. **The published specification inventory is the checker's own registry.** Read out of
   `check-vectors.py`'s `SPECS` and `EXCLUDED` rather than re-listed here, for the reason `CF-25`
   filed: the registry is what the gate enforces, so a second copy of it on a public page is a
   claim nothing holds. A count in front of the word *specifications*, and any sentence that
   enumerates registered prefixes, are both compared against it.
8. **A release-facing capability claim waits for the driver that performs it.** `V-18`. Four
   behaviours are modelled by a pure decision core and were, or still are, discarded by the real
   driver, and the site said *today* about all four. Each is mapped to the story that owns its
   real-socket proof, and the mapping is enforced in **both** directions off the board's own
   `status` field: while the story is open the claim is refused, and once it is `done` the denial
   is refused — because a stale "not implemented" is how `PX-13` shipped a whole release still
   documented as broken.
9. **Every proof the site offers is in CI, or says why not.** "In CI" means the gate or a workflow
   **invokes** it — parsed as a command, not matched as a substring, so a commented-out line or a
   step merely *named* after a proof does not count. See `PROOF_DIRECTIVE` for the other branch and
   `invokes` for this one; `self_test` pins both against the ways they can be faked.

## Where the CLI surface comes from, and why it is two sources

The flags are taken from the **binary** when one is built — `$SIPX_CLSTR_BIN`, else
`target/{debug,release}/sipx-clstr` — by running `--help` and `<sub> --help` and reading the
`Options:` sections. That is the surface a reader actually meets.

There is no binary in the `docs` workflow, which sets up Python and no Rust toolchain, and building
one there is a different cost class from the rest of that job. So the flags are *also* derived
statically from the `clap` derive in `main.rs`, and when both sources are available **they must
agree** — which is what keeps the static reader from drifting away from the parser it models. In
the gate the binary is present (the gate builds and tests the workspace first), so the gate checks
against the real thing; in the `docs` workflow it is not, and the summary says so rather than
implying a verification that did not happen.

The **version banner** is read the same way and for the same reason: from `--version` when there is
a binary, and otherwise composed from `Cargo.toml` — the workspace `version` and the `tag = "v…"`
the kernel is pinned at, which are the two numbers the banner is made of. When both readings are
available they must agree, so a binary built before a version bump is reported rather than believed.
Neither reading is silent: every run names which one it used, and a run that could do neither says
so in those words instead of exiting 0 on a check it did not perform.

## What this does not do, said out loud

It does not execute documented commands. Nine of them need Docker, a k3d cluster, `kubectl`,
`devspace` or the external `sipx` CLI, and running a cluster bring-up in the gate is a different
cost class from reading files. The failure this exists to catch is a flag that no longer exists,
not a runtime error. Every command is therefore classified and **counted in the summary** —
verified, or named as unrunnable here — because a check that silently narrows what it looks at is
the failure mode this file was written in response to.

Exit 0 when clean, 1 otherwise. Run from anywhere; paths resolve against this file's parent.
"""

from __future__ import annotations

import importlib.util
import os
import pathlib
import re
import shutil
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
SITE_DOCS = ROOT / "website" / "docs"
SIDEBARS = ROOT / "website" / "sidebars.js"
CLI_PAGE = SITE_DOCS / "reference" / "cli.md"
MAIN_RS = ROOT / "crates" / "sipx-clstr-node" / "src" / "main.rs"
MANIFEST = ROOT / "Cargo.toml"

# The published tree plus the one page that is not in it. `README.md` is the project explained for
# humans arriving cold and it carries the same quickstart; it rotted in exactly the same way and for
# the same reason, so it is held to the same standard.
COMMAND_SOURCES = ("README.md", "website/docs")

# Fenced blocks whose contents are commands. `text` blocks are *output* — expected stdout, a help
# dump, an error message — and holding those to a command grammar would turn every quoted refusal
# into a defect.
COMMAND_FENCES = ("bash", "sh", "shell", "console")

FENCE = re.compile(r"^[ \t]*(`{3,}|~{3,})([^\n]*)\n(.*?)^[ \t]*\1[ \t]*$", re.S | re.M)

# The binary as it appears in a command: bare, or with any path in front of it. Deliberately exact —
# `sipx-clstr:dev` is a container image tag and `sipx-clstr-node-a` is a Deployment, and neither
# takes these flags.
BINARY_TOKEN = re.compile(r"^(?:\S*/)?sipx-clstr$")

# `VAR=value` prefixes (`RUST_LOG=debug sipx-clstr ...`) and wrappers that pass their tail through.
ENV_PREFIX = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*=")
WRAPPERS = ("setsid", "exec", "timeout", "env", "sudo", "nohup")

# A flag as written on a page or a command line. Long form only: the short forms are aliases of
# longs in this binary, and `-p 5060:5060/udp` in a `docker run` is not ours to check.
FLAG = re.compile(r"(--[a-z][a-z0-9-]*)")

# An `Options:` entry in `--help` output: two spaces, an optional short alias, then the long form.
# `re.M` matters: this is run over whole `--help` dumps as well as line by line.
HELP_FLAG = re.compile(r"^\s+(?:-\w,\s+)?(--[a-z][a-z0-9-]*)", re.M)

# The first cell of a markdown table row, when it is a flag: `| \`--config <PATH>\` | — | ... |`.
TABLE_FLAG = re.compile(r"^\|\s*`?(--[a-z][a-z0-9-]*)")

# A version banner as `sipx-clstr --version` prints it and as a page quotes it back. Recognised by
# **shape** — the binary's name, then something version-shaped — rather than by content, because a
# recogniser that only matched *correct* banners would see nothing precisely when there is something
# to see. A stale banner is a banner; so is one that has lost its `(sipx kernel …)` half, which is
# the same defect one level further down. The comparison itself is byte for byte against what the
# binary prints; only the line's own indentation is dropped, because a fence inside a list item is
# indented and that is not a version defect.
VERSION_BANNER = re.compile(r"^[ \t]*(sipx-clstr[ \t]+v?\d[^\n]*?)[ \t]*$")

# The two numbers the banner is made of, as `Cargo.toml` spells them: the workspace version, and the
# tag the kernel is pinned at. The pin is read the way `crates/sipx-clstr-node/tests/kernel_pin.rs`
# reads it — first `sipx-sip` git dependency, its `tag` — so the static reading here and the test
# that holds `KERNEL_VERSION` to the manifest are answering from the same line of the same file.
MANIFEST_SECTION = re.compile(r"^\[([^\]]+)\]")
MANIFEST_VERSION = re.compile(r'^version\s*=\s*"([^"]+)"')
MANIFEST_KERNEL_PIN = re.compile(r'^sipx-sip\s*=.*codewandler/sipx.*\btag\s*=\s*"v?([^"]+)"')

# `#[arg(...)]` and the field it decorates, in the `clap` derive. `long` alone takes the field name
# kebab-cased; `long = "x"` overrides it.
CLAP_ARG = re.compile(
    r"#\[arg\(([^\]]*)\)\]\s*(?:///[^\n]*\n\s*)*([a-z_][a-z0-9_]*)\s*:", re.S
)
CLAP_LONG_RENAME = re.compile(r"\blong\s*=\s*\"([^\"]+)\"")

# Tools this repository does not ship and a runner does not have. A command that starts with one is
# reported as unrunnable rather than quietly skipped.
EXTERNAL_TOOLS = ("docker", "kubectl", "k3d", "devspace", "helm", "sipx", "psql")

# Paths inside this repository, as a command would name one. A `cluster.yaml` the page just told the
# reader to write, a `/etc/sipx-clstr/...` path inside a container and a `/path/to/sipx` placeholder
# are all correct as written and none of them is here.
#
# A **file extension is required**, and that bound is deliberate rather than lazy: `kubectl logs
# deploy/sipx-clstr-node-a` names a Deployment in a namespace, not a directory in this repository,
# and the two are spelled identically. Requiring a suffix keeps every path the documentation
# actually points at — `scripts/e2e-call.sh`, `deploy/devspace/manifests/node.yaml` — and gives up
# checking bare directory references, which no documented command currently passes to a tool.
REPO_PATH = re.compile(
    r"(?<![\w/.-])((?:scripts|deploy|crates|website|docs)/[\w./-]*\.\w+|Dockerfile)(?![\w/.-])"
)

# `PROOF_DIRECTIVE`. The end-to-end proofs are what `README.md` offers as evidence for the one
# feature that works end to end, and none of them runs in CI: they need a database, a `sipx` CLI
# built from another repository, and in one case a Kubernetes cluster. That is a defensible
# position and a bad accident, and from the outside the two are indistinguishable — which is how
# `FC-4` broke three of them for a release cycle. So the exemption is written down *in the script
# it exempts*, with a reason, and this fails on a proof that has neither a CI reference nor a
# recorded decision. Widening the gate is a deliberate act; so is declining to.
#
# Anchored to the start of a line, for `CF-14`'s reason and with its polarity reversed. This is a
# declaration, and an unanchored search matched the token anywhere — including in a *description* of
# the convention, or in a quotation of the error message below, both of which a proof script is
# exactly the place to write. `check-proof-domains.py` had the same defect and it at least announced
# itself as a false red; this one is a false green, so the script that stopped declaring its
# exemption keeps it, silently, and nothing ever goes red to say so. All four proofs this currently
# governs declare it at the start of a line already — three as `# not-in-ci: …`, and `sip_demo.py` in
# its module docstring, which is why the comment marker is optional.
PROOF_DIRECTIVE = re.compile(r"^[ \t]*#?[ \t]*not-in-ci:[ \t]*(\S[^\n]*)", re.M)
CI_REFERENCES = ("scripts/gate.sh", ".github/workflows")

# `CF-15`. "Runs in CI" used to be resolved with `name in text` over the files above — a bare
# substring test, which a commented-out line satisfies, and so does a step *named* after a proof, and
# so does a sentence in a comment explaining that the proof is **not** run. It was inert only because
# nothing in either file mentioned any proof at all; the moment one did, it would have started
# passing for the wrong reason, which is the same false green `PROOF_DIRECTIVE` was anchored to close.
#
# So the name has to appear where a shell would *execute* it. `shell_commands` removes comments and
# splits on the operators that start a new command; `invokes` then requires the name in command
# position rather than as an argument.
#
# Where this is still approximate, stated plainly rather than implied to be exact — a checker whose
# predecessor failed open has no business overselling its successor:
#
#   - **Heredoc bodies are read as if they were code.** A `<<YAML` block in `scripts/gate.sh` with a
#     line beginning `scripts/e2e-call.sh` would be counted, wrongly. Neither the gate nor any
#     workflow currently contains such a heredoc, and the proof scripts that do are not read by this.
#   - **A line continued with `\` is judged on its own.** The continuation of `echo foo \` reads as a
#     fresh command. Continuations of real invocations therefore work, and the pathological case is a
#     wrapped `echo`, which nothing here does.
#   - Command substitution needs no special case: splitting on `(` puts `$(scripts/e2e-call.sh)`'s
#     body in command position, which is correct — that form does run it.
#
# All three are narrow and none of them is the failure being fixed. The one that mattered — a
# commented-out line, or a mention in prose — is closed, and `self_test` holds it closed.
ENV_ASSIGNMENT = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*=")
COMMAND_SEPARATOR = re.compile(r"&&|\|\||[;|&()]")

# Words that may precede the real command without making it an argument. `timeout` also takes a
# duration, which is why a bare number is tolerated. `echo` is deliberately **not** here.
COMMAND_PREFIXES = frozenset(
    {"exec", "time", "sudo", "nohup", "setsid", "timeout", "env", "bash", "sh", "python3", "python"}
)

# One YAML construct, read by hand: `run:`, inline or as a block scalar. There is no third-party
# dependency in this script and the `docs` job installs a bare interpreter, so PyYAML is not
# available — but the narrow reader is also the point rather than a concession. Everything outside a
# `run:` body is ignored, so a step whose `name:` quotes a proof does not count as running it.
RUN_KEY = re.compile(r"^(\s*)(?:-\s+)?run:\s*(.*)$")


def strip_comment(line: str) -> str:
    """`line` with any trailing shell comment removed.

    Quote-aware to the depth this needs: a `#` inside quotes is text, and a `#` that does not start a
    word is not a comment either (`--color=#fff`). Backslash escapes are not tracked.
    """
    kept: list[str] = []
    quote: str | None = None
    for index, char in enumerate(line):
        if quote:
            kept.append(char)
            if char == quote:
                quote = None
        elif char in "'\"":
            quote = char
            kept.append(char)
        elif char == "#" and (index == 0 or line[index - 1].isspace()):
            break
        else:
            kept.append(char)
    return "".join(kept)


def workflow_run_lines(text: str) -> list[str]:
    """Every line a workflow's `run:` steps hand to the shell."""
    lines = text.splitlines()
    body: list[str] = []
    index = 0
    while index < len(lines):
        match = RUN_KEY.match(lines[index])
        index += 1
        if not match:
            continue
        indent, rest = match.group(1), match.group(2).strip()
        if not rest.startswith(("|", ">")):
            body.append(rest)  # An inline scalar: the whole command is on this line.
            continue
        # A block scalar: every following line indented past the key belongs to it. A `#` in there is
        # shell, not YAML — the block is handed to `bash` verbatim — so it is stripped as shell below.
        while index < len(lines):
            line = lines[index]
            if line.strip() and len(line) - len(line.lstrip()) <= len(indent):
                break
            body.append(line)
            index += 1
    return body


def shell_commands(lines: list[str]) -> list[str]:
    """The command segments a shell would execute, comments removed."""
    return [
        segment.strip()
        for line in lines
        for segment in COMMAND_SEPARATOR.split(strip_comment(line))
        if segment.strip()
    ]


def invokes(segment: str, name: str) -> bool:
    """Is `name` the command this segment runs, rather than a word inside it?"""
    for token in segment.split():
        candidate = token.strip("'\"")
        candidate = candidate[2:] if candidate.startswith("./") else candidate
        if candidate == name:
            return True
        if ENV_ASSIGNMENT.match(token) or token.startswith("-") or token.isdigit():
            continue
        if candidate.rsplit("/", 1)[-1] in COMMAND_PREFIXES:
            continue
        return False  # A real command, and it is not ours — the name can only be an argument now.
    return False


def runs_in_ci(text: str, name: str, *, yaml: bool) -> bool:
    """Does `text` — a shell script, or a workflow — actually invoke `name`?"""
    lines = workflow_run_lines(text) if yaml else text.splitlines()
    return any(invokes(segment, name) for segment in shell_commands(lines))


def tracked(*roots: str) -> list[pathlib.Path]:
    """Every file this repository tracks under `roots`.

    Asked git rather than walked, for the reason `check-docs.py` documents at length: agent
    worktrees under `.claude/worktrees/` are full checkouts, so a walk finds every page many times
    over and the verdict starts depending on which sibling worktrees happen to exist.
    """
    try:
        listed = subprocess.run(
            ["git", "-C", str(ROOT), "ls-files", "-z", "--", *roots],
            capture_output=True,
            check=True,
        ).stdout
    except (OSError, subprocess.CalledProcessError) as error:
        # Loudly, never silently. A gate that checks nothing because it could not work out what to
        # check is the failure mode this whole file exists to answer.
        raise SystemExit(
            f"site: FAIL — cannot list tracked files ({error}).\n"
            "  This check needs the repository's file list from git. Run it inside a git\n"
            "  checkout; it deliberately does not fall back to walking the directory tree."
        ) from error
    names = [name for name in listed.decode("utf-8").split("\0") if name]
    return sorted(path for name in names if (path := ROOT / name).is_file())


def strip_js_comments(text: str) -> str:
    """`sidebars.js` minus its comments, which are half the file and full of doc-id-shaped words."""
    text = re.sub(r"/\*.*?\*/", "", text, flags=re.S)
    return re.sub(r"^\s*//[^\n]*$", "", text, flags=re.M)


def sidebar_ids() -> set[str]:
    """Every doc id `sidebars.js` routes to.

    A sidebar is JavaScript, so this reads it as text rather than evaluating it — running the site's
    config to check the site's config needs a Node toolchain the `docs` workflow does not have, and
    would make a gate depend on `npm install`. The parse is therefore deliberately narrow: property
    values that are *not* doc ids (`type`, `label`, `description`, …) are removed, and what is left
    inside the arrays is the id set. `id:` is kept, because `{type: 'doc', id: 'x'}` is the long
    spelling of `'x'`.

    Fail-closed: finding nothing means the parse stopped understanding the file, not that the site
    has no pages, and it is reported as a failure by the caller.
    """
    text = strip_js_comments(SIDEBARS.read_text(encoding="utf-8"))
    text = re.sub(
        r"\b(?:type|label|description|className|href|dirName|position)\s*:\s*"
        r"(?:'[^']*'|\"[^\"]*\")",
        "",
        text,
    )
    return {match.group(1) for match in re.finditer(r"['\"]([\w][\w./-]*)['\"]", text)}


def page_id(path: pathlib.Path) -> str:
    """The doc id Docusaurus gives a page: its path under `website/docs/`, without the suffix."""
    return path.relative_to(SITE_DOCS).with_suffix("").as_posix()


def check_reachability() -> list[str]:
    problems: list[str] = []
    if not SITE_DOCS.is_dir() or not SIDEBARS.is_file():
        return problems

    ids = sidebar_ids()
    if not ids:
        return [
            f"{SIDEBARS.relative_to(ROOT)}: no doc id could be read out of this file. That is a "
            f"parse failure, not an empty sidebar — refusing to report every page as unreachable"
        ]

    pages = {page_id(path): path for path in tracked("website/docs") if path.suffix == ".md"}

    for identifier, path in sorted(pages.items()):
        if identifier not in ids:
            problems.append(
                f"{path.relative_to(ROOT)}: authored but not reachable — no entry in "
                f"website/sidebars.js routes to `{identifier}`, so the page publishes at its URL "
                f"and nothing links to it. Add it to the ladder or delete it"
            )
    for identifier in sorted(ids - set(pages)):
        problems.append(
            f"website/sidebars.js: names `{identifier}`, but website/docs/{identifier}.md does "
            f"not exist — the site build fails on this"
        )
    return problems


def binary_path() -> pathlib.Path | None:
    """A built `sipx-clstr`, if there is one. Never builds: that is the gate's job, not this."""
    named = os.environ.get("SIPX_CLSTR_BIN")
    if named:
        path = pathlib.Path(named)
        return path if path.is_file() else None
    for candidate in ("target/debug/sipx-clstr", "target/release/sipx-clstr"):
        path = ROOT / candidate
        if path.is_file():
            return path
    found = shutil.which("sipx-clstr")
    return pathlib.Path(found) if found else None


def flags_from_binary(binary: pathlib.Path) -> set[str] | None:
    """Every long flag the binary accepts, top level and in each subcommand.

    `help` is skipped: it is `clap`'s generated subcommand and takes no flags of its own.
    """

    def help_of(*argv: str) -> str | None:
        try:
            done = subprocess.run(
                [str(binary), *argv, "--help"], capture_output=True, text=True, timeout=30
            )
        except (OSError, subprocess.SubprocessError):
            return None
        return done.stdout if done.returncode == 0 else None

    top = help_of()
    if top is None:
        return None

    flags = {match.group(1) for match in HELP_FLAG.finditer(top)}
    commands = re.search(r"^Commands:\n((?:[ \t]+\S.*\n)+)", top, re.M)
    for line in commands.group(1).splitlines() if commands else []:
        name = line.split()[0]
        if name == "help":
            continue
        sub = help_of(name)
        if sub is None:
            return None
        flags |= {match.group(1) for match in HELP_FLAG.finditer(sub)}
    return flags


def flags_from_source() -> set[str]:
    """Every long flag the `clap` derive in `main.rs` declares, plus the one `clap` adds itself."""
    text = MAIN_RS.read_text(encoding="utf-8")
    flags = {"--help"}  # `clap` generates it; it appears in no `#[arg(...)]`.
    for match in CLAP_ARG.finditer(text):
        attributes, field = match.group(1), match.group(2)
        if not re.search(r"\blong\b", attributes):
            continue
        rename = CLAP_LONG_RENAME.search(attributes)
        flags.add(f"--{rename.group(1) if rename else field.replace('_', '-')}")
    return flags


def cli_surface() -> tuple[set[str], str, list[str]]:
    """The flag set to hold the documentation to, where it came from, and any disagreement.

    Both sources when both are available, and they must match: the static reader models a parser it
    cannot run, and an unchecked model is a second source of truth that drifts.
    """
    from_source = flags_from_source()
    binary = binary_path()
    if binary is None:
        return (
            from_source,
            f"{MAIN_RS.relative_to(ROOT)} (no built binary here — the gate checks against one)",
            [],
        )

    from_binary = flags_from_binary(binary)
    if from_binary is None:
        return (
            from_source,
            f"{MAIN_RS.relative_to(ROOT)} ({binary} would not report its help)",
            [],
        )

    where = f"{binary.relative_to(ROOT) if binary.is_relative_to(ROOT) else binary} --help"
    problems = []
    if from_binary != from_source:
        problems.append(
            f"scripts/check-site.py: the flags read from {where} ({sorted(from_binary)}) and the "
            f"flags read from {MAIN_RS.relative_to(ROOT)} ({sorted(from_source)}) disagree. The "
            f"static reader has drifted from the parser it models; fix `flags_from_source`"
        )
    return from_binary, where, problems


def banner_from_binary(binary: pathlib.Path) -> str | None:
    """What `sipx-clstr --version` actually prints, or `None` if it would not say."""
    try:
        done = subprocess.run(
            [str(binary), "--version"], capture_output=True, text=True, timeout=30
        )
    except (OSError, subprocess.SubprocessError):
        return None
    if done.returncode != 0:
        return None
    return done.stdout.strip() or None


def banner_from_manifest() -> str | None:
    """The banner composed from `Cargo.toml`, or `None` if either number could not be read.

    `None` rather than a partial banner on purpose: half a banner compares unequal against every
    documented line, and would report the site as wrong when it is this reader that failed.
    """
    try:
        text = MANIFEST.read_text(encoding="utf-8")
    except OSError:
        return None

    section = ""
    version: str | None = None
    kernel: str | None = None
    for raw in text.splitlines():
        line = raw.strip()
        if header := MANIFEST_SECTION.match(line):
            section = header.group(1)
            continue
        if version is None and section == "workspace.package":
            if match := MANIFEST_VERSION.match(line):
                version = match.group(1)
        if kernel is None:
            if match := MANIFEST_KERNEL_PIN.match(line):
                kernel = match.group(1)
    if version is None or kernel is None:
        return None
    # The format string lives in `crates/sipx-clstr-node/src/main.rs`; this is the one place it is
    # spelled twice, and the two readings are held to each other below whenever both are available.
    return f"sipx-clstr {version} (sipx kernel {kernel})"


def version_banner() -> tuple[str | None, str, list[str]]:
    """The banner to hold the documentation to, where it came from, and any disagreement.

    Same shape and same discipline as `cli_surface`: the binary when there is one, the static
    reading otherwise, and a failure when both exist and disagree — which here means either the
    built binary predates a version bump, or `KERNEL_VERSION` has drifted from the pin.
    """
    from_manifest = banner_from_manifest()
    binary = binary_path()
    from_binary = banner_from_binary(binary) if binary is not None else None

    if from_binary is not None and binary is not None:
        where = f"{binary.relative_to(ROOT) if binary.is_relative_to(ROOT) else binary} --version"
        problems = []
        if from_manifest is None:
            problems.append(
                f"scripts/check-site.py: {where} prints `{from_binary}`, and no banner could be "
                f"composed from {MANIFEST.relative_to(ROOT)} — the `[workspace.package]` version "
                f"or the sipx `tag` pin has moved out of reach of `banner_from_manifest`. The docs "
                f"workflow has only that reading, so it is checking nothing until this is fixed"
            )
        elif from_manifest != from_binary:
            problems.append(
                f"scripts/check-site.py: {where} prints `{from_binary}`, and "
                f"{MANIFEST.relative_to(ROOT)} reads as `{from_manifest}`. Either the built binary "
                f"predates a version bump — rebuild it — or `KERNEL_VERSION` has drifted from the "
                f"pin, which `crates/sipx-clstr-node/tests/kernel_pin.rs` also holds"
            )
        return from_binary, where, problems

    if from_manifest is None:
        return (
            None,
            f"nowhere — no built binary here, and {MANIFEST.relative_to(ROOT)} yielded neither a "
            f"workspace version nor a kernel pin",
            [],
        )
    return (
        from_manifest,
        f"{MANIFEST.relative_to(ROOT)} (no built binary here — the gate checks against one)",
        [],
    )


def fenced_blocks_of(text: str) -> list[tuple[int, str, str]]:
    """`(line number of the opening fence, info string, body)` for every fenced block in `text`."""
    return [
        (text[: match.start()].count("\n") + 1, match.group(2).strip(), match.group(3))
        for match in FENCE.finditer(text)
    ]


def fenced_blocks(path: pathlib.Path) -> list[tuple[int, str, str]]:
    """`(line number, info string, body)` for every fenced block in a file."""
    return fenced_blocks_of(path.read_text(encoding="utf-8"))


def command_lines(body: str) -> list[tuple[int, str]]:
    """A fenced block's logical command lines as `(offset within the block, joined line)`.

    `\\`-continuations are joined, and the offset is the one where the command *starts* — carried
    through rather than recomputed, because blank and comment lines are dropped and a position
    counted after the drop points at the wrong line in every block that has one.
    """
    lines: list[tuple[int, str]] = []
    pending = ""
    started = 0
    for offset, line in enumerate(body.splitlines()):
        if not pending:
            started = offset
        if line.rstrip().endswith("\\"):
            pending += line.rstrip()[:-1] + " "
            continue
        pending += line
        if pending.strip() and not pending.lstrip().startswith("#"):
            lines.append((started, pending))
        pending = ""
    if pending.strip() and not pending.lstrip().startswith("#"):
        lines.append((started, pending))
    return lines


def invocation(line: str) -> list[str] | None:
    """The argv of the `sipx-clstr` invocation on this line, or `None` if there is not one.

    Two ways to be an invocation, because the documentation shows both:

    - the binary in **command position** — first token, after environment prefixes
      (`RUST_LOG=debug ...`) and pass-through wrappers (`setsid`, `timeout`) are stripped;
    - the binary followed by a **subcommand**, which is the `docker run ... sipx-clstr run
      --config ...` form, where the same token is the image name and the command at once.

    Command position alone is not enough (it would miss the container form), and the token alone is
    far too much: `cargo build --bin sipx-clstr --features postgres` names the binary and then
    passes `--features` to *cargo*, and reading that as ours reports a defect that is not there.
    """
    # One pipeline segment at a time: `sipx-clstr run ... | while read ...` is an invocation and a
    # loop, and only the first half is argv.
    tokens = line.split("|")[0].split()
    while tokens and (ENV_PREFIX.match(tokens[0]) or tokens[0] in WRAPPERS):
        tokens = tokens[1:]
    for index, token in enumerate(tokens):
        if not BINARY_TOKEN.match(token):
            continue
        rest = tokens[index + 1 :]
        if index == 0 or (rest and rest[0] in ("run", "help")):
            return rest
    return None


def check_cli_page(surface: set[str], where: str) -> list[str]:
    """The reference page and the binary name the same flags — in both directions.

    Only three places on the page are read as naming a flag: a table's first cell, an `Options:`
    entry inside a fenced block, and an actual invocation. Prose is left alone, because the page
    legitimately mentions flags that are not the binary's — `--features postgres` is `cargo`'s, and
    scraping every `--word` on the page would report it as an undocumented flag forever.
    """
    if not CLI_PAGE.is_file():
        return [f"{CLI_PAGE.relative_to(ROOT)} is missing — the CLI surface is unverified"]

    named: set[str] = set()
    for line in CLI_PAGE.read_text(encoding="utf-8").splitlines():
        match = TABLE_FLAG.match(line)
        if match:
            named.add(match.group(1))
    for _, _, body in fenced_blocks(CLI_PAGE):
        for line in body.splitlines():
            match = HELP_FLAG.match(line)
            if match:
                named.add(match.group(1))
        for _, line in command_lines(body):
            argv = invocation(line)
            if argv is not None:
                named |= {match.group(1) for match in FLAG.finditer(" ".join(argv))}

    page = CLI_PAGE.relative_to(ROOT)
    problems = []
    for flag in sorted(named - surface):
        problems.append(
            f"{page}: documents `{flag}`, which {where} does not accept. A reader who types the "
            f"page gets exit 2"
        )
    for flag in sorted(surface - named):
        problems.append(
            f"{page}: {where} accepts `{flag}` and this page never names it. A flag the reference "
            f"omits is a flag nobody finds — name it or remove it from the binary"
        )
    return problems


def check_documented_commands(surface: set[str], where: str) -> tuple[list[str], int, list[str]]:
    """Every documented command: ours are held to the flag set, the rest are named and counted."""
    problems: list[str] = []
    verified = 0
    unrunnable: list[str] = []

    for path in tracked(*COMMAND_SOURCES):
        if path.suffix != ".md":
            continue
        rel = path.relative_to(ROOT).as_posix()
        for start, info, body in fenced_blocks(path):
            if (info.split()[0] if info.split() else "") not in COMMAND_FENCES:
                continue
            for offset, line in command_lines(body):
                number = start + 1 + offset
                argv = invocation(line)
                if argv is not None:
                    verified += 1
                    for match in FLAG.finditer(" ".join(argv)):
                        if match.group(1) not in surface:
                            problems.append(
                                f"{rel}:{number}: `{match.group(1)}` is not a flag {where} "
                                f"accepts — `{line.strip()}` exits 2 for anyone who runs it"
                            )
                    if argv and argv[0] == "run" and "--config" not in argv:
                        problems.append(
                            f"{rel}:{number}: `run` without `--config`, which is required — "
                            f"`{line.strip()}` exits 2"
                        )
                else:
                    tool = line.split()[0] if line.split() else ""
                    tool = tool.rsplit("/", 1)[-1]
                    if tool in EXTERNAL_TOOLS:
                        unrunnable.append(f"{rel}:{number}: {line.strip()}")

                for match in REPO_PATH.finditer(line):
                    named = match.group(1)
                    if not (ROOT / named).exists():
                        problems.append(
                            f"{rel}:{number}: names `{named}`, which is not in this repository — "
                            f"`{line.strip()}` cannot run as written"
                        )
    return problems, verified, unrunnable


def documented_banners(text: str) -> list[tuple[int, str]]:
    """`(line number, banner)` for every version banner inside a fenced block of `text`.

    **Every** fence, whatever its info string. The banner is output rather than a command, so it
    sits in the `text` blocks `check_documented_commands` deliberately leaves alone — which is why
    no check had ever read one. Outside a fence it is prose and belongs to `check-docs.py`.
    """
    found: list[tuple[int, str]] = []
    for start, _, body in fenced_blocks_of(text):
        for offset, line in enumerate(body.splitlines()):
            if match := VERSION_BANNER.match(line):
                found.append((start + 1 + offset, match.group(1)))
    return found


def check_documented_versions(expected: str | None, where: str) -> tuple[list[str], int]:
    """Every version the documentation prints is the version the binary prints."""
    problems: list[str] = []
    found = 0

    for path in tracked(*COMMAND_SOURCES):
        if path.suffix != ".md":
            continue
        rel = path.relative_to(ROOT).as_posix()
        for number, banner in documented_banners(path.read_text(encoding="utf-8")):
            found += 1
            if expected is None or banner == expected:
                continue
            problems.append(
                f"{rel}:{number}: documents `{banner}`, but the banner is `{expected}` (from "
                f"{where}). The site deploys on release and the version only goes stale at a cut, "
                f"so the first reader of this line is a public one"
            )

    if expected is None and found:
        problems.append(
            f"scripts/check-site.py: {found} documented version banner(s) went unchecked — the "
            f"banner to hold them to could be read from {where}. Refusing to exit 0 on a check "
            f"that read nothing"
        )
    return problems, found


def check_proofs_are_gated() -> list[str]:
    """A proof the documentation offers either runs in CI or records why it does not.

    "Offers" is meant literally: the set is the scripts a documented command tells a reader to run.
    A script nothing points at is not evidence for anything and is not held to this.
    """
    offered: dict[str, str] = {}
    for path in tracked(*COMMAND_SOURCES):
        if path.suffix != ".md":
            continue
        rel = path.relative_to(ROOT).as_posix()
        for _, info, body in fenced_blocks(path):
            if not info.split() or info.split()[0] not in COMMAND_FENCES:
                continue
            for _, line in command_lines(body):
                for match in re.finditer(r"(?<![\w/.-])(scripts/[\w.-]+\.(?:sh|py))", line):
                    offered.setdefault(match.group(1), rel)

    referenced: set[str] = set()
    for source in CI_REFERENCES:
        target = ROOT / source
        files = sorted(target.rglob("*")) if target.is_dir() else [target]
        for file in files:
            if not file.is_file():
                continue
            try:
                text = file.read_text(encoding="utf-8")
            except (UnicodeDecodeError, OSError):
                continue
            yaml = file.suffix in (".yml", ".yaml")
            referenced |= {
                name for name in offered if runs_in_ci(text, name, yaml=yaml)
            }

    problems = []
    for name, cited_by in sorted(offered.items()):
        if name in referenced:
            continue
        path = ROOT / name
        if not path.is_file():
            continue  # Already reported as a missing path by the command check.
        if PROOF_DIRECTIVE.search(path.read_text(encoding="utf-8")):
            continue
        problems.append(
            f"{name}: {cited_by} offers this as proof, and nothing in scripts/gate.sh or "
            f".github/workflows/ runs it. Either wire it in, or record the decision in the script "
            f"itself as a `not-in-ci: <reason>` comment — an unverified proof and a deliberately "
            f"unverified proof look identical from here, and the first one shipped a 403"
        )
    return problems


# ------------------------------------------------- `DX-14`: release claims against the evidence ---
#
# One question, three sources of truth, none of them retyped in this file: the generated
# conformance report, the vector checker's own spec registry, and the board's `status` field. A
# release claim that cannot be traced to one of those is a sentence somebody wrote once.

CHECKER = ROOT / "scripts" / "check-vectors.py"
CONFORMANCE = ROOT / "docs" / "reference" / "conformance.md"
STORIES = ROOT / "docs" / "stories"

# Where a page stops describing the present. A release note is a **dated record** — `0.12.0` really
# did ship `157 of 586`, and rewriting that line whenever the suite moves would turn the release
# history into a second copy of today's numbers instead of a record of what shipped. So the scan
# stops at the named heading, and `main` prints how many lines it dropped for this reason: a
# checker that narrows what it looks at has to say so in its output, which is the rule the rest of
# this file was written to.
HISTORICAL = {"website/docs/whats-new.md": "## Releases"}

# The report's headline sentence, exactly as `check-vectors.py`'s `render` writes it. Read rather
# than recomputed: the gate regenerates and re-checks that file in the step before this one, so the
# committed report *is* the current measurement, and reading it keeps one generator rather than two.
HEADLINE = re.compile(
    r"\*\*(\d+) of (\d+) rows proved\*\*; (\d+) covered for shape only; (\d+) deferred\."
)
# One row of one of the report's own tables: `| `PB-F-1` | proved | … |`. Parsed so a claim about a
# single specification has something to be compared against — see `SCOPE_TOKEN`.
REPORT_ROW = re.compile(r"^\|\s*`([A-Z]{2})-(?:[A-Z]-)?\d+`\s*\|\s*([a-z ]+?)\s*\|", re.M)

# How a conformance count is written on a public page. Four shapes because the triple appears four
# ways: as the badge's URL, as the badge's alt text, and twice in prose. The badge is the one a
# reader sees without opening anything, and it is the copy that was wrong for two releases.
BADGE_COUNTS = re.compile(r"vectors-(\d+)%2F(\d+)%20proved")
PROVED_COUNTS = re.compile(
    r"\b(\d+)\s+of\s+(?:its\s+)?(\d+)\s+(?:`[A-Z]{2}`\s+)?(?:vector\s+)?rows\s+proved"
)
SHAPE_COUNT = re.compile(r"\b(\d+)\s+covered for shape only")
DEFERRED_COUNT = re.compile(r"\b(\d+)\s+deferred\b")

# A count claim may be about one specification rather than the whole ledger — `intro.md` quotes
# registrar-auth's own rows beside the defect they explain. A scoped claim **names its prefix in
# backticks on the same line** and is compared against that prefix's rows; an unscoped one is
# compared against the headline. Naming the prefix is the price of quoting a subset, and it is
# cheaper than the alternative, which is a published number nothing can check.
SCOPE_TOKEN = re.compile(r"`([A-Z]{2})`")

# A count in front of the word *specifications*. Only a number is read as a claim, so "normative
# specifications" is prose and "Fifteen specifications" is a measurement — of the documents the
# checker registers, which is not the same as the file count under `docs/specs/`: two of them
# deliberately register no prefix. A sentence about a *past* state must not spell its number this
# way; the error message says so, because the cheap fix for a false positive is rewording and the
# cheap fix for a real one is the current number, and both should be visible from the failure.
SPEC_COUNT = re.compile(r"\b([A-Za-z]+|\d+)\s+specifications?\b")
NUMBER_WORDS = {
    "one": 1, "two": 2, "three": 3, "four": 4, "five": 5, "six": 6, "seven": 7, "eight": 8,
    "nine": 9, "ten": 10, "eleven": 11, "twelve": 12, "thirteen": 13, "fourteen": 14,
    "fifteen": 15, "sixteen": 16, "seventeen": 17, "eighteen": 18, "nineteen": 19, "twenty": 20,
}
SPELLED = {value: word for word, value in NUMBER_WORDS.items()}

# An *inventory* of the registered prefixes, as opposed to a sentence that happens to cite one.
# Three or more bare backticked prefixes in one paragraph is where the corpus separates: a page
# citing a row writes `` `PB-F-1` `` or `` `PB-*` ``, and only the inventory sentence lists the
# codes themselves. An inventory that names three must name all of them — a partial list reads as
# complete, which is exactly how "all ten specifications" survived five registrations.
INVENTORY_FLOOR = 3

# A table row whose status cell is exactly `today`, in the site's closed vocabulary (`today` ·
# `today, partly` · `specified, not shipped` · `designed` · `not planned`). Anchored to the end of
# the line so the qualified form is *not* matched: `today, partly` is the honest answer for a
# capability the driver performs in part, and a check that refused it would push pages towards the
# unqualified word it exists to prevent.
TODAY_CELL = re.compile(r"\|\s*\*{0,2}today\*{0,2}\s*\|?\s*$")
# `README.md`'s status table keys its rows by state rather than by capability, so its **Working**
# row is a `today` cell written the other way round.
WORKING_ROW = re.compile(r"^\s*\|\s*\*{0,2}Working\*{0,2}\s*\|")

# The clause a claim sits in. Table cells and sentences are the units these things are written in,
# and the unit matters because of `REFUSAL` below: judged over a whole line, one honest "not" in a
# neighbouring cell would excuse an overstatement three cells away.
SEGMENT = re.compile(r"[.;|]")
# A clause that **denies** the capability is the clause this check wants pages to write, so a
# pattern below is not allowed to fire on one — in either direction.
#
# **Negation scopes forward**, and that is the whole rule: a refusal word denies what comes after
# it, so only one standing at or before the end of a match can deny that match. Everything after it
# is the next proposition and denies nothing behind it. The patterns below run subject → verb →
# object, so a negation that really applies lands *inside* the match ("roles do **not** gate
# dispatch", "a Timer C … and **never** fires"), which is how the site's honest sentences are
# actually written; a refusal in the tail belongs to a different claim ("roles gate runtime
# dispatch, so **no** method reaches another role's handler" is the overstatement, not a denial of
# it). Reading the whole clause instead — the first version of this — let any trailing "rather
# than …" excuse the claim in front of it, which is precisely the shape `86e6b10` shipped, so the
# check was inert against the one sentence it was written for.
#
# Said plainly rather than implied: this is where the prose half of the rule is approximate. It
# errs towards silence on a comma-joined sentence whose *first* clause refuses ("the driver does
# not perform this, and a Timer C fires on schedule" is read as denied). The structural `today`-cell
# rule carries the weight for table rows and cannot be talked out of a verdict either way.
REFUSAL = re.compile(
    r"\b(?:not|never|no|cannot|nothing|until|yet)\b|\brather than\b|\binstead of\b", re.I
)

# The four claims a release-facing page may not make on its own authority (`V-18`), each mapped to
# the story that owns its real-driver proof. The mapping is enforced in both directions off the
# board's `status` field, because both directions have already failed here: `86e6b10` shipped
# *declared roles gate runtime dispatch* while the driver dropped the role set, and this tree
# shipped *`ACK` … resolved by address-of-record lookup* for a whole release after `PX-13` made
# that false.
#
# Closing a vector row moves none of these. A row proves what the pure engine *emits*; every gap
# here is a driver that does not perform the effect the engine produced, so the promotion is tied
# to a story's real-socket acceptance and to nothing that `check-vectors.py` can turn green.
#
#   `subject`  — the capability, as a page names it. Read only inside a `today` cell, so it can be
#                broad without turning every mention of `CANCEL` into a defect.
#   `claims`   — phrasings refused while the story is open, whatever row they sit in.
#   `denials`  — phrasings refused once the story is `done`; the stale-deferral rule, applied to
#                prose. Empty is allowed and means only the promotion direction is held.
#   `instead`  — what is true today, named with the file that decides it, so a failure is
#                actionable without opening the driver.
GOVERNED: dict[str, dict] = {
    "matched CANCEL and Timer C": {
        "story": "PX-12",
        "subject": re.compile(r"\bTimer\s+C\b|\bCANCEL\b"),
        "claims": (
            re.compile(r"\bTimer\s+C\b[^.|\n]{0,60}\b(?:fires|expires|is performed)\b", re.I),
        ),
        "denials": (),
        "instead": "the engine produces `CancelBranch` and `SetTimer`; the driver logs the first "
        "and drops the second (crates/sipx-clstr-node/src/driver.rs)",
    },
    "role dispatch": {
        "story": "DP-13",
        "subject": re.compile(r"\brole\b|\broles\b", re.I),
        "claims": (
            re.compile(r"\broles?\b[^.|\n]{0,40}\b(?:gate|drive|select)s?\b[^.|\n]{0,30}"
                       r"\b(?:runtime )?dispatch\b", re.I),
        ),
        "denials": (),
        "instead": "the released node derives a capability set from the declared roles and answers "
        "`405` for a method they do not wire (driver.rs `Dispatch::of`), while the refusal shape, "
        "the counted ACK and the echo runtime are still `DP-13`'s",
    },
    "outbound transport resolution": {
        "story": "RT-12",
        "subject": re.compile(r"outbound (?:target|transport)|target selection|RFC\s*3263|NAPTR"),
        "claims": (),
        "denials": (),
        "instead": "`destination_of` returns a UDP target and refuses a hostname outright "
        "(crates/sipx-clstr-node/src/driver.rs), so there is no transport selection to claim",
    },
    "in-dialog routing on the node": {
        "story": "PX-13",
        "subject": re.compile(r"in-dialog|\bACK\b"),
        "claims": (),
        "denials": (
            re.compile(r"\bACK\b[^.|\n]{0,80}\baddress[- ]of[- ]record lookup\b", re.I),
            re.compile(r"in-dialog[^.|\n]{0,80}\baddress[- ]of[- ]record lookup\b", re.I),
        ),
        "instead": "`PX-13` landed: `forward_ack` follows the engine's `Route` preprocessing and "
        "in-dialog requests use the core's next hop (crates/sipx-clstr-node/src/driver.rs)",
    },
    "the probe's dialog": {
        "story": "ET-7",
        "subject": re.compile(r"\bprobe\b[^.|\n]{0,40}\bdialog\b|\bdialog\b[^.|\n]{0,40}\bprobe\b"),
        "claims": (),
        "denials": (),
        "instead": "the probe still builds AoR-shaped `ACK` and `BYE`, so a passing run proves a "
        "second lookup rather than a dialog route",
    },
}

# `CF-3`'s wording, held apart from everybody else's. The end-to-end proofs put the **same kernel**
# on both ends — the `sipx` CLI is built from the checkout this repository pins — so what they
# prove is that two processes speaking through real sockets agree, which is worth having and is not
# what "independent implementation" means. The independent interop target is `CF-3`'s, unbuilt, and
# borrowing its credibility early is the cheapest way for a release to overstate itself.
INDEPENDENT = re.compile(
    r"\bindependent(?:ly)?\s+(implementation|parser|stack|client software|client|sipx|"
    r"implementations)\b",
    re.I,
)
INTEROP_STORY = "CF-3"
REQUIRED_WORDING = "same-kernel, separate-process integration test"


def vector_checker():
    """`scripts/check-vectors.py`, imported for its registries rather than copied into this file.

    The spec set has exactly one definition (`CF-25`) and this check exists because second copies
    of measurements go stale; a second copy of the registry *inside the checker for second copies*
    would be the same defect one level up. Imported by path because the file name is not an
    identifier, and importing runs only its module-level definitions — its `main` is behind the
    usual guard.
    """
    spec = importlib.util.spec_from_file_location("check_vectors", CHECKER)
    if spec is None or spec.loader is None:
        return None
    module = importlib.util.module_from_spec(spec)
    # No `scripts/__pycache__/` for it: a checker that leaves untracked files behind makes
    # `git status` dirty on a machine that only ran the gate, and this one runs on every push.
    written, sys.dont_write_bytecode = sys.dont_write_bytecode, True
    try:
        spec.loader.exec_module(module)
    finally:
        sys.dont_write_bytecode = written
    return module


def conformance() -> tuple[dict[str, dict[str, int]], list[str]]:
    """The generated report's counts: the headline under `""`, and the same four per prefix.

    The per-prefix tally is summed and compared against the headline before anything is judged
    against either. They are two traversals of one document — the same shape `CF-17` found
    disagreeing inside `check-vectors.py` itself — and a public copy compared against a misreading
    would fail for a reason nobody could act on.
    """
    if not CONFORMANCE.is_file():
        return {}, [
            f"{CONFORMANCE.relative_to(ROOT)} does not exist, so no published conformance count "
            f"could be checked. Run scripts/check-vectors.py"
        ]
    text = CONFORMANCE.read_text(encoding="utf-8")
    headline = HEADLINE.search(text)
    if not headline:
        return {}, [
            f"{CONFORMANCE.relative_to(ROOT)}: the generated headline could not be read. This "
            f"check reads the report rather than recomputing it, so it refuses to pass every "
            f"published number on a parse that failed"
        ]

    proved, total, shape, waived = (int(value) for value in headline.groups())
    counts: dict[str, dict[str, int]] = {
        "": {"proved": proved, "total": total, "shape only": shape, "deferred": waived}
    }
    for prefix, state in REPORT_ROW.findall(text):
        family = counts.setdefault(
            prefix, {"proved": 0, "total": 0, "shape only": 0, "deferred": 0}
        )
        family["total"] += 1
        if state in family:
            family[state] += 1

    problems = []
    for field in ("proved", "total", "shape only", "deferred"):
        tallied = sum(counts[prefix][field] for prefix in counts if prefix)
        if tallied != counts[""][field]:
            problems.append(
                f"{CONFORMANCE.relative_to(ROOT)}: its headline says {counts[''][field]} "
                f"{field} and its tables show {tallied}. This check reads both and refuses to "
                f"hold the site to a reading of the report that does not add up"
            )
    return counts, problems


def count_problems(line: str, counts: dict[str, dict[str, int]]) -> list[str]:
    """Every conformance count this line states that the generated report contradicts.

    Returned without a file or a line number so `self_test` can feed it synthetic text; the caller
    adds those.
    """
    scope = ""
    for candidate in SCOPE_TOKEN.findall(line):
        if candidate in counts:
            scope = candidate
            break
    if scope not in counts:
        return []
    truth = counts[scope]
    named = f"`{scope}`" if scope else "the whole ledger"

    def mismatch(what: str, stated: int) -> list[str]:
        if stated == truth[what]:
            return []
        return [
            f"claims {stated} {what} for {named}; the generated report says {truth[what]} "
            f"(expected {truth[what]}, actual {stated})"
        ]

    problems: list[str] = []
    for match in BADGE_COUNTS.finditer(line):
        problems += mismatch("proved", int(match.group(1)))
        problems += mismatch("total", int(match.group(2)))
    for match in PROVED_COUNTS.finditer(line):
        problems += mismatch("proved", int(match.group(1)))
        problems += mismatch("total", int(match.group(2)))
    for match in SHAPE_COUNT.finditer(line):
        problems += mismatch("shape only", int(match.group(1)))
    for match in DEFERRED_COUNT.finditer(line):
        problems += mismatch("deferred", int(match.group(1)))
    return problems


def spec_registry(module) -> tuple[int, set[str], list[str]]:
    """How many specifications the checker registers, which prefixes they use, and the excluded.

    The count is of **documents**, not prefixes and not files: two specs carry two vector tables
    each, and two more register no prefix at all by a decision `EXCLUDED` records. A public
    sentence saying "N specifications" is claiming the first of those numbers.
    """
    registered = {path for path, _section, _author in module.SPECS.values()}
    excluded = [path for path in module.EXCLUDED if path.startswith("docs/specs/")]
    return len(registered), set(module.SPECS), excluded


def spec_count_problems(line: str, registered: int) -> list[str]:
    """A published count of specifications that the registry contradicts."""
    problems = []
    for match in SPEC_COUNT.finditer(line):
        word = match.group(1)
        stated = NUMBER_WORDS.get(word.lower()) if not word.isdigit() else int(word)
        if stated is None or stated == registered:
            continue
        spelled = SPELLED.get(registered, str(registered))
        problems.append(
            f"says \"{match.group(0)}\"; scripts/check-vectors.py registers {registered} "
            f"(expected {spelled}, actual {word.lower()}). If the sentence is about a past "
            f"state, say it without a bare count in front of the word — a number there is read "
            f"as a claim about the registry"
        )
    return problems


def inventory_problems(paragraph: str, prefixes: set[str]) -> list[str]:
    """An enumeration of registered prefixes that has stopped enumerating all of them."""
    named = {code for code in SCOPE_TOKEN.findall(paragraph) if code in prefixes}
    if len(named) < INVENTORY_FLOOR:
        return []
    if missing := sorted(prefixes - named):
        return [
            f"lists {len(named)} of the {len(prefixes)} registered vector prefixes and reads as "
            f"complete; {', '.join(f'`{code}`' for code in missing)} "
            f"{'is' if len(missing) == 1 else 'are'} registered in scripts/check-vectors.py and "
            f"named nowhere in it (expected all {len(prefixes)}, actual {len(named)})"
        ]
    return []


def story_statuses() -> dict[str, str]:
    """Every story's `status`, read from its own frontmatter the way `check-vectors.py` reads it."""
    statuses: dict[str, str] = {}
    if not STORIES.is_dir():
        return statuses
    for path in sorted(STORIES.glob("*.md")):
        identifier = status = ""
        for line in path.read_text(encoding="utf-8").splitlines()[:12]:
            if line.startswith("id:"):
                identifier = line.split(":", 1)[1].strip()
            elif line.startswith("status:"):
                status = line.split(":", 1)[1].strip()
            elif line.strip() == "---" and identifier:
                break
        if identifier:
            statuses[identifier] = status
    return statuses


def segment_of(line: str, start: int, end: int) -> tuple[int, int]:
    """The bounds of the clause a match sits in — one table cell, or one sentence."""
    left = max((found.end() for found in SEGMENT.finditer(line[:start])), default=0)
    right = min(
        (end + found.start() for found in SEGMENT.finditer(line[end:])), default=len(line)
    )
    return left, right


def stated(line: str, pattern: re.Pattern[str]) -> re.Match[str] | None:
    """A match of `pattern` that its own clause does not deny.

    The clause is the unit — a "not" three cells away excuses nothing — and within it only the run
    from the clause's start to the **end of the match** can carry the denial, because that is how
    far a refusal word reaches (see `REFUSAL`). Searched over the line with bounds rather than over
    a slice, so `\\b` still sees the neighbouring characters.
    """
    for match in pattern.finditer(line):
        left, _ = segment_of(line, match.start(), match.end())
        if not REFUSAL.search(line, left, match.end()):
            return match
    return None


def unknown_owners(statuses: dict[str, str]) -> list[str]:
    """A gated claim whose owner is not on the board: reported once, and it fails the run."""
    return [
        f"scripts/check-site.py: {name} is gated on `{rule['story']}`, and no story carries that "
        f"id. The promotion rule reads the board's `status`, so an owner that is not on it is a "
        f"gate that can never fire"
        for name, rule in GOVERNED.items()
        if rule["story"] not in statuses
    ]


def capability_problems(line: str, statuses: dict[str, str]) -> list[str]:
    """Release claims on this line that the owning story does not yet support, or has outlived.

    A rule whose owner is not on the board is skipped here and reported once by `unknown_owners`,
    which is what keeps that failure from being repeated on every line of the site.
    """
    problems: list[str] = []
    today = TODAY_CELL.search(line)
    promoted = today or WORKING_ROW.match(line)
    where = "a `today` row" if today else "the **Working** row"
    for name, rule in GOVERNED.items():
        story = rule["story"]
        status = statuses.get(story)
        if status is None:
            continue
        if status != "done":
            if promoted and rule["subject"].search(line):
                problems.append(
                    f"{where} claims {name}, whose real-driver proof is `{story}` ({status}). "
                    f"Today {rule['instead']}. Say it in the qualified form the site's vocabulary "
                    f"already has (`today, partly`), or wait for `{story}`"
                )
            for pattern in rule["claims"]:
                if match := stated(line, pattern):
                    problems.append(
                        f"claims \"{match.group(0)}\" — {name} is `{story}`'s ({status}), and "
                        f"today {rule['instead']}"
                    )
        else:
            for pattern in rule["denials"]:
                if match := stated(line, pattern):
                    problems.append(
                        f"still says \"{match.group(0)}\" — `{story}` is done, and {rule['instead']}"
                    )
    return problems


def governed_pages() -> list[tuple[str, list[str]]]:
    """The public tree as lines, with each page's dated-record region dropped.

    `README.md` is in here for the reason it is in `COMMAND_SOURCES`: it is the release-facing page
    for a reader who never opens the site, and it carried the badge that was wrong.
    """
    pages: list[tuple[str, list[str]]] = []
    for path in tracked(*COMMAND_SOURCES):
        if path.suffix != ".md":
            continue
        rel = path.relative_to(ROOT).as_posix()
        lines = path.read_text(encoding="utf-8").splitlines()
        pages.append((rel, lines))
    return pages


def present_tense(rel: str, lines: list[str]) -> list[tuple[int, str]]:
    """A page's numbered lines, up to the heading at which it becomes a dated record."""
    stop = len(lines)
    if heading := HISTORICAL.get(rel):
        for index, line in enumerate(lines):
            if line.strip() == heading:
                stop = index
                break
    return [(number, line) for number, line in enumerate(lines[:stop], start=1)]


def check_release_claims() -> tuple[list[str], dict[str, int]]:
    """Items 6, 7 and 8: counts, inventory and capability language, against their three sources."""
    problems: list[str] = []
    counted = {"pages": 0, "lines": 0, "historical": 0}

    counts, trouble = conformance()
    problems += trouble
    module = vector_checker()
    if module is None:
        problems.append(
            f"{CHECKER.relative_to(ROOT)} could not be imported, so the published specification "
            f"inventory was compared against nothing. Refusing to exit 0 on that"
        )
        registered, prefixes = 0, set()
    else:
        registered, prefixes, _excluded = spec_registry(module)
    statuses = story_statuses()
    problems += unknown_owners(statuses)

    for rel, lines in governed_pages():
        counted["pages"] += 1
        governed = present_tense(rel, lines)
        counted["lines"] += len(governed)
        counted["historical"] += len(lines) - len(governed)
        for number, line in governed:
            for problem in count_problems(line, counts):
                problems.append(f"{rel}:{number}: {problem}")
            if module is not None:
                for problem in spec_count_problems(line, registered):
                    problems.append(f"{rel}:{number}: {problem}")
            for problem in capability_problems(line, statuses):
                problems.append(f"{rel}:{number}: {problem}")

        # Paragraph-grained, because an inventory is a sentence that may wrap. The line number
        # reported is the paragraph's first, which is where a reader starts editing it.
        if module is not None and prefixes:
            start, buffer = 1, []
            for number, line in governed + [(len(governed) + 1, "")]:
                if line.strip():
                    if not buffer:
                        start = number
                    buffer.append(line)
                    continue
                if buffer:
                    for problem in inventory_problems(" ".join(buffer), prefixes):
                        problems.append(f"{rel}:{start}: {problem}")
                buffer = []

    # The positive half of `CF-3`'s wording rule. A prohibition alone is satisfied by saying
    # nothing at all about what the end-to-end proof is worth, which is how the overstatement got
    # in: the reader was told the other end was independent because nobody had written down what it
    # actually is.
    if statuses.get(INTEROP_STORY) != "done":
        readme = ROOT / "README.md"
        if readme.is_file() and REQUIRED_WORDING not in readme.read_text(encoding="utf-8"):
            problems.append(
                f"README.md: the real-socket proof is offered as evidence and never characterised. "
                f"Describe it as a {REQUIRED_WORDING} — the `sipx` CLI on the other end is built "
                f"from the kernel checkout this repository pins, so the two ends share a parser"
            )

    return problems, counted


def check_proof_wording() -> list[str]:
    """"Independent" is `CF-3`'s word, and `CF-3` is not built."""
    if story_statuses().get(INTEROP_STORY) == "done":
        return []
    problems = []
    for rel, lines in governed_pages():
        for number, line in present_tense(rel, lines):
            if match := INDEPENDENT.search(line):
                problems.append(
                    f"{rel}:{number}: \"{match.group(0)}\" — the end-to-end proofs run the `sipx` "
                    f"CLI, built from the kernel checkout this repository pins, so both ends share "
                    f"one parser. Call it a {REQUIRED_WORDING}; \"independent\" belongs to "
                    f"`{INTEROP_STORY}`'s interop target, which does not exist yet"
                )
    return problems


def self_test() -> list[str]:
    """The ways "it runs in CI" can be made to look true without being true.

    `CF-15`. The substring test this replaces was demonstrated failing open by adding one line —
    `# scripts/e2e-call.sh` — to `scripts/gate.sh`: the checker went from FAIL to clean on a proof
    that nothing ran. Run on every invocation rather than as a separate suite, on `check-vectors.py`'s
    reasoning: a checker whose own defect is a false green has to carry the proof it is still closed.
    """
    failures: list[str] = []
    proof = "scripts/e2e-call.sh"

    def check(claim: str, held: bool) -> None:
        if not held:
            failures.append(claim)

    # The line that flipped the verdict, and its neighbours.
    check(
        "a commented-out invocation is not an invocation — the CF-15 case",
        not runs_in_ci('step "site"\n# scripts/e2e-call.sh\n', proof, yaml=False),
    )
    check(
        "nor is one commented without a space",
        not runs_in_ci("#scripts/e2e-call.sh\n", proof, yaml=False),
    )
    check(
        "nor a trailing comment on a line that runs something else",
        not runs_in_ci(
            "scripts/check-site.py  # unlike scripts/e2e-call.sh, this one runs\n",
            proof,
            yaml=False,
        ),
    )
    check(
        "nor prose that says the proof is deliberately not run",
        not runs_in_ci("# The proofs (scripts/e2e-call.sh) are run by hand.\n", proof, yaml=False),
    )
    check(
        "nor the name as an argument to another command",
        not runs_in_ci('echo "run scripts/e2e-call.sh before a release"\n', proof, yaml=False),
    )

    # And the forms that genuinely are an invocation, so this does not fail closed on real wiring.
    for shell in (
        "scripts/e2e-call.sh\n",
        "scripts/e2e-call.sh --port 5060\n",
        "./scripts/e2e-call.sh\n",
        "timeout 600 scripts/e2e-call.sh\n",
        "cargo build --bin sipx-clstr && scripts/e2e-call.sh\n",
        'SIPX="$PWD/sipx" scripts/e2e-call.sh\n',
    ):
        check(f"a real invocation counts: {shell.strip()}", runs_in_ci(shell, proof, yaml=False))

    # The same distinctions through the workflow reader.
    check(
        "a step *named* after a proof does not run it",
        not runs_in_ci(
            "      - name: runs scripts/e2e-call.sh\n        run: echo nothing\n", proof, yaml=True
        ),
    )
    check(
        "a commented-out YAML step does not run it",
        not runs_in_ci("      # - run: scripts/e2e-call.sh\n", proof, yaml=True),
    )
    check(
        "nor a commented line inside a block scalar",
        not runs_in_ci(
            "      - name: proof\n        run: |\n          # scripts/e2e-call.sh\n",
            proof,
            yaml=True,
        ),
    )
    check(
        "an inline `run:` counts",
        runs_in_ci("      - run: scripts/e2e-call.sh\n", proof, yaml=True),
    )
    check(
        "a block-scalar `run:` counts",
        runs_in_ci(
            "      - name: proof\n        run: |\n          scripts/e2e-call.sh --port 5060\n",
            proof,
            yaml=True,
        ),
    )
    check(
        "a `run:` body ends where the indentation does",
        not runs_in_ci(
            "      - name: proof\n        run: |\n          cargo test\n"
            "      - name: scripts/e2e-call.sh is not run here\n        run: echo nothing\n",
            proof,
            yaml=True,
        ),
    )

    # `CF-19`. The version check is a string comparison and carries no risk worth pinning; all of it
    # is in what `documented_banners` decides is a banner. Recognise too little and a stale line is
    # invisible — which is the defect — and recognise too much and every refusal message the site
    # quotes becomes a version defect.
    def banners(body: str) -> list[str]:
        return [banner for _, banner in documented_banners(f"```text\n{body}\n```\n")]

    current = "sipx-clstr 0.12.0 (sipx kernel 0.10.0)"
    stale = "sipx-clstr 0.11.0 (sipx kernel 0.10.0)"

    check("a banner in a fenced block is read", banners(current) == [current])
    check(
        "a stale one is still a banner — matching only current ones would see nothing",
        banners(stale) == [stale],
    )
    check(
        "so is one that has lost its kernel half",
        banners("sipx-clstr 0.12.0") == ["sipx-clstr 0.12.0"],
    )
    check(
        "an indented banner is read, without its indentation",
        banners(f"    {current}") == [current],
    )
    check(
        "a refusal message is not a banner",
        banners("sipx-clstr: cluster.yaml was refused — 2 problem(s):") == [],
    )
    check("nor is an invocation of the binary", banners("sipx-clstr --version") == [])
    check("nor a container image tag", banners("sipx-clstr:dev") == [])
    check(
        "nor the binary named anywhere but the start of the line",
        banners("docker build -t sipx-clstr 0.12.0") == [],
    )
    check(
        "a banner outside every fence is prose, which is check-docs.py's",
        documented_banners(f"{current}\n") == [],
    )
    check(
        "the reported line is the file's, not the block's",
        documented_banners(f"intro\n\n```text\n{current}\n```\n") == [(4, current)],
    )

    # `DX-14`. Both classes of release drift, replayed as they were found: a conformance triple
    # copied by hand and left behind by the generator, and a capability the site called `today`
    # while the driver discarded the effect. The detectors are pinned in both directions, because
    # each of them is one false green away from being decoration.
    truth = {
        "": {"proved": 169, "total": 598, "shape only": 19, "deferred": 410},
        "RA": {"proved": 23, "total": 29, "shape only": 5, "deferred": 1},
    }
    stale_badge = "vectors-157%2F586%20proved"
    check("a stale badge is caught", count_problems(stale_badge, truth))
    check(
        "and it names both numbers, not just the first",
        len(count_problems(stale_badge, truth)) == 2,
    )
    check(
        "the current badge passes",
        not count_problems("vectors-169%2F598%20proved", truth),
    )
    check(
        "a stale triple in prose is caught",
        len(count_problems("**157 of 586 vector rows proved**, 19 covered for shape only, "
                           "410 deferred", truth)) == 2,
    )
    check(
        "the current triple passes",
        not count_problems("**169 of 598 vector rows proved**, 19 covered for shape only, "
                           "410 deferred", truth),
    )
    check(
        "a one-row denominator change is enough to fail a copy",
        count_problems("169 of 598 rows proved", {"": dict(truth[""], total=599)}),
    )
    check(
        "a claim scoped to a prefix is compared against that prefix",
        not count_problems("23 of its 29 `RA` rows proved, 5 covered for shape only, "
                           "1 deferred", truth),
    )
    check(
        "and the same numbers unscoped are a headline claim, and wrong",
        count_problems("23 of 29 rows proved", truth),
    )
    check(
        "a prefix nothing registered does not scope anything",
        count_problems("23 of its 29 `ZZ` rows proved", truth),
    )

    check(
        "a stale specification count is caught",
        spec_count_problems("**Fifteen specifications** with vector tables", 13),
    )
    check("the current one passes", not spec_count_problems("Thirteen specifications", 13))
    check("as does a digit", not spec_count_problems("13 specifications", 13))
    check(
        "a word that is not a number is prose, not a count",
        not spec_count_problems("the normative specifications are published", 13),
    )
    check(
        "an inventory that has stopped enumerating is caught",
        inventory_problems("registered: `PB`, `EP`, `RA`", {"PB", "EP", "RA", "SC"}),
    )
    check(
        "and names what is missing",
        "`SC`" in inventory_problems("registered: `PB`, `EP`, `RA`", {"PB", "EP", "RA", "SC"})[0],
    )
    check(
        "a complete one passes",
        not inventory_problems("`PB`, `EP`, `RA`, `SC`", {"PB", "EP", "RA", "SC"}),
    )
    check(
        "a paragraph citing one prefix is not an inventory",
        not inventory_problems("the forwarding rules carry a `PB` table", {"PB", "EP", "RA", "SC"}),
    )

    open_board = {"PX-12": "backlog", "DP-13": "in-progress", "RT-12": "ready",
                  "PX-13": "done", "ET-7": "backlog", "CF-3": "backlog"}
    check(
        "a `today` row claiming an effect the driver discards is refused",
        capability_problems(
            "| Calls between two registered devices | forking, `CANCEL`, Timer C | today |",
            open_board,
        ),
    )
    check(
        "the qualified form the vocabulary already has is not refused",
        not capability_problems(
            "| Calls | forking; `CANCEL` and Timer C are produced and not performed | "
            "today, partly |",
            open_board,
        ),
    )
    check(
        "nor is a `specified, not shipped` row",
        not capability_problems("| Timer C | the decision core | specified, not shipped |", open_board),
    )
    check(
        "the README's **Working** row is a `today` cell written the other way round",
        capability_problems("| **Working** | declared roles gate runtime dispatch |", open_board),
    )
    check(
        "and the same sentence is refused outside a table too",
        capability_problems("declared roles gate runtime dispatch on this build.", open_board),
    )
    # The tails are the point of the next two. A claim that carries on into a second, negative
    # clause is a *stronger* claim than the bare one, and the first version of this check threw
    # both away because it read the refusal word in the tail as a denial of what preceded it.
    check(
        "a promotion is still a promotion when its sentence carries on into a \"not\"",
        capability_problems(
            "declared roles gate runtime dispatch, and a method the role does not wire is "
            "not accepted.",
            open_board,
        ),
    )
    check(
        "and when it carries on into a \"no\"",
        capability_problems(
            "declared roles gate runtime dispatch, so no method reaches another role's handler.",
            open_board,
        ),
    )
    check(
        "while a negation inside the claim itself is the honest sentence and passes",
        not capability_problems("declared roles do not gate runtime dispatch.", open_board),
    )
    # Pinned verbatim from the tree this story corrected (`43594b1`, website/docs/intro.md:49):
    # a trimmed version of this row passes the guard that the real one defeats, so the trimmed
    # version is worth nothing as a test.
    stale = (
        "| **In-dialog routing** | `ACK` and in-dialog requests are resolved by address-of-record "
        "lookup rather than by the `Route` set and the dialog's remote target | engine only |"
    )
    check("a denial that outlived its story is refused", capability_problems(stale, open_board))
    check(
        "and is allowed again while that story is open",
        not capability_problems(stale, dict(open_board, **{"PX-13": "ready"})),
    )
    check(
        "the same denial written with \"not\" is refused too",
        capability_problems(
            "| **In-dialog routing** | `ACK` and in-dialog requests are resolved by "
            "address-of-record lookup, not by the `Route` set | engine only |",
            open_board,
        ),
    )
    check(
        "and written with \"never\"",
        capability_problems(
            "| **In-dialog routing** | `ACK` is resolved by address-of-record lookup and never by "
            "the dialog's remote target | engine only |",
            open_board,
        ),
    )
    check(
        "a page describing the shape as superseded is not making the claim",
        not capability_problems(
            "`ACK` is no longer resolved by address-of-record lookup.", open_board
        ),
    )
    check("an owner that is not on the board fails the run", unknown_owners({}))
    check("and a board carrying all of them does not", not unknown_owners(open_board))
    check(
        # website/docs/intro.md:46, whole cell: the negation sits between the subject and the verb,
        # which is where a real denial sits, and is the reason the guard reaches that far and no
        # further.
        "a clause that denies the capability is what this wants pages to write",
        not capability_problems(
            "| **Proxy — `CANCEL`, Timer C** | Modelled in the decision core and **not performed "
            "by the driver**: the effects are produced and discarded, so a Timer C is armed with "
            "the right value and never fires (`PX-12`) | engine only |",
            open_board,
        ),
    )
    check(
        "and the denial in one cell does not excuse a claim in another",
        capability_problems(
            "| roles gate runtime dispatch | media never enters this process |", open_board
        ),
    )
    check(
        "\"independent implementation\" is recognised",
        INDEPENDENT.search("the other end is an independent implementation") is not None,
    )
    check(
        "so is \"independent parser\"",
        INDEPENDENT.search("an independent parser reads it") is not None,
    )
    check(
        "a separate process is not the same claim",
        INDEPENDENT.search(f"a {REQUIRED_WORDING}") is None,
    )

    dated = ["current text", "", "## Releases", "", "157 of 586 rows proved"]
    check(
        "the release history is a dated record and is not scanned",
        [number for number, _ in present_tense("website/docs/whats-new.md", dated)] == [1, 2],
    )
    check(
        "every other page is read to the end",
        len(present_tense("README.md", dated)) == len(dated),
    )
    return failures


def main() -> int:
    if failures := self_test():
        for failure in failures:
            print(f"self-test: {failure}")
        print(f"\nsite: FAIL — this checker is not holding its own invariant ({len(failures)})")
        return 2

    surface, where, drift = cli_surface()
    expected, banner_where, banner_drift = version_banner()
    commands, verified, unrunnable = check_documented_commands(surface, where)
    versions, banners = check_documented_versions(expected, banner_where)
    claims, scanned = check_release_claims()
    problems = (
        check_reachability()
        + drift
        + check_cli_page(surface, where)
        + commands
        + banner_drift
        + versions
        + claims
        + check_proof_wording()
        + check_proofs_are_gated()
    )

    for problem in problems:
        print(problem)

    # Printed on every run, green or red. The point of the summary is that narrowing what this looks
    # at should be visible in its output rather than in its exit code.
    print(f"\nsite: CLI surface from {where} — {len(sorted(surface))} flag(s): {sorted(surface)}")
    if expected is None:
        print(
            f"site: version banner UNVERIFIED — it could be read from {banner_where}. "
            f"{banners} documented banner(s) went unchecked"
        )
    else:
        print(
            f"site: version banner from {banner_where} — `{expected}`; "
            f"{banners} documented banner(s) checked against it"
        )
    if unrunnable:
        print(f"site: {len(unrunnable)} documented command(s) need a tool this runner has not got:")
        for line in unrunnable:
            print(f"  {line}")

    # What the release-claim scan actually read. Printed green or red, on this file's own rule: a
    # check that quietly narrows its scope is indistinguishable from one that passes.
    counts, _ = conformance()
    module = vector_checker()
    registered = spec_registry(module)[0] if module else 0
    headline = counts.get("", {})
    print(
        f"site: release claims held to {headline.get('proved', '?')}/{headline.get('total', '?')} "
        f"proved, {headline.get('shape only', '?')} shape only, "
        f"{headline.get('deferred', '?')} deferred (docs/reference/conformance.md), "
        f"{registered} registered specification(s) and {len(GOVERNED)} gated capability claim(s) "
        f"— {scanned['lines']} line(s) across {scanned['pages']} page(s), "
        f"{scanned['historical']} skipped as dated release history"
    )

    pages = len([path for path in tracked("website/docs") if path.suffix == ".md"])
    if problems:
        print(f"\nsite: FAIL — {len(problems)} problem(s)")
        return 1
    print(
        f"site: clean ({pages} page(s) reachable, {verified} sipx-clstr command(s) verified, "
        f"{len(unrunnable)} listed above as unrunnable here)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
