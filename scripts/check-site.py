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
5. **Every proof the site offers is in CI, or says why not.** "In CI" means the gate or a workflow
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


def fenced_blocks(path: pathlib.Path) -> list[tuple[int, str, str]]:
    """`(line number, info string, body)` for every fenced block in a file."""
    text = path.read_text(encoding="utf-8")
    return [
        (text[: match.start()].count("\n") + 1, match.group(2).strip(), match.group(3))
        for match in FENCE.finditer(text)
    ]


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
    return failures


def main() -> int:
    if failures := self_test():
        for failure in failures:
            print(f"self-test: {failure}")
        print(f"\nsite: FAIL — this checker is not holding its own invariant ({len(failures)})")
        return 2

    surface, where, drift = cli_surface()
    commands, verified, unrunnable = check_documented_commands(surface, where)
    problems = (
        check_reachability()
        + drift
        + check_cli_page(surface, where)
        + commands
        + check_proofs_are_gated()
    )

    for problem in problems:
        print(problem)

    # Printed on every run, green or red. The point of the summary is that narrowing what this looks
    # at should be visible in its output rather than in its exit code.
    print(f"\nsite: CLI surface from {where} — {len(sorted(surface))} flag(s): {sorted(surface)}")
    if unrunnable:
        print(f"site: {len(unrunnable)} documented command(s) need a tool this runner has not got:")
        for line in unrunnable:
            print(f"  {line}")

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
