"""A step cannot start or close on a stale input chain. Run: python3 test_step_run_chain.py (or through test_step_run.py)"""
import re, sys, tempfile
from contextlib import redirect_stdout
from io import StringIO
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))  # python3 -I does not add the script's folder
import step_run  # noqa: E402
from test_step_run_gate import take  # noqa: E402
from test_step_run_support import run  # noqa: E402

TABLE = """# chain

| File | Needs | Holds |
|---|---|---|
| 01-source | none | source |
| 02-build | source | build |
| 03-review | build | review |
| 04-ship | review | ship |
"""


def chain_run(tmp):
    skill = Path(tmp, "SKILL.md")
    skill.write_text(TABLE, encoding="utf-8")
    folder = Path(tmp, "run")
    assert run("start", folder, skill).returncode == 0
    return folder


def check(folder, step):
    file = folder / f"{step}.md"
    text = file.read_text(encoding="utf-8")
    file.write_text(re.sub(r"^- \[ \] (.*): $", r"- [x] \1: seen", text, flags=re.M), encoding="utf-8")


def finish(folder, step):
    claim = take(folder, step)
    check(folder, step)
    out = run("done", folder, step, claim)
    assert out.returncode == 0, out.stderr


def edit(folder, step):
    file = folder / f"{step}.md"
    file.write_text(file.read_text(encoding="utf-8") + "a later edit\n", encoding="utf-8")


def test_take_refuses_a_stale_needed_step(tmp):
    folder = chain_run(tmp)
    finish(folder, "01-source")
    finish(folder, "02-build")
    edit(folder, "01-source")
    out = run("take", folder, "03-review", "tester")
    assert out.returncode == 1 and out.stderr.strip() == (
        "03-review is not ready: 02-build is stale (its input 01-source changed after it was done); take 02-build again"), out
    assert "03-review: open\n" in run("status", folder).stdout, "a step on a stale chain is not shown as ready"
    finish(folder, "02-build")
    finish(folder, "03-review")


def test_take_refuses_a_stale_step_two_links_back(tmp):
    folder = chain_run(tmp)
    for step in ("01-source", "02-build", "03-review"):
        finish(folder, step)
    edit(folder, "01-source")
    out = run("take", folder, "04-ship", "tester")
    assert out.returncode == 1 and "02-build is stale (its input 01-source changed after it was done)" in out.stderr, out


def test_done_refuses_a_needed_step_that_went_stale(tmp):
    folder = chain_run(tmp)
    finish(folder, "01-source")
    finish(folder, "02-build")
    claim = take(folder, "03-review")
    check(folder, "03-review")
    edit(folder, "01-source")
    out = run("done", folder, "03-review", claim)
    assert out.returncode == 1 and out.stderr.strip() == (
        "03-review is not done: 02-build, on its input chain, is stale (its input 01-source changed after it was done); "
        "take 02-build again, then this step"), out


def test_a_skipped_step_ends_the_walk(tmp):
    folder = chain_run(tmp)
    assert run("skip", folder, "01-source", "not needed").returncode == 0
    finish(folder, "02-build")
    finish(folder, "03-review")


def test_the_callers_revision_is_used(tmp):
    """implement passes its own revision (05-build is HEAD); the chain check must use it, not the file hash."""
    folder = chain_run(tmp)
    rev = lambda f, s: "head1234abcd" if s == "02-build" else step_run.revision(f, s)
    finish(folder, "01-source")
    with redirect_stdout(StringIO()):
        step_run.take(folder, "02-build", "tester", None, rev)
        check(folder, "02-build")
        step_run.done(folder, "02-build", 1, "head1234abcd", rev_of=rev)
        step_run.take(folder, "03-review", "tester", None, rev)
        check(folder, "03-review")
        step_run.done(folder, "03-review", 1, rev_of=rev)
    assert step_run.ready(folder, "04-ship", rev=rev)
    with redirect_stdout(StringIO()):  # done checks the chain again, with the same revision
        step_run.take(folder, "04-ship", "tester", None, rev)
        check(folder, "04-ship")
        step_run.done(folder, "04-ship", 1, rev_of=rev)
    assert not step_run.ready(folder, "04-ship"), "the file hash of 02-build is not its revision, so 03-review reads as stale"


def test_done_checks_the_chain_again_after_its_checks(tmp):
    """A step further back that changes while done runs its checks still refuses, as a direct input does."""
    folder = chain_run(tmp)
    finish(folder, "01-source")
    finish(folder, "02-build")
    claim = take(folder, "03-review")
    check(folder, "03-review")
    real = step_run.run_checks

    def edit_during_checks(*args):
        edit(folder, "01-source")
        return real(*args)

    with patch.object(step_run, "run_checks", edit_during_checks), redirect_stdout(StringIO()):
        try:
            step_run.done(folder, "03-review", claim)
            raise AssertionError("done wrote done on a stale chain")
        except SystemExit as stop:
            assert "03-review is not done: 02-build, on its input chain, is stale" in str(stop), stop
    assert step_run.status_of(folder, "03-review").startswith("active ")


def test_the_message_names_a_step_that_can_be_taken(tmp):
    """With 02-build and 03-review both stale, take 03-review would refuse, so the message names 02-build."""
    folder = chain_run(tmp)
    for step in ("01-source", "02-build", "03-review"):
        finish(folder, step)
    edit(folder, "02-build")
    edit(folder, "01-source")
    out = run("take", folder, "04-ship", "tester")
    assert out.returncode == 1 and out.stderr.strip() == (
        "04-ship is not ready: 02-build is stale (its input 01-source changed after it was done); take 02-build again"), out
    assert run("take", folder, "02-build", "tester").returncode == 0


def test_a_skipped_step_taken_before_its_input_changed_ends_the_walk(tmp):
    folder = chain_run(tmp)
    finish(folder, "01-source")
    take(folder, "02-build")
    edit(folder, "01-source")
    assert step_run.stale(folder, "02-build"), "02-build holds an old revision of 01-source"
    assert run("skip", folder, "02-build", "not needed").returncode == 0
    finish(folder, "03-review")


def test_the_message_waits_for_a_step_taken_again(tmp):
    """01-source is taken again, so 02-build cannot be taken yet; the message says to finish 01-source first."""
    folder = chain_run(tmp)
    for step in ("01-source", "02-build"):
        finish(folder, step)
    take(folder, "01-source")
    edit(folder, "01-source")
    out = run("take", folder, "03-review", "tester")
    assert out.returncode == 1 and out.stderr.strip() == (
        "03-review is not ready: 02-build is stale (its input 01-source changed after it was done); finish 01-source, then take 02-build again"), out



def test_a_uses_name_with_no_file_refuses_with_a_reason(tmp):
    """A hand-written Uses line can name a step with no file; take refuses with a reason, not a traceback."""
    folder = chain_run(tmp)
    for step in ("01-source", "02-build"):
        finish(folder, step)
    build = folder / "02-build.md"
    text = build.read_text(encoding="utf-8")
    build.write_text(re.sub(r"^Uses: .*$", lambda m: m[0] + ", ghost@0123456789ab", text, count=1, flags=re.M), encoding="utf-8")
    out = run("take", folder, "03-review", "tester")
    assert out.returncode == 1 and "Traceback" not in out.stderr, out.stderr
    assert "03-review is not ready: 02-build is stale (its input ghost changed after it was done)" in out.stderr, out.stderr

CASES = (test_take_refuses_a_stale_needed_step, test_take_refuses_a_stale_step_two_links_back,
         test_done_refuses_a_needed_step_that_went_stale, test_a_skipped_step_ends_the_walk, test_the_callers_revision_is_used,
         test_done_checks_the_chain_again_after_its_checks, test_the_message_names_a_step_that_can_be_taken,
         test_a_skipped_step_taken_before_its_input_changed_ends_the_walk, test_the_message_waits_for_a_step_taken_again,
         test_a_uses_name_with_no_file_refuses_with_a_reason)

if __name__ == "__main__":
    for case in CASES:
        with tempfile.TemporaryDirectory() as tmp:
            case(tmp)
    print("ok")
