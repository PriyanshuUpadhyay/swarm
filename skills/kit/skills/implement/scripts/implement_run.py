#!/usr/bin/env python3
"""Make and take implement steps. Run from inside the repo.

  start [--profile <name>] [--domains <a,b>] make tmp/flow/<date>-<branch>/ with selected profile domains
  status [<folder>]            print each step's status, whether it is ready, and whether it is stale
  take <folder> <step> <who> [--force] mark the step active and print its file with the skills the catalog gives it
  done <folder> <step>         set done with evidence; 07-close moves the run to tmp/flow/_closed/
  skip <folder> <step> <why>    skip design, contracts, or impact, with the reason
  block <folder> <step> <why>   block an open or active step, with the reason

`start` writes the full text of each step's skills into that step's file, because an agent that gets
only a path often does not open it (Sonnet workers opened 0 of 4 pointer skills, 2026-10-04 and
2026-10-05), and every agent reads the step file it works on, on every provider. `take` prints them
again. A catalog item `block:<id>` names a block of the kit's blocks/ folder; blocks/blocks.toml owns
the block, alias, and profile format, and the repo's .blocks.toml adds params, repo blocks, and off.

The step graph is the step table in ../SKILL.md. `start` writes the need names and marks 05-build.md
with `Revision: HEAD`, as references/step-run.md describes. The kit's references/step_run.py
owns status, ready, stale, take, done, and events.log. This script adds the implement rules: the revision
of 05-build is HEAD, take 04-impact lists the places that name the contracts' code names, and take
07-close needs the review-check run's own APPROVE verdict on HEAD.
"""

import codecs
import datetime
import json
import os
import re
import subprocess
import sys
import tempfile
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3] / "references"))
import step_run  # noqa: E402
from step_run import RESULT_HEAD, TODO_HEAD, kit_path, plain, status_of  # noqa: E402
from block_resolver import BLOCKS, CAP, kit_profile, one_line, read_toml, read_utf8, resolve_blocks, sections  # noqa: E402

SKILL_MD = Path(__file__).resolve().parents[1] / "SKILL.md"
SKIPPABLE = ("02-design", "03-contracts", "04-impact")
SEARCH_SECONDS = 120
SEARCH_EACH_SECONDS = 60
NAME_CAP = 25
LINE_CHUNK = 4096
MAX_HITS = 10
HIT_CHARS = 160
RULES_HEAD = "## Rules for this step (written by implement_run.py start; apply them, keep this section)"
JUDGE_HEAD = "## Judge checks of blocks that ran"
LEDGER_HEAD = "## Blocks considered"
# write_callers uses this stored heading to avoid duplicate sections in existing runs.
CALLERS_HEAD = "## Callers (written by implement_run.py take from the code names in 03-contracts.md; a search, not a judgment)"
TODOS = {  # each item names the file that owns the rule
    "01-frame": ["Goal, user, and out of scope (implement)", "Done-when, each one checkable (implement)",
                 "Lane, fast or full, and why (implement)"],
    "02-design": ["UX flow and screens, or the reason no UI changes (implement)"],
    "03-contracts": ["APIs, data model, and integrations that change (implement)",
                     "Standards rows that apply (engineering-standards)", "Decision check for the topic (decisions)"],
    "04-impact": ["Existing features that change, judged from the Callers section and the contracts, or independent (implement)"],
    "05-build": ["Task file path and Base commit (pair or deliver)",
                 "Failing check first: command and its failing output (prove-it-works)",
                 "Standards rows for each behavior, or none and why (engineering-standards)",
                 "Scope: each file touched and the part of the change that names it (AGENTS.md)",
                 "Dead code: what this change made unused, removed; older dead code, named (AGENTS.md)",
                 "Check passes: command and its passing output (prove-it-works)"],
    "06-review": ["review-check range, verdict, and each fix row with its fix or reason (review-check)",
                  "A `Run: tmp/review-check/<run>` line under Result for the last run (implement)"],
    "07-close": ["Each done-when of 01-frame with its evidence (prove-it-works)"],
}


def git(*args):
    return subprocess.run(["git", *args], capture_output=True, text=True).stdout.strip()


def implement_home():
    return Path.home() / ".flow"


def repo_name():
    return Path(git("rev-parse", "--path-format=absolute", "--git-common-dir")).parent.name


def graph():
    """Each step file and the steps it needs, from the step table in SKILL.md."""
    rows = step_run.table(SKILL_MD)
    names = [r[0] for r in rows]
    return {name: step_run.need_files(need, names) for name, need, _, _ in rows}


def rev(folder, step):
    """05-build's revision is the commit it built, so a new commit makes its review stale."""
    return git("rev-parse", "HEAD")[:12] if step == "05-build" else step_run.revision(folder, step)


def start(profile="default", domains=None):
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
    allowed, _ = repo_profile()
    chosen = allowed if domains is None else {d.strip() for d in domains.split(",") if d.strip()} | {"any"}
    if unknown := chosen - allowed:
        raise SystemExit(f"unknown domains: {', '.join(sorted(unknown))}; allowed domains: {', '.join(sorted(allowed))}")
    if folder.exists() and domains is not None:
        print(f"note: {folder} exists, so --domains {domains} was not applied", file=sys.stderr)
    if not folder.exists():
        steps = graph()
        # Read every step's text before the folder exists, so a config or read error leaves no half-made run.
        try:
            catalog = {step: skill_paths(step, chosen) for step in steps}
            blocks, judge, ledger = resolve_blocks(Path(root), {s.split("-", 1)[1]: ids for s, (_, ids) in catalog.items()}, profile)
            skills = {step: skills_text(catalog[step][0]) for step in steps}
        except SystemExit as error:  # its reasons quote table names, keys, and paths from the repo's .blocks.toml
            raise SystemExit(one_line(str(error.code))) from None
        folder.mkdir(parents=True)
        for step, need in steps.items():
            extra = f"Worktree: {root}\nBranch: {branch}\nDomains: {', '.join(sorted(chosen))}\n" if step == "01-frame" else ""
            extra += "Revision: HEAD\n" if step == "05-build" else ""  # the rule in rev(), for readers without this script
            uses = f"Uses: {', '.join(need)}".rstrip()
            paths = catalog[step][0]
            read, text, todos = blocks[step.split("-", 1)[1]]
            rules = "\n".join(part for part in (skills[step], text) if part)
            judged = f"{JUDGE_HEAD}\n{judge}\n\n" if step == "06-review" and judge else ""
            considered = f"{LEDGER_HEAD}\n\n{ledger}\n" if step == "01-frame" else ""
            # The step file lives in the repo, so a kit-root path in the copied text needs the kit's place.
            if any(token in rules + judged for token in ("blocks/", "the kit's `references/", "<kit root>")):
                rules = f"Kit blocks (`blocks/<id>/`) and the kit's `references/` are under {kit_path(BLOCKS.parent)} (the kit root).\n{rules}"
            (folder / f"{step}.md").write_text(
                f"Status: open\n{uses}\n{extra}Take with: python3 {kit_path(Path(__file__).resolve())} take {folder} {step} <agent>\n"
                f"Skills read: {', '.join(paths + read) or 'none'}\n\n{considered}{RULES_HEAD}\n{rules}\n{judged}"
                f"{TODO_HEAD}\n" + "".join(f"- [ ] {item}: \n" for item in TODOS[step] + todos) + f"\n{RESULT_HEAD}\n", encoding="utf-8")
    print(folder)
    print(f"Each step file holds its rules, a todo list, and a result section. Close a step with: python3 {kit_path(Path(__file__).resolve())} done <folder> <step>")
    # Read the saved choice on repeat starts; changed domain lines apply only to a new run.
    frame = read_utf8(folder / "01-frame.md") if (folder / "01-frame.md").exists() else ""
    if saved := re.search(r"^Domains: .+$", frame, re.M):
        print(saved.group())
    if f"\n{LEDGER_HEAD}\n" in frame:
        print(frame.split(f"\n{LEDGER_HEAD}\n", 1)[1].split("\n## ", 1)[0].strip())


def status(folder=None):
    if folder is None:
        folders = sorted(p for p in Path(git("rev-parse", "--show-toplevel"), "tmp", "flow").glob("2*/"))
        if not folders:
            raise SystemExit("no implement run folder")
        folder = folders[-1]
    print(folder)
    step_run.status(folder, graph(), rev)


def repo_profile():
    """The allowed domains and repo rules share one profile source."""
    profile = implement_home() / repo_name() / "profile.md"
    domains, repo_rules = {"any"}, []
    if profile.exists():
        text = read_utf8(profile)
        if m := re.search(r"^Domains:\s*(.+)$", text, re.M):
            domains |= {d.strip() for d in m.group(1).split(",") if d.strip()}
        if m := re.search(r"^Repo rules:\s*(.+)$", text, re.M):
            repo_rules = [r.strip() for r in m.group(1).split(",") if r.strip() and r.strip() != "none"]
    else:
        print(f"note: no profile at {profile}; the implement skill's start rule says what to do", file=sys.stderr)
    return domains, repo_rules


def skill_paths(step, domains=None):
    """The paths and block ids for the selected domains and repo rules."""
    key = step.split("-", 1)[1]
    allowed, repo_rules = repo_profile()
    domains = allowed if domains is None else domains
    items, section = [], None
    for line in read_utf8(implement_home() / "domains.md").splitlines():
        if line.startswith("## "):
            section = line[3:].strip()
        elif section in domains and line.startswith(f"- {key}:"):
            items += [p.strip() for p in line.split(":", 1)[1].split(",") if p.strip()]
    root = Path(git("rev-parse", "--show-toplevel"))
    paths = []
    installed = Path.home() / ".swarm" / "kit"
    for item in items:
        if item.startswith("block:"):
            continue
        path = Path(item).expanduser()
        if path.is_relative_to(installed):
            local = BLOCKS.parent / path.relative_to(installed)
            if local.is_file():
                item = str(local)
        if item not in paths:
            paths.append(item)
    return [kit_path(p) for p in paths + [str(root / r) for r in repo_rules]], [p.removeprefix("block:") for p in items if p.startswith("block:")]


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
    if step == "07-close":
        if Path(folder).resolve().parent.name == "_closed":
            raise SystemExit(f"{folder} is already closed")
        destination = Path(folder).resolve().parent / "_closed" / Path(folder).name
        if destination.exists() or destination.is_symlink():
            raise SystemExit(f"close destination already exists: {destination}; "
                             "rename the run folder to a distinct name, then run done <renamed-folder> 07-close")
    # Only 04-impact gets a Callers section; elsewhere that line is pasted text and must not end the list.
    ends = (RESULT_HEAD, CALLERS_HEAD) if step == "04-impact" else (RESULT_HEAD,)
    step_run.done(folder, step, rev(folder, step) if step == "05-build" else None, ends=ends)
    if step == "07-close":
        destination.parent.mkdir(parents=True, exist_ok=True)
        Path(folder).rename(destination)
        print(destination)


def skip(folder, step, *why):
    if step not in SKIPPABLE:
        raise SystemExit(f"{step} cannot be skipped; only {', '.join(SKIPPABLE)} can")
    step_run.skip(folder, step, *why)


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


def block(folder, step, *why):
    if step not in graph():
        raise SystemExit(f"unknown step {step}; steps are {', '.join(graph())}")
    step_run.block(folder, step, *why)


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
        path = kit_path(path)
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
    commands = {"start": start, "status": status, "take": take, "done": done, "skip": skip, "block": block}
    arity = {"start": (0, 0), "status": (0, 1), "take": (3, 3), "done": (2, 2), "skip": (3, None), "block": (3, None)}
    if len(sys.argv) < 2 or sys.argv[1] not in commands:
        raise SystemExit(__doc__)
    try:
        args = sys.argv[2:]
        command = sys.argv[1]
        force = command == "take" and "--force" in args
        if command == "take":
            args = [arg for arg in args if arg != "--force"]
        if command in ("done", "skip", "block") and "--force" in args:
            raise SystemExit("done <folder> <step> accepts no --force" if command == "done" else f"{command} accepts no --force")
        options = {"force": force} if command == "take" else {}
        if command == "start":
            while len(args) >= 2 and args[0] in ("--profile", "--domains") and args[0][2:] not in options:
                options[args[0][2:]] = args[1]
                args = args[2:]
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
