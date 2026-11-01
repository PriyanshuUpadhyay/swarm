"""A step cannot start or close on a stale input chain. Run: python3 test_step_run_chain.py (or through test_step_run.py)"""
import re, subprocess, sys, tempfile
from contextlib import redirect_stdout
from io import StringIO
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))  # python3 -I does not add the script's folder
import step_run  # noqa: E402

SCRIPT = Path(__file__).with_name("step_run.py")
TABLE = """# chain

| File | Needs | Holds |
|---|---|---|
| 01-a | none | a |
| 02-b | a | b |
| 03-c | b | c |
| 04-d | c | d |
"""


def cli(*args):
    return subprocess.run([sys.executable, "-I", SCRIPT, *map(str, args)], capture_output=True, text=True)


def chain_run(tmp):
    skill = Path(tmp, "SKILL.md")
    skill.write_text(TABLE, encoding="utf-8")
    folder = Path(tmp, "run")
    assert cli("start", folder, skill).returncode == 0
    return folder


def take(folder, step):
    out = cli("take", folder, step, "tester")
    assert out.returncode == 0, out.stderr
    return re.search(r"#(\d+)", out.stdout.split("\n", 1)[0])[1]


def check(folder, step):
    file = folder / f"{step}.md"
    text = file.read_text(encoding="utf-8")
    file.write_text(re.sub(r"^- \[ \] (.*): $", r"- [x] \1: seen", text, flags=re.M), encoding="utf-8")


def finish(folder, step):
    claim = take(folder, step)
    check(folder, step)
    out = cli("done", folder, step, claim)
    assert out.returncode == 0, out.stderr


def edit(folder, step):
    file = folder / f"{step}.md"
    file.write_text(file.read_text(encoding="utf-8") + "a later edit\n", encoding="utf-8")


def test_take_refuses_a_stale_needed_step(tmp):
    folder = chain_run(tmp)
    finish(folder, "01-a")
    finish(folder, "02-b")
    edit(folder, "01-a")
    out = cli("take", folder, "03-c", "tester")
    assert out.returncode == 1 and out.stderr.strip() == (
        "03-c is not ready: 02-b is stale (its input 01-a changed after it was done); take 02-b again"), out
    assert "03-c: open\n" in cli("status", folder).stdout, "a step on a stale chain is not shown as ready"
    finish(folder, "02-b")
    finish(folder, "03-c")


def test_take_refuses_a_stale_step_two_links_back(tmp):
    folder = chain_run(tmp)
    for step in ("01-a", "02-b", "03-c"):
        finish(folder, step)
    edit(folder, "01-a")
    out = cli("take", folder, "04-d", "tester")
    assert out.returncode == 1 and "02-b is stale (its input 01-a changed after it was done)" in out.stderr, out


def test_done_refuses_a_needed_step_that_went_stale(tmp):
    folder = chain_run(tmp)
    finish(folder, "01-a")
    finish(folder, "02-b")
    claim = take(folder, "03-c")
    check(folder, "03-c")
    edit(folder, "01-a")
    out = cli("done", folder, "03-c", claim)
    assert out.returncode == 1 and out.stderr.strip() == (
        "03-c is not done: its input 02-b is stale (its input 01-a changed after it was done); "
        "take 02-b again, then this step"), out


def test_a_skipped_step_ends_the_walk(tmp):
    folder = chain_run(tmp)
    assert cli("skip", folder, "01-a", "not needed").returncode == 0
    finish(folder, "02-b")
    finish(folder, "03-c")


def test_the_callers_revision_is_used(tmp):
    """implement passes its own revision (05-build is HEAD); the chain check must use it, not the file hash."""
    folder = chain_run(tmp)
    rev = lambda f, s: "head1234abcd" if s == "02-b" else step_run.revision(f, s)
    finish(folder, "01-a")
    with redirect_stdout(StringIO()):
        step_run.take(folder, "02-b", "tester", None, rev)
        check(folder, "02-b")
        step_run.done(folder, "02-b", 1, "head1234abcd", rev_of=rev)
        step_run.take(folder, "03-c", "tester", None, rev)
        check(folder, "03-c")
        step_run.done(folder, "03-c", 1, rev_of=rev)
    assert step_run.ready(folder, "04-d", rev=rev)
    assert not step_run.ready(folder, "04-d"), "the file hash of 02-b is not its revision, so 03-c reads as stale"


CASES = (test_take_refuses_a_stale_needed_step, test_take_refuses_a_stale_step_two_links_back,
         test_done_refuses_a_needed_step_that_went_stale, test_a_skipped_step_ends_the_walk, test_the_callers_revision_is_used)

if __name__ == "__main__":
    for case in CASES:
        with tempfile.TemporaryDirectory() as tmp:
            case(tmp)
    print("ok")
