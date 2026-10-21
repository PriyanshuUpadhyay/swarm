# Implement graph, profile, loop key, and skill text helpers.
import json
import re
import subprocess
import sys
import uuid
from pathlib import Path

import step_run
from step_run import kit_path, plain
from block_resolver import BLOCKS, CAP, read_utf8

SKILL_MD = Path(__file__).resolve().parents[1] / "SKILL.md"


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
    return {name: step_run.need_files(need, names) for name, need, *_ in rows}


def check_loop_key(key):
    if not isinstance(key, str) or not key.strip() or any(ord(c) < 32 or ord(c) == 127 for c in key):
        raise SystemExit("parent loop needs a nonempty key with no control characters")


def loop_key(folder, inherited=None):
    file = Path(folder) / "loop.json"
    with step_run.step_lock(folder, "loop", command="loop key"):
        key = json.loads(read_utf8(file))["parent_loop"] if file.exists() else inherited or f"implement:{uuid.uuid4()}"
        if inherited is not None and inherited != key:
            raise SystemExit(f"{folder} already uses parent loop {key}")
        check_loop_key(key)
        if not file.exists():
            step_run.write_step(file, json.dumps({"parent_loop": key}, indent=2) + "\n")
    return key


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
                 "Lane, fast or full, and why (implement)",
                 "A `Mode:` line, pair or auto; auto quotes the owner's words (implement)"],
    "02-design": ["UX flow and screens, or the reason no UI changes (implement)"],
    "03-contracts": ["APIs, data model, and integrations that change (implement)",
                     "Standards rows that apply (engineering-standards)", "Decision check for the topic (decisions)"],
    "04-impact": ["Existing features that change, judged from the Callers section and the contracts, or independent (implement)"],
    "05-build": ["Task file path and Base commit (pair or auto mode)",
                 "Failing check first: command and its failing output (prove-it-works)",
                 "Standards rows for each behavior, or none and why (engineering-standards)",
                 "Scope: each file touched and the part of the change that names it (AGENTS.md)",
                 "Dead code: what this change made unused, removed; older dead code, named (AGENTS.md)",
                 "Check passes: command and its passing output (prove-it-works)"],
    "06-review": ["review check range, verdict, and each fix row with its fix or reason (review check mode)",
                  "A `Run: tmp/review-check/<run>` line under Result for the last run (implement)"],
    "07-close": ["Each done-when of 01-frame with its evidence (prove-it-works)"],
}


def check_mode(folder):
    """Refuse auto mode unless 01-frame.md quotes the owner choosing it; no Mode line means pair.

    Any line whose key reads as mode, in any case or after a list mark, counts, so an odd line refuses.
    """
    lines = [l for l in read_utf8(Path(folder) / "01-frame.md").splitlines() if re.match(r"\W*mode\s*:", l, re.I)]
    if lines and (len(lines) > 1 or not re.fullmatch(r'Mode: (pair|auto, owner said: ".*\S.*")', lines[0])):
        raise SystemExit('05-build needs at most one line `Mode: pair` or `Mode: auto, owner said: "<the owner\'s words>"` '
                         f"in 01-frame.md, not {' / '.join(lines)}; the owner chooses auto, never the agent")


