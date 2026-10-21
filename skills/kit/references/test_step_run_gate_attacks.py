# Step runner tests for the done gate: the ways round 1 of review found to pass or crash it.
import re
import shlex
import shutil

import step_checks
from test_step_run_gate import CANVAS, gate_run, take
from test_step_run_support import *


def test_claim_survives_odd_logs(tmp):
    """A name that holds " #" takes its step again; a short or missing events.log never reuses a claim number."""
    folder = gate_run(tmp, "names", [])
    assert take(folder, "01-ask", "lens #2") == "1" and take(folder, "01-ask", "lens #2") == "2"
    with (folder / "events.log").open("a", encoding="utf-8") as log:
        log.write("cut\t01-ask\n")
    assert take(folder, "01-ask", "lens #2") == "3"
    (folder / "events.log").unlink()
    assert take(folder, "01-ask", "lens #2") == "4", "a deleted log must not restart the numbers"
    assert run("done", folder, "01-ask", "4").returncode == 0


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

    repo = gate_run(tmp, "repo-block", ["block my.block script: `true` exits 0: a repo block"])
    assert run("done", repo, "01-ask", take(repo, "01-ask")).returncode == 0
    out = run("done", repo, "02-work", take(repo, "02-work"))
    assert out.returncode == 0 and "not run: block my.block: no kit file" in out.stdout, out


def test_check_runs_in_the_piece(tmp):
    """A piece outside git runs its checks from the piece, where start resolved {{site}} to "."."""
    piece = Path(tempfile.mkdtemp(dir="/tmp"))
    try:
        (piece / "index.js").write_text("const seed = Math.random(); // é\n", encoding="utf-8")
        folder = gate_run(piece, "ink", [f"block build-canvas script: {CANVAS.format(site='.')}"])
        assert run("done", folder, "01-ask", take(folder, "01-ask")).returncode == 0
        held = take(folder, "02-work")
        env = {**os.environ, "LC_ALL": "C", "PYTHONUTF8": "0", "PYTHONIOENCODING": "ascii:backslashreplace"}
        out = subprocess.run([sys.executable, SCRIPT, "done", folder, "02-work", held],
                             capture_output=True, text=True, encoding="utf-8", errors="replace", env=env)
        assert out.returncode == 1 and "block build-canvas check failed" in out.stderr, out
        assert "Traceback" not in out.stderr, "non-ASCII check output must not crash done in the C locale"
    finally:
        shutil.rmtree(piece)


def test_check_results(tmp):
    """rg's exit 1 passes `prints nothing` only with no stderr, and a timeout stops the whole check."""
    folder = Path(tmp, "results")
    folder.mkdir()
    marker = Path(tmp, "late")
    cases = (("sh -c 'echo boom >&2; exit 1'", "boom"),
             (f"(sleep 2; touch {shlex.quote(str(marker))}) & wait", "no result in 1 s"))
    with patch.object(step_checks, "block_checks", lambda block: [re.compile(".*")]), \
         patch.object(step_checks, "CHECK_SECONDS", 1):
        for command, shown in cases:
            todo = f"- [x] block fake script: `{command}` prints nothing: x: ok\n"
            try:
                with redirect_stdout(StringIO()):
                    step_checks.run_checks(folder, "02-work", todo, "")
                raise AssertionError(f"{command} must fail")
            except SystemExit as error:
                assert shown in str(error.code), error.code
        with redirect_stdout(StringIO()):
            step_checks.run_checks(folder, "02-work", "- [x] block fake script: `exit 1` prints nothing: x: ok\n", "")
    time.sleep(2.5)
    assert not marker.exists(), "the timeout must stop the check's children too"


def test_take_logs_inside_the_lock(tmp):
    """take writes its log line while it holds the step lock, so a take --force in between cannot reuse its number."""
    folder = gate_run(tmp, "race", [])
    held = []
    real = step_run.log

    def log(folder, step, event):
        try:
            with step_run.step_lock(folder, step):
                held.append(False)
        except SystemExit:
            held.append(True)
        real(folder, step, event)
    with patch.object(step_run, "log", log), redirect_stdout(StringIO()):
        step_run.take(folder, "01-ask", "w")
    assert held == [True], "the take line must be logged inside the lock"
