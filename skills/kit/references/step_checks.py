"""The script checks that `step_run.py done` runs. `blocks/blocks.toml` owns the check-line format."""

import os
import re
import shlex
import signal
import subprocess
from pathlib import Path

KIT = Path(__file__).resolve().parents[1]
# The expectations that a `Check (script)` line may state after its command; any other wording needs a reader.
RUN_FORM = re.compile(r"`([^`]+)` (exits 0|prints nothing|prints `([^`]*)`|starts with `([^`]*)`):")
CHECK_TODO = re.compile(r"- \[x\] block ([\w.-]+) script: (.*)")
CHECK_SECONDS = 600
# start fills a {{param}} in the command span with shlex.quote, so there it is one shell word and never `; cmd`.
SLOT = "(?:[\\w@%+=:,./-]|'[^'`]*'|\"'\")+"


def work_root(folder):
    """The git top level of the step folder's piece, else the piece itself; start and done both use it."""
    piece = Path(folder).resolve().parent
    directory = next(path for path in (piece, *piece.parents) if path.is_dir())
    env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
    try:
        git = subprocess.run(["git", "-C", str(directory), "rev-parse", "--show-toplevel"],
                             env=env, capture_output=True, text=True, encoding="utf-8", errors="replace")
    except OSError:
        return piece, piece
    root = Path(git.stdout.strip()).resolve() if git.returncode == 0 else piece
    return (root if piece.is_relative_to(root) else piece), piece


def block_checks(block):
    """The block's own `Check (script)` lines, each with its pattern: a {{param}} in the command span matches
    one shell word, and one after it matches any text."""
    import block_resolver
    file = KIT / "blocks" / block / "SKILL.md"
    if not file.is_file():
        return None
    lines = block_resolver.check_lines(block_resolver.sections(file.read_text(encoding="utf-8")), "Check (script)")
    def pattern(text, slot):
        return slot.join(map(re.escape, re.split(r"\{\{\w+\}\}", text)))
    cut = [line.index("`", 1) + 1 if line.startswith("`") and "`" in line[1:] else 0 for line in lines]
    return [(line, re.compile(pattern(line[:c], SLOT) + pattern(line[c:], ".+?") + ":")) for line, c in zip(lines, cut)]


def plan_blocks(folder, step):
    """The resolved blocks of the step's row in the run's plan.md, for a plan with a Writes column; else none.

    start writes the rules and check todos of exactly these blocks, so done needs no `Skills read:` line to know
    them, and deleting that line skips no check.
    """
    plan = Path(folder) / "plan.md"
    if not plan.is_file():
        return set()
    import sys
    sys.path.insert(0, str(KIT / "blocks" / "plan-chart" / "scripts"))
    import block_resolver
    import plan_chart
    row = next((r for r in plan_chart.read_plan(plan)[2] if r[0] == step), None)
    if row is None or row[5] is None:
        return set()
    return set(block_resolver.resolve_blocks(None, {step: row[3]}, "default", blocks_dir=KIT / "blocks", pack=False)[step])


def run_checks(folder, step, todo, text):
    """Run each checked `block <id> script:` todo that is in the run form; the worker's evidence never runs."""
    from step_run import log, plain
    folder = Path(folder)
    root, _ = work_root(folder)
    places = {"<kit root>": KIT, "<run>": folder.resolve(), "<repo-root>": root}
    found = [m.groups() for m in map(CHECK_TODO.match, todo.splitlines()) if m]
    read = next((l for l in text.splitlines() if l.startswith("Skills read: ")), "")
    for block in sorted(set(re.findall(r"\bblock:([\w.-]+)@", read)) | plan_blocks(folder, step)):
        for line, p in block_checks(block) or []:
            if not any(b == block and p.match(t) for b, t in found):
                raise SystemExit(f"{step} is not done: a blocks/{block} script check has no todo: "
                                 f"{line[:120]}; restore the line that start wrote, or start a new run folder if the block changed")
    for line in todo.splitlines():
        found = CHECK_TODO.match(line)
        if not found:
            continue
        block, text = found.groups()
        checks = block_checks(block)
        if checks is None:
            print(f"not run: block {block}: no kit file blocks/{block}/SKILL.md")
            continue
        source = next((l for l, p in checks if p.match(text)), None)
        if source is None:
            raise SystemExit(f"{step} is not done: the check text differs from blocks/{block} (an edited todo, or a block "
                             f"that changed after start): {plain(text[:200])}")
        form = RUN_FORM.match(text)
        if not form:
            print(f"not run: block {block}: its expectation needs a reader")
            continue
        command = form[1]
        for word, value in places.items():
            command = command.replace(word, shlex.quote(str(value)))
        # The block's own line decides a missing word, so a worker's slot value cannot turn a check off.
        if left := next((w for w in re.findall(r"<[\w -]+>", source) if w not in places), None):
            print(f"not run: block {block}: needs {left}")
            continue
        # A new session makes the check its own process group, so a timeout also stops a pipeline's children.
        child = subprocess.Popen(["sh", "-c", command], cwd=root, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                 stderr=subprocess.PIPE, text=True, encoding="utf-8", errors="replace",
                                 start_new_session=True)
        try:
            stdout, stderr = child.communicate(timeout=CHECK_SECONDS)
            code, tail = child.returncode, stdout + stderr
        except subprocess.TimeoutExpired:
            code, stdout, stderr, tail = "timeout", "", "", f"no result in {CHECK_SECONDS} s"
        finally:
            # Ctrl-C does not reach the new session, and a job the check put in the background outlives sh,
            # so stop the whole group on every way out.
            try:
                os.killpg(child.pid, signal.SIGKILL)
            except (ProcessLookupError, PermissionError):
                pass  # the group is gone, or macOS refuses a group whose leader is ending
            try:
                child.communicate(timeout=5)
            except (subprocess.TimeoutExpired, ValueError):
                pass  # a grandchild that left the group still holds the pipe; give up on its output
        lines = [l.strip() for l in stdout.splitlines() if l.strip()]
        expect = form[2]
        passed = code == 0 and (
            expect == "exits 0"
            or form[3] is not None and lines[-1:] == [form[3]]
            or form[4] is not None and stdout.startswith(form[4])
        # rg exits 1 for no match; a crash also exits 1, but it writes to stderr.
        ) or expect == "prints nothing" and not stdout.strip() and (code == 0 or code == 1 and not stderr.strip())
        log(folder, step, f"check {block} exit {code}")
        if not passed:
            last = "\n".join(plain(tail).splitlines()[-5:])
            raise SystemExit(f"{step} is not done: block {block} check failed (exit {code}, expected: {expect}):\n"
                             f"{plain(command)}\n{last}")
