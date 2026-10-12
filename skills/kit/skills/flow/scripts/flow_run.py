#!/usr/bin/env python3
"""Make and take flow steps. Run from inside the repo.

  start [--profile <name>]     make tmp/flow/<date>-<branch>/ with the seven step files; the profile packs blocks
  status [<folder>]            print each step's status, whether it is ready, and whether it is stale
  take <folder> <step> <who> [--force] mark the step active and print its file with the skills the catalog gives it
  done <folder> <step>         set the step done, only when every todo is checked with evidence
  skip <folder> <step> <why>    skip design, contracts, or impact, with the reason

`start` writes the full text of each step's skills into that step's file, because an agent that gets
only a path often does not open it (Sonnet workers opened 0 of 4 pointer skills, 2026-10-04 and
2026-10-05), and every agent reads the step file it works on, on every provider. `take` prints them
again. A catalog item `block:<id>` names a block of the kit's blocks/ folder; blocks/blocks.toml owns
the block, alias, and profile format, and the repo's .blocks.toml adds params, repo blocks, and off.

The step graph is the step table in ../SKILL.md. `start` writes the need names and marks 05-build.md
with `Revision: HEAD`, as references/step-run.md describes. The kit's references/step_run.py
owns status, ready, stale, take, done, and events.log. This script adds the flow rules: the revision
of 05-build is HEAD, take 04-impact lists the places that name the contracts' code names, and take
07-close needs the review-check run's own APPROVE verdict on HEAD.
"""

import codecs
import datetime
import hashlib
import json
import os
import re
import shlex
import subprocess
import sys
import tempfile
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3] / "references"))
import step_run  # noqa: E402
from step_run import RESULT_HEAD, TODO_HEAD, plain, status_of  # noqa: E402

SKILL_MD = Path(__file__).resolve().parents[1] / "SKILL.md"
BLOCKS = Path(__file__).resolve().parents[3] / "blocks"
SKIPPABLE = ("02-design", "03-contracts", "04-impact")
SEARCH_SECONDS = 120
SEARCH_EACH_SECONDS = 60
NAME_CAP = 25
LINE_CHUNK = 4096
MAX_HITS = 10
HIT_CHARS = 160
CAP = 15000  # characters per skill, cut to it, and per block, refused over it; engineering-standards is about 10k
RULES_HEAD = "## Rules for this step (written by flow_run.py start; apply them, keep this section)"
JUDGE_HEAD = "## Judge checks of blocks that ran"
PARAM = re.compile(r"\{\{(\w+)\}\}")
CHECKS = ("Check (script)", "Check (judge)")
CALLERS_HEAD = "## Callers (written by flow_run.py take from the code names in 03-contracts.md; a search, not a judgment)"
TODOS = {  # each item names the file that owns the rule
    "01-frame": ["Goal, user, and out of scope (flow)", "Done-when, each one checkable (flow)",
                 "Lane, fast or full, and why (flow)"],
    "02-design": ["UX flow and screens, or the reason no UI changes (flow)"],
    "03-contracts": ["APIs, data model, and integrations that change (flow)",
                     "Standards rows that apply (engineering-standards)", "Decision check for the topic (decisions)"],
    "04-impact": ["Existing features that change, judged from the Callers section and the contracts, or independent (flow)"],
    "05-build": ["Task file path and Base commit (pair or deliver)",
                 "Failing check first: command and its failing output (prove-it-works)",
                 "Standards rows for each behavior, or none and why (engineering-standards)",
                 "Scope: each file touched and the part of the change that names it (AGENTS.md)",
                 "Dead code: what this change made unused, removed; older dead code, named (AGENTS.md)",
                 "Check passes: command and its passing output (prove-it-works)"],
    "06-review": ["review-check range, verdict, and each fix row with its fix or reason (review-check)",
                  "A `Run: tmp/review-check/<run>` line under Result for the last run (flow)"],
    "07-close": ["Each done-when of 01-frame with its evidence (prove-it-works)"],
}


def git(*args):
    return subprocess.run(["git", *args], capture_output=True, text=True).stdout.strip()


def flow_home():
    return Path.home() / ".flow"


def repo_name():
    return Path(git("rev-parse", "--path-format=absolute", "--git-common-dir")).parent.name


def graph():
    """Each step file and the steps it needs, from the step table in SKILL.md."""
    rows = step_run.table(SKILL_MD)
    names = [r[0] for r in rows]
    return {name: step_run.need_files(need, names) for name, need, _ in rows}


def rev(folder, step):
    """05-build's revision is the commit it built, so a new commit makes its review stale."""
    return git("rev-parse", "HEAD")[:12] if step == "05-build" else step_run.revision(folder, step)


def start(profile="default"):
    root = git("rev-parse", "--show-toplevel")
    if not root:
        raise SystemExit("not in a git repository")
    branch = git("branch", "--show-current") or "detached"
    folder = Path(root) / "tmp" / "flow" / f"{datetime.date.today()}-{branch.replace('/', '-')}"
    exclude = Path(git("rev-parse", "--path-format=absolute", "--git-common-dir")) / "info" / "exclude"
    try:  # a sandboxed agent may not write inside .git
        exclude.parent.mkdir(exist_ok=True)
        if "/tmp/" not in (exclude.read_text(encoding="utf-8").splitlines() if exclude.exists() else []):
            with exclude.open("a", encoding="utf-8") as f:
                f.write("/tmp/\n")
    except (OSError, UnicodeDecodeError) as error:  # an exclude file that is not UTF-8 gets this note too
        print(f"note: could not add /tmp/ to {exclude}: {error}", file=sys.stderr)
    try:  # a second start packs nothing, but an unknown name is still an error; default needs no TOML read
        if profile != "default":
            kit_profile(read_toml(BLOCKS / "blocks.toml"), BLOCKS / "blocks.toml", profile)
    except SystemExit as error:
        raise SystemExit(one_line(str(error.code))) from None
    if folder.exists() and profile != "default":
        print(f"note: {folder} exists, so --profile {profile} was not applied", file=sys.stderr)
    if not folder.exists():
        steps = graph()
        # Read every step's text before the folder exists, so a config or read error leaves no half-made run.
        try:
            catalog = {step: skill_paths(step) for step in steps}
            blocks, judge = resolve_blocks(Path(root), {s.split("-", 1)[1]: ids for s, (_, ids) in catalog.items()}, profile)
            skills = {step: skills_text(catalog[step][0]) for step in steps}
        except SystemExit as error:  # its reasons quote table names, keys, and paths from the repo's .blocks.toml
            raise SystemExit(one_line(str(error.code))) from None
        folder.mkdir(parents=True)
        for step, need in steps.items():
            extra = f"Worktree: {root}\nBranch: {branch}\n" if step == "01-frame" else ""
            extra += "Revision: HEAD\n" if step == "05-build" else ""  # the rule in rev(), for readers without this script
            uses = f"Uses: {', '.join(need)}".rstrip()
            paths = catalog[step][0]
            read, text, todos = blocks[step.split("-", 1)[1]]
            rules = "\n".join(part for part in (skills[step], text) if part)
            judged = f"{JUDGE_HEAD}\n{judge}\n\n" if step == "06-review" and judge else ""
            (folder / f"{step}.md").write_text(
                f"Status: open\n{uses}\n{extra}Take with: python3 {Path(__file__).resolve()} take {folder} {step} <agent>\n"
                f"Skills read: {', '.join(paths + read) or 'none'}\n\n{RULES_HEAD}\n{rules}\n{judged}"
                f"{TODO_HEAD}\n" + "".join(f"- [ ] {item}: \n" for item in TODOS[step] + todos) + f"\n{RESULT_HEAD}\n", encoding="utf-8")
    print(folder)
    print(f"Each step file holds its rules, a todo list, and a result section. Close a step with: python3 {Path(__file__).resolve()} done <folder> <step>")


def status(folder=None):
    if folder is None:
        folders = sorted(p for p in Path(git("rev-parse", "--show-toplevel"), "tmp", "flow").glob("2*/"))
        if not folders:
            raise SystemExit("no flow folder")
        folder = folders[-1]
    print(folder)
    step_run.status(folder, graph(), rev)


def skill_paths(step):
    """The catalog items for this step, from the `any` section, each profile domain, and the repo rules:
    the paths, and apart from them the `block:<id>` items."""
    key = step.split("-", 1)[1]
    profile = flow_home() / repo_name() / "profile.md"
    domains, repo_rules = {"any"}, []
    if profile.exists():
        text = read_utf8(profile)
        if m := re.search(r"^Domains:\s*(.+)$", text, re.M):
            domains |= {d.strip() for d in m.group(1).split(",") if d.strip()}
        if m := re.search(r"^Repo rules:\s*(.+)$", text, re.M):
            repo_rules = [r.strip() for r in m.group(1).split(",") if r.strip() and r.strip() != "none"]
    else:
        print(f"note: no profile at {profile}; the flow skill's start rule says what to do", file=sys.stderr)
    items, section = [], None
    for line in read_utf8(flow_home() / "domains.md").splitlines():
        if line.startswith("## "):
            section = line[3:].strip()
        elif section in domains and line.startswith(f"- {key}:"):
            items += [p.strip() for p in line.split(":", 1)[1].split(",") if p.strip()]
    root = Path(git("rev-parse", "--show-toplevel"))
    paths = [p for p in dict.fromkeys(items) if not p.startswith("block:")]
    return paths + [str(root / r) for r in repo_rules], [p.removeprefix("block:") for p in items if p.startswith("block:")]


def kit_profile(kit, kit_file, name):
    if (profile := kit.get("profiles", {}).get(name)) is None:
        raise SystemExit(f"unknown profile {name}; {kit_file} has {', '.join(kit.get('profiles', {}))}")
    return profile


def resolve_blocks(root, wanted, profile_name):
    """For each step key in `wanted` (key -> catalog block ids), the blocks it gets from the catalog and
    the repo's .blocks.toml, packed by the profile: (Skills read items, rules text, script todos). Also
    the judge lines of every block that ran, once each. blocks/blocks.toml owns the format."""
    kit_file, repo_file = BLOCKS / "blocks.toml", root / ".blocks.toml"
    # A run with no block reads no TOML, so it starts as before on a Python before 3.11 or with no blocks/.
    if profile_name == "default" and not repo_file.exists() and not any(wanted.values()):
        return {key: ([], "", []) for key in wanted}, ""
    kit = read_toml(kit_file)
    files = {block: BLOCKS / name for block, name in kit.get("blocks", {}).items()}
    aliases = kit.get("aliases", {})
    for alias, members in aliases.items():
        if alias in files:
            raise SystemExit(f"alias {alias} is also a block id in {kit_file}; rename one")
        if unknown := [m for m in members if m not in files]:
            raise SystemExit(f"unknown block id {unknown[0]} in alias {alias} of {kit_file}")
    if unknown := [b for b in kit.get("base", []) if b not in files]:
        raise SystemExit(f"unknown block id {unknown[0]} in base of {kit_file}")
    profile = kit_profile(kit, kit_file, profile_name)
    merge = profile.get("merge", [])
    if unknown := [name for name in merge if name not in aliases]:
        raise SystemExit(f"profile {profile_name} merges {unknown[0]}, which is not an alias in {kit_file}")

    repo = read_toml(repo_file) if repo_file.exists() else {}
    if unknown := sorted(set(repo) - {"params", "add", "off"}):
        raise SystemExit(f"unknown table [{unknown[0]}] in {repo_file}; it takes [params], [add], and [off]")
    if wrong := [t for t in ("params", "add", "off") if not isinstance(repo.get(t, {}), dict)]:
        raise SystemExit(f"{repo_file}: {wrong[0]} must be a table, written as [{wrong[0]}]")
    params, off, added = repo.get("params", {}), repo.get("off", {}), {}
    if bad := next((k for k, v in params.items() if not isinstance(v, str)), None):
        raise SystemExit(f"[params] {bad} must be a string in {repo_file}, such as {bad} = \"docs/x.md\"")
    # An empty value would reach a script check as '', which a command reads as a missing path.
    if bad := next((k for k, v in params.items() if not one_line(v).strip()), None):
        raise SystemExit(f"[params] {bad} is empty in {repo_file}; a value is a path or a name, such as {bad} = \".\"")
    # A value read as an option would change a block command, and one that starts a line with `## ` would put a
    # step heading (`## Todo (`, `## Result`) into the rules text, where done would take it for the todo list.
    if bad := next((k for k, v in params.items() if one_line(str(v)).lstrip()[:1] in ("-", "#")), None):
        raise SystemExit(f"[params] {bad} starts with {one_line(str(params[bad])).lstrip()[0]!r} in {repo_file}; "
                         "a value is a path or a name, never an option or a heading")
    # A backtick would end a script check's code span early, so the todo would not hold the command as written.
    if bad := next((k for k, v in params.items() if "`" in str(v)), None):
        raise SystemExit(f"[params] {bad} holds a backtick in {repo_file}; a value is a path or a name")
    for key, paths in repo.get("add", {}).items():
        if key not in wanted:
            raise SystemExit(f"[add] {key} in {repo_file} is not a step; steps are {', '.join(wanted)}")
        if not isinstance(paths, list) or not all(isinstance(p, str) for p in paths):
            raise SystemExit(f"[add] {key} in {repo_file} must be a list of paths, such as {key} = [\"docs/x.md\"]")
        for path in paths:
            if "\0" in path:
                raise SystemExit(f"[add] {path} in {repo_file} holds a NUL byte")
            try:  # Python 3.11 and 3.12 raise for a symlink loop (RuntimeError) or a too-long name (OSError)
                outside = Path(path).is_absolute() or not (root / path).resolve().is_relative_to(root.resolve())
                is_file = (root / path).is_file()
            except (OSError, RuntimeError):
                raise SystemExit(f"[add] {path} in {repo_file} is not a file") from None
            # The repo file is repo-owned input, and the step text goes to agents, so no file outside the repo.
            if outside:
                raise SystemExit(f"[add] {path} leaves the repo; {repo_file} takes repo-relative paths")
            block = Path(path).stem
            # The id goes into the step file and take's output, so it is one plain word.
            if not re.fullmatch(r"[A-Za-z0-9._-]+", block):
                raise SystemExit(f"[add] {path} in {repo_file} has id {block}, which is not only letters, digits, "
                                 "dots, dashes, and underscores; rename the file")
            if block in aliases:
                raise SystemExit(f"[add] {path} in {repo_file} has id {block}, which is an alias in {kit_file}; rename the file")
            if files.get(block, root / path) != root / path:
                raise SystemExit(f"duplicate block id {block}: {path} in {repo_file} and {files[block]}")
            if not is_file:
                raise SystemExit(f"[add] {path} in {repo_file} is not a file")
            files[block] = root / path
            added.setdefault(key, []).append(block)
    # ponytail: off works per block; per-check off (block id plus check line) is the upgrade if a repo needs it.
    for block, reason in off.items():
        if block not in files:
            raise SystemExit(f"unknown block id {block} in [off] of {repo_file}")
        if not isinstance(reason, str) or not one_line(reason).strip():  # the text that the Off line prints
            raise SystemExit(f"[off] {block} needs a reason in {repo_file}")

    named = {}
    for key, ids in wanted.items():
        if unknown := [i for i in ids if i not in files and i not in aliases]:
            raise SystemExit(f"unknown block id {unknown[0]} in the {key} line of ~/.flow/domains.md")
        named[key] = [m for i in ids for m in aliases.get(i, [i])] + added.get(key, [])
    base = kit.get("base", []) if any(named.values()) else []  # a run that names no block stays as before

    loaded = {}

    def load(block):
        """The block's content hash and its filled sections, read once."""
        if block not in loaded:
            data = read_bytes(files[block])
            # Repo text goes to agents and to take's output: a param is one line, and no text keeps control bytes.
            # The param check reads the filtered text, the same text the fill reads.
            text = plain(read_utf8(files[block], data), keep_newlines=True)
            # A large repo file would fill every step file, and a cut could end inside a check, so it stops start.
            if len(text) > CAP:
                raise SystemExit(f"block {block} is {len(text)} characters; a block holds at most {CAP}")
            if missing := [p for p in PARAM.findall(text) if p not in params]:
                raise SystemExit(f"block {block} needs {{{{{missing[0]}}}}}; set it under [params] in {repo_file}")
            def fill(part, quote=False):
                return PARAM.sub(lambda m: (shlex.quote if quote else str)(one_line(str(params[m[1]]))), part)

            # A script check runs its backtick spans in a shell, so a value there is one quoted word; the rest of
            # the line, and other sections, get the value as written. Odd split parts are the spans.
            parts = [(head, ["".join(fill(p, head == CHECKS[0] and i % 2) for i, p in enumerate(re.split(r"(`[^`]*`)", line)))
                             for line in lines]) for head, lines in sections(text)]
            # sections() split the raw text, so a `## ` line here was made by a param; in the step file it
            # would be a step heading, such as `## Todo (` or `## Result`, that done takes for the todo list.
            if made := next((l for _, lines in parts for l in lines if l.startswith("## ")), None):
                raise SystemExit(f"block {block} has the line {made!r} once its params are filled; "
                                 f"a param must not make a '## ' heading")
            # A misspelled check heading would drop its checks without a word.
            if bad := [h for h, _ in parts[1:] if h.lower().startswith("check") and h not in CHECKS]:
                raise SystemExit(f"block {block} has the section heading ## {bad[0]}; a check section is "
                                 f"## {CHECKS[0]} or ## {CHECKS[1]}")
            # So would a check line that is not a `- ` item, as `* ` or an indented `- ` is.
            if bad := next(((h, l) for h, lines in parts[1:] if h in CHECKS for l in lines
                            if l.strip() and not l.startswith("- ")), None):
                raise SystemExit(f"block {block} has the line {bad[1]!r} under ## {bad[0]}; "
                                 f"each check is one line that starts with '- '")
            # So would a missing check section; blocks/blocks.toml says every block has both.
            if missing := next((h for h in CHECKS if h not in [head for head, _ in parts[1:]]), None):
                raise SystemExit(f"block {block} has no ## {missing} section; every block has one, "
                                 "with `- none: <why>` when it has no check")
            # The hash is of the file's bytes; the text has LF for CRLF, so it cannot give it.
            loaded[block] = (hashlib.sha256(data).hexdigest()[:8], parts)
        return loaded[block]

    out, judge = {}, {}
    for key, ids in named.items():
        joined = list(dict.fromkeys(base + ids))
        blocks = [b for b in joined if b not in off]
        groups = {}  # a merged alias sits where its first member stood
        for b in blocks:
            groups.setdefault(next((a for a in merge if b in aliases[a]), b), []).append(b)
        rest = [g for g in groups if g not in base]
        if profile.get("merge_base") and rest:
            folded = [b for g in groups if g in base for b in groups[g]]
            groups = {g: folded + groups[g] if g == rest[0] else groups[g] for g in rest}
        text = "\n".join([f"Off: {b}: {one_line(str(off[b]))}" for b in joined if b in off]
                         + [render(name, [load(b)[1] for b in members], profile.get("examples", True))
                            for name, members in groups.items()])
        read = [f"profile {profile_name}"] + [f"block:{b}@{load(b)[0]}" for b in blocks] if blocks else []
        todos = [f"block {b} script: {line}" for b in blocks for line in check_lines(load(b)[1], "Check (script)")]
        for b in blocks:
            judge.setdefault(b, check_lines(load(b)[1], "Check (judge)"))
        out[key] = (read, text, todos)
    return out, "\n".join(f"- block {b} judge: {line}" for b, lines in judge.items() for line in lines)


def one_line(text):
    return plain(text, keep_newlines=True).replace("\n", " ").replace("\t", " ")


def read_toml(path):
    try:  # only start reads TOML, so the other commands still run on a Python before 3.11
        import tomllib
    except ImportError:
        raise SystemExit(f"{path}: reading TOML needs Python 3.11 or later (tomllib)") from None
    try:
        return tomllib.loads(read_utf8(path))
    except (tomllib.TOMLDecodeError, RecursionError, ValueError) as error:  # a deep array, a huge integer
        raise SystemExit(f"{path} is not valid TOML: {error}") from None


def read_bytes(path):
    try:
        return path.read_bytes()
    except OSError as error:
        raise SystemExit(f"{path} cannot be read: {error.strerror}") from None


def read_utf8(path, data=None):
    """The file's text with each line end as LF, as read_text gives it; `data` is its bytes if already read."""
    try:
        return (read_bytes(path) if data is None else data).decode("utf-8").replace("\r\n", "\n").replace("\r", "\n")
    except UnicodeDecodeError as error:
        raise SystemExit(f"{path} is not UTF-8: {error}") from None


def sections(text):
    """A block's header lines and each `## ` section, in order: [(None, header), (heading, lines), ...]."""
    parts = [(None, [])]
    for line in text.splitlines():
        if line.startswith("## "):
            parts.append((line[3:].strip(), []))
        else:
            parts[-1][1].append(line)
    return parts


def check_lines(parts, heading):
    return [line[2:] for head, lines in parts if head == heading for line in lines
            if line.startswith("- ") and not line.startswith("- none:")]


def render(name, members, examples):
    """Blocks as step text under one name, without their check sections, which become todos and judge
    lines. One member is the block itself; several are one merged block."""
    lines = [f"===== block:{name} =====", f"# {name}"]
    lines += [line for parts in members for line in parts[0][1] if not line.startswith("# ")]
    body = {}
    for parts in members:
        for head, section in parts[1:]:
            if head not in CHECKS and (examples or head != "Example"):
                body.setdefault(head, []).extend(section)
    # `### ` nests each section under the block marker. sections() split on every `## ` line, so no block
    # line now starts with `## `, and repo text cannot stand in for the step's todo head or `## Result`.
    return "\n".join(lines + [line for head, section in body.items() for line in (f"### {head}", *section)]).rstrip() + "\n"


def take(folder, step, who, force=False):
    need = graph().get(step)
    if need is None:
        raise SystemExit(f"unknown step {step}; steps are {', '.join(graph())}")
    if not step_run.ready(folder, step, need):
        waiting = [u for u in need if status_of(folder, u).split()[0] not in ("done", "skipped")]
        raise SystemExit(f"{step} is not ready; it needs {', '.join(waiting)}")
    if step == "06-review":
        runs = len(re.findall(r"^Run:", (Path(folder) / "06-review.md").read_text(encoding="utf-8"), re.M))
        if runs >= 2:
            print(f"06-review already lists {runs} runs; bring the open rows to the user before a new range")
    if step == "07-close":
        approved(folder)
    step_run.take(folder, step, who, need, rev, force=force,
                  on_claim=write_callers if step == "04-impact" else None)


def done(folder, step):
    # Only 04-impact gets a Callers section; elsewhere that line is pasted text and must not end the list.
    ends = (RESULT_HEAD, CALLERS_HEAD) if step == "04-impact" else (RESULT_HEAD,)
    step_run.done(folder, step, rev(folder, step) if step == "05-build" else None, ends=ends)


def skip(folder, step, *why):
    if step not in SKIPPABLE:
        raise SystemExit(f"{step} cannot be skipped; only {', '.join(SKIPPABLE)} can")
    reason = one_line(" ".join(why)).strip()
    if not reason:
        raise SystemExit("skip needs a reason, for example: skip <folder> 02-design no UI")
    file = Path(folder) / f"{step}.md"
    with step_run.step_lock(folder, step, command="skip"):
        if status_of(folder, step).split()[0] not in ("open", "active"):
            raise SystemExit(f"{step} is {status_of(folder, step)}; only an open or active step can be skipped")
        step_run.write_step(file, f"Status: skipped {reason}\n" + file.read_text(encoding="utf-8").split("\n", 1)[1])
    step_run.log(folder, step, f"skip {reason}")
    print(f"{step}: skipped {reason}")


def approved(folder):
    """Refuse 07-close unless 06-review names a review-check run whose verdict is APPROVE on HEAD.
    The verdict is read from the run's own 03-verdict.md, not from what the step file says."""
    text = (Path(folder) / "06-review.md").read_text(encoding="utf-8")
    runs = re.findall(r"^Run: (\S+)", text, re.M)
    if not runs:
        raise SystemExit("07-close needs a `Run: tmp/review-check/<run>` line in 06-review.md")
    run = Path(git("rev-parse", "--show-toplevel")) / runs[-1]
    lines = read_utf8(run / "03-verdict.md").splitlines() if (run / "03-verdict.md").exists() else []
    verdict = lines[2] if len(lines) > 2 else f"no verdict in {run}/03-verdict.md"
    if not verdict.startswith("Verdict: APPROVE"):
        raise SystemExit(f"07-close needs an APPROVE verdict on HEAD. {runs[-1]} says: {verdict}")
    target = {}
    if (run / "target.json").exists():
        try:
            target = json.loads(read_utf8(run / "target.json"))
        except (ValueError, RecursionError) as error:  # bad JSON, a number past the int digit limit, a deep array
            raise SystemExit(f"{run}/target.json is not valid JSON: {error}") from None
        if not isinstance(target, dict):
            raise SystemExit(f"{run}/target.json is not a JSON object")
    head = target.get("head", "") if isinstance(target.get("head"), str) else ""
    if not head.startswith(rev(folder, "05-build")):
        raise SystemExit(f"{runs[-1]} covers {head[:12] or 'no head'}, but the build is now {rev(folder, '05-build')}; "
                         "run review-check on the new range")


def code_names(text):
    """Code names in backticks: `parse_phone()`, `store::migrate`, `SWARM_HOME`. A plain lowercase word
    such as `init` counts only with () or ::, because alone it matches too much."""
    # ponytail: a regex over backticks, not a parser; it misses a name the contracts never quote.
    names = []
    for tok in re.findall(r"`([^`\n]+)`", text):
        if " " in tok or "/" in tok:
            continue
        name = re.split(r"::|\.|#", tok.removesuffix("()"))[-1]
        codeish = "()" in tok or "::" in tok or "_" in name or any(c.isupper() for c in name)
        if re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]{2,}", name) and codeish:
            names.append(name)
    return list(dict.fromkeys(names))


def decode_prefix(chunk):
    # A bounded read can end inside a UTF-8 character; discard that incomplete tail.
    return codecs.utf_8_decode(chunk, "backslashreplace", False)[0].rstrip("\n")


def write_callers(folder):
    contracts = (folder / "03-contracts.md").read_text(encoding="utf-8")
    # Below the rules, which quote other code; a list cut by a `## ` line still has its head.
    names = code_names(contracts[max(step_run.todo_scan(contracts)[0], 0):])
    omitted = max(0, len(names) - NAME_CAP)
    names = names[:NAME_CAP]
    root = git("rev-parse", "--show-toplevel")
    parts = []
    deadline = time.monotonic() + SEARCH_SECONDS
    for index, name in enumerate(names):
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            parts.append(f"callers: search stopped after {SEARCH_SECONDS} s, {len(names) - index} names not searched")
            break
        try:
            with tempfile.TemporaryFile() as stdout, tempfile.TemporaryFile() as stderr:
                result = subprocess.run(["git", "grep", "-n", "-w", "-F", "-e", name, "--", ".", ":!tmp"],
                                        cwd=root, stdout=stdout, stderr=stderr, timeout=min(SEARCH_EACH_SECONDS, remaining))
                if result.returncode not in (0, 1):
                    stderr.seek(0)
                    error = decode_prefix(stderr.readline(LINE_CHUNK))
                    parts.append(f"### {name}\ncallers: git grep failed: {plain(error)}")
                    continue
                stdout.seek(0)
                hits, count = [], 0
                while prefix := stdout.readline(LINE_CHUNK):
                    line = prefix
                    # Drain long lines in chunks, retaining only their bounded prefix.
                    while line and not line.endswith(b"\n"):
                        line = stdout.readline(LINE_CHUNK)
                    hit = decode_prefix(prefix)
                    if hit:
                        count += 1
                        if len(hits) < MAX_HITS:
                            hits.append(plain(hit)[:HIT_CHARS])
        except subprocess.TimeoutExpired:
            parts.append(f"### {name}\ncallers: git grep timed out")
            continue
        more = f"\n... and {count - MAX_HITS} more" if count > MAX_HITS else ""
        parts.append(f"### {name} ({count} places)\n" + "\n".join(hits) + more)
    if omitted:
        parts.append(f"... and {omitted} more names not searched")
    body = "\n\n".join(parts) or "No code names in backticks in 03-contracts.md. Search by hand and say so."
    file = folder / "04-impact.md"
    text = file.read_text(encoding="utf-8")
    if CALLERS_HEAD in text:
        return
    head, _, tail = text.rpartition(f"\n{RESULT_HEAD}")
    step_run.write_step(file, f"{head}\n{CALLERS_HEAD}\n{body}\n\n{RESULT_HEAD}{tail}" if _ else f"{text}\n{CALLERS_HEAD}\n{body}\n")


def demote(text):
    """As in render: `## ` becomes `### `, so no rules line can pass for a step heading. Inside a ``` or ~~~
    fence a `## ` line is quoted format and stays, except a TODO_MARK line, which could still pass for a head."""
    lines, fence = text.split("\n"), None
    for i, line in enumerate(lines):
        if mark := re.match(r" {0,3}(`{3,}|~{3,})", line):
            if fence is None:
                fence = mark[1]
            # As in CommonMark, only a bare fence line closes: a line such as ```js inside the fence is content.
            elif mark[1][0] == fence[0] and len(mark[1]) >= len(fence) and not line[mark.end():].strip(" \t"):
                fence = None
        elif line.startswith("## ") and (fence is None or line.startswith(step_run.TODO_MARK)):
            lines[i] = "#" + line
    return "\n".join(lines)


def skills_text(paths):
    parts = []
    for path in paths:
        p = Path(path).expanduser()
        if not p.exists():
            parts.append(f"===== {path} (missing) =====")
            continue
        text = demote(plain(read_utf8(p), keep_newlines=True))
        cut = f" (first {CAP} characters; read the file for the rest)" if len(text) > CAP else ""
        parts.append(f"===== {path}{cut} =====\n{text[:CAP]}")
    return "\n".join(parts)


if __name__ == "__main__":
    if hasattr(sys.stdout, "reconfigure"):
        # Escape non-ASCII output when the console uses an ASCII encoding.
        sys.stdout.reconfigure(errors="backslashreplace")
    commands = {"start": start, "status": status, "take": take, "done": done, "skip": skip}
    arity = {"start": (0, 0), "status": (0, 1), "take": (3, 3), "done": (2, 2), "skip": (3, None)}
    if len(sys.argv) < 2 or sys.argv[1] not in commands:
        raise SystemExit(__doc__)
    try:
        args = sys.argv[2:]
        command = sys.argv[1]
        force = command == "take" and "--force" in args
        if command == "take":
            args = [arg for arg in args if arg != "--force"]
        if command in ("done", "skip") and "--force" in args:
            raise SystemExit("done <folder> <step> accepts no --force" if command == "done" else "skip accepts no --force")
        options = {"force": force} if command == "take" else {}
        if command == "start" and len(args) == 2 and args[0] == "--profile":
            options, args = {"profile": args[1]}, []
        minimum, maximum = arity[command]
        if len(args) < minimum or maximum is not None and len(args) > maximum:
            raise SystemExit(next(line.strip() for line in __doc__.splitlines()
                                  if line.strip().split()[:1] == [command]))
        commands[command](*args, **options)
        sys.stdout.flush()
    except BrokenPipeError:
        fd = os.open(os.devnull, os.O_WRONLY)
        # Python flushes stdout at exit; devnull prevents another error from the closed pipe.
        os.dup2(fd, sys.stdout.fileno())
        os.close(fd)
        sys.exit(0)
