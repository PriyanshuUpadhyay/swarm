# Step runner test support cases and helpers.
import fcntl, os, subprocess, sys, tempfile, time

from contextlib import redirect_stdout

from io import StringIO

from pathlib import Path

from unittest.mock import patch

import step_run  # noqa: E402


SCRIPT = Path(__file__).with_name("step_run.py")
run = lambda *a: subprocess.run([sys.executable, SCRIPT, *map(str, a)], capture_output=True, text=True)


def lock_holder(folder, step):
    code = """import sys
sys.path.insert(0, sys.argv[1])
import step_run
with step_run.step_lock(sys.argv[2], sys.argv[3]):
    print("held", flush=True)
    sys.stdin.read()
"""
    child = subprocess.Popen([sys.executable, "-c", code, str(SCRIPT.parent), str(folder), step],
                             stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    held = False
    try:
        held = child.stdout.readline() == "held\n"
    finally:
        if not held:
            child.kill()
            _, stderr = child.communicate(timeout=5)
    if not held:
        raise AssertionError(stderr)
    return child


def claim(folder, step):
    """The claim number on the step's status line, or `#0` when no one holds it."""
    state = (Path(folder) / f"{step}.md").read_text(encoding="utf-8").split("\n", 1)[0]
    return f"#{state.rsplit(' #', 1)[1]}" if state.startswith("Status: active ") and " #" in state else "#0"
