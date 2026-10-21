"""The script checks that `step_run.py done` runs. `blocks/blocks.toml` owns the check-line format."""

import os
import re
import shlex
import subprocess
from pathlib import Path

KIT = Path(__file__).resolve().parents[1]
# The expectations that a `Check (script)` line may state after its command; any other wording needs a reader.
RUN_FORM = re.compile(r"`([^`]+)` (exits 0|prints nothing|prints `([^`]*)`|starts with `([^`]*)`):")
CHECK_TODO = re.compile(r"- \[x\] block ([\w-]+) script: (.*)")
CHECK_SECONDS = 600


def block_checks(block):
    """The block's own `Check (script)` lines as patterns, each {{param}} matching any filled value."""
    import block_resolver
    file = KIT / "blocks" / block / "SKILL.md"
    if not file.is_file():
        return None
    lines = block_resolver.check_lines(block_resolver.sections(file.read_text(encoding="utf-8")), "Check (script)")
    return [re.compile(".+?".join(map(re.escape, re.split(r"\{\{\w+\}\}", line))) + ":") for line in lines]


def run_checks(folder, step, todo):
    """Run each checked `block <id> script:` todo that is in the run form; the worker's evidence never runs."""
    from step_run import log, plain
    folder = Path(folder)
    git = subprocess.run(["git", "-C", str(folder), "rev-parse", "--show-toplevel"], capture_output=True, text=True,
                         env={k: v for k, v in os.environ.items() if not k.startswith("GIT_")})
    root = Path(git.stdout.strip()) if git.returncode == 0 else folder
    places = {"<kit root>": KIT, "<run>": folder.resolve(), "<repo-root>": root}
    for line in todo.splitlines():
        found = CHECK_TODO.match(line)
        if not found:
            continue
        block, text = found.groups()
        patterns = block_checks(block)
        if patterns is None:
            print(f"not run: block {block}: no kit file blocks/{block}/SKILL.md")
            continue
        if not any(p.match(text) for p in patterns):
            raise SystemExit(f"{step} is not done: the check text differs from blocks/{block}: {text[:200]}")
        form = RUN_FORM.match(text)
        if not form:
            print(f"not run: block {block}: its expectation needs a reader")
            continue
        command = form[1]
        for word, value in places.items():
            command = command.replace(word, shlex.quote(str(value)))
        if left := re.search(r"<[\w -]+>", command):
            print(f"not run: block {block}: needs {left[0]}")
            continue
        try:
            out = subprocess.run(["sh", "-c", command], cwd=root, capture_output=True, text=True,
                                 timeout=CHECK_SECONDS, stdin=subprocess.DEVNULL)
            code, stdout, tail = out.returncode, out.stdout, out.stdout + out.stderr
        except subprocess.TimeoutExpired:
            code, stdout, tail = "timeout", "", f"no result in {CHECK_SECONDS} s"
        lines = [l.strip() for l in stdout.splitlines() if l.strip()]
        expect = form[2]
        passed = code == 0 and (
            expect == "exits 0"
            or form[3] is not None and lines[-1:] == [form[3]]
            or form[4] is not None and stdout.startswith(form[4])
        ) or expect == "prints nothing" and code in (0, 1) and not stdout.strip()
        log(folder, step, f"check {block} exit {code}")
        if not passed:
            last = "\n".join(plain(tail).splitlines()[-5:])
            raise SystemExit(f"{step} is not done: block {block} check failed (exit {code}, expected: {expect}):\n"
                             f"{command}\n{last}")
