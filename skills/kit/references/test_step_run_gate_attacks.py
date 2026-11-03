# Step runner tests for the done gate: edited todos, odd claims and logs, and checks that hang or leave jobs.
import re
import shlex

import step_checks
from test_step_run_gate import CANVAS, gate_run, take
from test_step_run_support import *


def test_check_slots_and_todos(tmp):
    """A {{param}} slot in the command holds one shell word, and a block named in the step keeps its check todo."""
    marker = Path(tmp, "ran")
    line = CANVAS.format(site=f"/x; touch {shlex.quote(str(marker))}; true")
    folder = gate_run(tmp, "slot", [f"block build-canvas script: {line}"])
    assert run("done", folder, "01-ask", take(folder, "01-ask")).returncode == 0
    out = run("done", folder, "02-work", take(folder, "02-work"))
    assert out.returncode == 1 and "the check text differs from blocks/build-canvas" in out.stderr, out
    assert not marker.exists(), "a slot must not carry a second command"

    work = gate_run(tmp, "dropped", [])
    file = work / "02-work.md"
    text = file.read_text(encoding="utf-8")
    file.write_text(text.replace(step_run.TODO_MARK, "Skills read: block:build-canvas@0\n\n" + step_run.TODO_MARK, 1),
                    encoding="utf-8")
    assert run("done", work, "01-ask", take(work, "01-ask")).returncode == 0
    out = run("done", work, "02-work", take(work, "02-work"))
    assert out.returncode == 1 and "a blocks/build-canvas script check has no todo" in out.stderr, out

    # Worker text cannot hide a check, cut the command with a backtick, or write ESC to the refusal.
    for name, site, shown in (("angle", "'<x>'", "block build-canvas check failed"),
                              ("tick", "'a` exits 0: b'", "the check text differs"),
                              ("esc", "'\x1b[2J'; true", "the check text differs")):
        folder = gate_run(tmp, name, [f"block build-canvas script: {CANVAS.format(site=site)}"])
        assert run("done", folder, "01-ask", take(folder, "01-ask")).returncode == 0
        out = run("done", folder, "02-work", take(folder, "02-work"))
        assert out.returncode == 1 and shown in out.stderr and "\x1b" not in out.stderr, (name, out)


def test_done_sees_an_edit_during_checks(tmp):
    """An edit to the step file while its checks run refuses, so done never writes over it."""
    folder = gate_run(tmp, "edit", [])
    file = folder / "02-work.md"
    command = f"echo late >> {shlex.quote(str(file))}"
    text = file.read_text(encoding="utf-8")
    file.write_text(text.replace(f"\n{step_run.RESULT_HEAD}", f"- [x] block fake script: `{command}` exits 0: x: ok\n"
                                 f"\n{step_run.RESULT_HEAD}", 1), encoding="utf-8")
    assert run("done", folder, "01-ask", take(folder, "01-ask")).returncode == 0
    held = take(folder, "02-work")
    with patch.object(step_checks, "block_checks", lambda block: [("`x` exits 0: x", re.compile(".*"))]):
        try:
            with redirect_stdout(StringIO()):
                step_run.done(folder, "02-work", held)
            raise AssertionError("done must refuse an edit made during its checks")
        except SystemExit as error:
            assert "changed while its checks ran" in str(error.code), error.code
    assert file.read_text(encoding="utf-8").endswith("late\n"), "the edit stays"

    # A check that edits an input file leaves the step stale, so done does not close on the old input.
    folder = gate_run(tmp, "input", [])
    ask, file = folder / "01-ask.md", folder / "02-work.md"
    command = f"echo late >> {shlex.quote(str(ask))}"
    text = file.read_text(encoding="utf-8")
    file.write_text(text.replace(f"\n{step_run.RESULT_HEAD}", f"- [x] block fake script: `{command}` exits 0: x: ok\n"
                                 f"\n{step_run.RESULT_HEAD}", 1), encoding="utf-8")
    assert run("done", folder, "01-ask", take(folder, "01-ask")).returncode == 0
    held = take(folder, "02-work")
    with patch.object(step_checks, "block_checks", lambda block: [("`x` exits 0: x", re.compile(".*"))]):
        try:
            with redirect_stdout(StringIO()):
                step_run.done(folder, "02-work", held)
            raise AssertionError("done must refuse an input changed during its checks")
        except SystemExit as error:
            assert "input changed while its checks ran: 01-ask" in str(error.code), error.code


def test_done_refuses_a_written_back_claim(tmp):
    """A replaced worker that writes back its old copy of the step file still holds an old claim."""
    folder = gate_run(tmp, "copy", [])
    take(folder, "01-ask", "a")
    old = (folder / "01-ask.md").read_text(encoding="utf-8")
    assert run("take", folder, "01-ask", "b", "--force").stdout.startswith("Status: active b #2")
    (folder / "01-ask.md").write_text(old, encoding="utf-8")
    out = run("done", folder, "01-ask", "1")
    assert out.returncode == 1 and "claim #1 is not the current one" in out.stderr, out


def test_take_refuses_a_name_that_forges_a_line(tmp):
    """A newline or tab in the agent name would add a false take row to events.log (trust boundary)."""
    folder = gate_run(tmp, "forge", [])
    for name in ("w\n2026-10-21T00:00:00\t02-work\ttake x", "w\tx"):
        assert run("take", folder, "01-ask", name).returncode == 1, name
    assert not (folder / "events.log").exists() or "take" not in (folder / "events.log").read_text(encoding="utf-8")
