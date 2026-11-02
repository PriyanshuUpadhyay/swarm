# Shared repository setup, review rows, loop helpers, and process helpers.
import json, os, runpy, shutil, signal, subprocess, sys, tempfile, time
from contextlib import contextmanager, redirect_stdout
from io import BytesIO, StringIO, TextIOWrapper
from pathlib import Path
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import review_check as rc


def sh(*cmd):
    return subprocess.run(cmd, check=True, capture_output=True, text=True).stdout.strip()



def commit(files, msg):
    for name, text in files.items():
        if text is None:
            Path(name).unlink()
        else:
            Path(name).parent.mkdir(parents=True, exist_ok=True)
            Path(name).write_text(text)
    sh("git", "add", "-A")
    sh("git", "commit", "-qm", msg)
    return sh("git", "rev-parse", "HEAD")



def rows(run, *lines):
    checklist = load_checklist(run)
    files = {"02-review.md": []}
    for line in lines:
        cells = [cell.strip() for cell in line.split("|")[1:-1]]
        uid, rules = cells[:2]
        owned = checklist[uid].get("owners", {})
        groups = {}
        for rule in rules.split(","):
            owner = owned.get(rule.strip().strip("`"))
            name = f"02-review-{owner}.md" if owner else "02-review.md"
            groups.setdefault(name, []).append(rule.strip())
        for name, ids in groups.items():
            files.setdefault(name, []).append("| " + " | ".join([uid, ", ".join(ids), *cells[2:]]) + " |")
    for name, values in files.items():
        (run / name).write_text("Status: done x\nUses: 01-units@x\n\n" + "".join(l + "\n" for l in values))



def load_checklist(run):
    value = json.loads((run / "checklist.json").read_text(encoding="utf-8"))
    value.pop("seats", None)
    return value



@contextmanager
def loop_repo():
    previous = Path.cwd()
    with tempfile.TemporaryDirectory() as folder:
        try:
            os.chdir(folder)
            sh("git", "init", "-qb", "main")
            sh("git", "config", "user.email", "t@t")
            sh("git", "config", "user.name", "t")
            base = commit({"value.py": "def value():\n    return 0\n"}, "base")
            head = commit({"value.py": "def value():\n    return 1\n"}, "change")
            with patch.object(rc, "load_rules", return_value=[]):
                yield base, head
        finally:
            os.chdir(previous)



def loop_round(base, head, key=None, kind="fix", quote="return 1", *options):
    opts = ("--parent-loop", key) if key else ()
    with redirect_stdout(StringIO()) as out:
        rc.start(f"{base}..{head}", *opts, *options)
    run = max(Path("tmp/review-check").glob("*/target.json"), key=lambda p: p.stat().st_mtime_ns).parent
    assert "parent loop:" in out.getvalue(), out.getvalue()
    unit = json.loads((run / "units.json").read_text())[0]
    rows(run, f"| {unit['id']} | - | value.py:{unit['range'][1]} | `{quote}` | {kind} | value issue | caller needs a different result |")
    assert rc.verdict(run.name) == 0
    return run



def loop_refused(base, head, key, reason, *options):
    before = {p: p.read_bytes() for p in Path("tmp/review-check").glob("*/03-verdict.md")}
    try:
        rc.start(f"{base}..{head}", "--parent-loop", key, *options)
    except SystemExit as error:
        assert reason in str(error) and "rounds used" in str(error) and f"continue: {key}" in str(error), str(error)
    else:
        raise AssertionError(f"start accepted {reason}")
    assert before == {p: p.read_bytes() for p in before}, "refusal changed a verdict"



def wait_pid(pidfile, process, message):
    deadline = time.monotonic() + 5
    while True:
        text = pidfile.read_text(encoding="utf-8").strip() if pidfile.exists() else ""
        if text:
            return int(text)
        assert process.poll() is None and time.monotonic() < deadline, message
        time.sleep(0.01)



def kill_pid(pidfile, pid=None, *, group=False):
    if pid is None:
        text = pidfile.read_text(encoding="utf-8").strip() if pidfile.exists() else ""
        pid = int(text) if text else None
    if pid is not None:
        try:
            (os.killpg if group else os.kill)(pid, signal.SIGKILL)
        except ProcessLookupError:
            pass

