#!/usr/bin/env python3
"""Make and take implement steps. Run from inside the repo.

  start [--profile <name>] [--domains <a,b>] [--parent-loop KEY] [--plan <run>] make tmp/flow/<date>-<branch>/
  review <folder> <local|base..head> [--continue FILE] start the review check with this run's saved key
  status [<folder>]            print each step's status, whether it is ready, and whether it is stale
  take <folder> <step> <who> [--force] mark the step active and print its file with the skills the catalog gives it
  done <folder> <step> <n>     set done for take's claim #<n>, with evidence; 07-close moves the run to tmp/flow/_closed/
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
07-close needs the check-mode run's own APPROVE verdict on HEAD. `start --plan <run>` refuses a plan that
the plan skill's stop rule (blocks/plan-review/scripts/start_plan.py) does not pass, or whose steps or Needs
differ from the step graph. A plan that starts is named in 01-frame.md.
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
sys.path.insert(0, str(Path(__file__).resolve().parent))
from implement_context import *  # noqa: E402,F403
import step_run  # noqa: E402
from step_run import RESULT_HEAD, TODO_HEAD, kit_path, plain, status_of  # noqa: E402
from block_resolver import BLOCKS, CAP, kit_profile, one_line, read_toml, read_utf8, resolve_blocks, sections  # noqa: E402


def rev(folder, step):
    """05-build's revision is the commit it built, so a new commit makes its review stale."""
    return git("rev-parse", "HEAD")[:12] if step == "05-build" else step_run.revision(folder, step)


def review(folder, target, *opts):
    script = BLOCKS.parent / "skills" / "review" / "scripts" / "review_check.py"
    result = subprocess.run([sys.executable, str(script), "start", target, "--parent-loop", loop_key(folder), *opts])
    if result.returncode:
        raise SystemExit(result.returncode)


def check_graph(plan, rows):
    """Refuse an approved plan whose steps or Needs differ from graph(), so no approved step is dropped in silence."""
    # need_files reads the word after NN-, so a name without it is reported here and kept out of the Needs read.
    odd = [r[0] for r in rows if not re.fullmatch(r"\d\d-[^-\s][^\s]*", r[0])]
    names = [r[0] for r in rows if r[0] not in odd]
    planned = {r[0]: step_run.need_files(r[1], names) for r in rows if r[0] not in odd}
    ours = graph()
    diff = [f"{s or '(empty)'} is not NN-name" for s in odd]
    diff += [f"{s} twice" for s in dict.fromkeys(s for s in names if names.count(s) > 1)]
    diff += [f"added {s}" for s in planned if s not in ours] + [f"missing {s}" for s in ours if s not in planned]
    diff += [f"{s} needs {', '.join(ours[s]) or 'none'} in implement, {', '.join(planned[s]) or 'none'} in plan"
             for s in ours if s in planned and sorted(planned[s]) != sorted(ours[s])]
    if diff:
        raise SystemExit(one_line(
            f"{plan}/plan.md steps differ from implement's graph: {'; '.join(diff)}. Implement runs its own steps: "
            f"skip {', '.join(SKIPPABLE[:-1])} or {SKIPPABLE[-1]} after 01-frame, and put other work in a plan for "
            "another verb. Edit plan.md and approve it in a new round."))


def start(profile="default", domains=None, parent_loop=None, plan=None):
    if parent_loop is not None:
        check_loop_key(parent_loop)
    if plan is not None:
        sys.path.insert(0, str(BLOCKS / "plan-review" / "scripts"))
        import contextlib
        import start_plan
        if start_plan.preset.of_plan(Path(plan) / "plan.md") != SKILL_MD:
            raise SystemExit(f"{plan}/plan.md needs `Preset: implement` on the line under its title")
        with contextlib.redirect_stdout(sys.stderr):  # the folder stays the first stdout line
            start_plan.approved(plan)
        plan = Path(plan).resolve()
        rows = start_plan.preset.plan_chart.read_plan(plan / "plan.md")[2]
        check_graph(plan, rows)
        plan_blocks = {r[0]: r[3] for r in rows}
        # step_run's scope, review, and skip rules read these lines, so a plan with Writes keeps them in implement.
        plan_scope = {r[0]: (f"Writes: {', '.join(r[5]) or 'none'}\n" + ("Optional: yes\n" if r[6] else ""))
                      for r in rows if r[5] is not None}
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
            extra += f"Plan: {plan}\n" if step == "01-frame" and plan else ""
            extra += f"Plan blocks: {', '.join(plan_blocks[step])}\n" if plan and plan_blocks.get(step) else ""
            extra += plan_scope.get(step, "") if plan else ""
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
    loop_key(folder, parent_loop)
    print(folder)
    print(f"Each step file holds its rules, a todo list, and a result section. Close a step with: python3 {kit_path(Path(__file__).resolve())} done <folder> <step> <n>")
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


def take(folder, step, who, force=False):
    need = graph().get(step)
    if need is None:
        raise SystemExit(f"unknown step {step}; steps are {', '.join(graph())}")
    if not step_run.ready(folder, step, need):
        waiting = [u for u in need if status_of(folder, u).split()[0] not in ("done", "skipped")]
        raise SystemExit(f"{step} is not ready; it needs {', '.join(waiting)}")
    if step == "07-close":
        approved(folder)
    if step == "05-build":
        check_mode(folder)
    step_run.take(folder, step, who, need, rev, force=force,
                  on_claim=write_callers if step == "04-impact" else None)


def done(folder, step, claim):
    if step == "07-close":
        if Path(folder).resolve().parent.name == "_closed":
            raise SystemExit(f"{folder} is already closed")
        destination = Path(folder).resolve().parent / "_closed" / Path(folder).name
        if destination.exists() or destination.is_symlink():
            raise SystemExit(f"close destination already exists: {destination}; "
                             "rename the run folder to a distinct name, then run done <renamed-folder> 07-close <n>")
    # Only 04-impact gets a Callers section; elsewhere that line is pasted text and must not end the list.
    ends = (RESULT_HEAD, CALLERS_HEAD) if step == "04-impact" else (RESULT_HEAD,)
    step_run.done(folder, step, claim, rev(folder, step) if step == "05-build" else None, ends=ends, rev_of=rev)
    if step == "07-close":
        destination.parent.mkdir(parents=True, exist_ok=True)
        Path(folder).rename(destination)
        print(destination)


def skip(folder, step, *why):
    if step not in SKIPPABLE:
        raise SystemExit(f"{step} cannot be skipped; only {', '.join(SKIPPABLE)} can")
    step_run.skip(folder, step, *why)


def approved(folder):
    """Refuse 07-close unless 06-review names a check-mode run whose verdict is APPROVE on HEAD.
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
                         "run the review check on the new range")


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


if __name__ == "__main__":
    if hasattr(sys.stdout, "reconfigure"):
        # Escape non-ASCII output when the console uses an ASCII encoding.
        sys.stdout.reconfigure(errors="backslashreplace")
    commands = {"start": start, "review": review, "status": status, "take": take, "done": done, "skip": skip, "block": block}
    arity = {"start": (0, 0), "review": (2, None), "status": (0, 1), "take": (3, 3), "done": (3, 3), "skip": (3, None), "block": (3, None)}
    if len(sys.argv) < 2 or sys.argv[1] not in commands:
        raise SystemExit(__doc__)
    try:
        args = sys.argv[2:]
        command = sys.argv[1]
        force = command == "take" and "--force" in args
        if command == "take":
            args = [arg for arg in args if arg != "--force"]
        if command in ("done", "skip", "block") and "--force" in args:
            raise SystemExit("done <folder> <step> <n> accepts no --force" if command == "done" else f"{command} accepts no --force")
        options = {"force": force} if command == "take" else {}
        if command == "start":
            while len(args) >= 2 and args[0] in ("--profile", "--domains", "--parent-loop", "--plan") and args[0][2:].replace("-", "_") not in options:
                options[args[0][2:].replace("-", "_")] = args[1]
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
