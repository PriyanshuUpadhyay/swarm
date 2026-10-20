# Step runner test todos cases and helpers.
from test_step_run_support import *


def test_todo_head_pinned():
    # swarm ui StepRuns.swift:133 matches this exact text; change it there in the same release.
    assert step_run.TODO_HEAD == "## Todo (check a box only with its evidence after the colon; `done` refuses an empty one)", step_run.TODO_HEAD


def test_old_todo_head(folder):
    file = folder / "09-old.md"
    # Step files written from 2026-10-09 to 2026-10-12 hold this other wording.
    head = "## Todo (check a box only with its evidence after the last colon; `done` refuses an empty one)"
    file.write_text(f"Status: open\n\n{head}\n- [ ] old item: \n\n{step_run.RESULT_HEAD}\n", encoding="utf-8")
    try:
        out = run("done", folder, "09-old")
        assert out.returncode == 1 and "old item" in out.stderr, "done must read todos under an older head wording"
    finally:
        for path in (file, folder / "09-old.lock"):
            path.unlink(missing_ok=True)


def test_rules_todo_heading(folder):
    """Rules text above the step's own todo head may hold a `## Todo (` and a `## Results` heading."""
    file = folder / "09-rules.md"
    file.write_text(f"Status: open\n\n## Rules\n## Todo (later)\n- later item\n## Results\n\n"
                    f"{step_run.TODO_HEAD}\n- [ ] real item: \n\n{step_run.RESULT_HEAD}\n", encoding="utf-8")
    try:
        out = run("done", folder, "09-rules")
        assert out.returncode == 1 and "real item" in out.stderr, "done must read the step's own todo list"
    finally:
        for path in (file, folder / "09-rules.lock"):
            path.unlink(missing_ok=True)


def test_result_todo_heading(folder):
    """Agent text under `## Result` may hold a checked `## Todo (` list; it must not hide the open real todos."""
    file = folder / "09-result.md"
    fake = f"## Todo (x)\n- [x] fake: ok\n\n{step_run.RESULT_HEAD}\n"
    try:
        for tail in (fake, fake.removesuffix(f"\n{step_run.RESULT_HEAD}\n")):
            file.write_text(f"Status: open\n\n{step_run.TODO_HEAD}\n- [ ] real item: \n\n{step_run.RESULT_HEAD}\n{tail}",
                            encoding="utf-8")
            out = run("done", folder, "09-result")
            assert out.returncode == 1 and "real item" in out.stderr, "done must read the step's own todo list"
        file.write_text("Status: open\n\n## Todo (x)\n- later item\n## Other\n", encoding="utf-8")
        out = run("done", folder, "09-result")
        assert out.returncode == 1 and "no todo list" in out.stderr, "done must refuse a file with no todo list"
    finally:
        for path in (file, folder / "09-result.lock"):
            path.unlink(missing_ok=True)


def test_todo_decoy_in_list(folder):
    """A `## ` line inside the real list (a pasted `## Todo (` or `## Other`, the tail of another step file,
    under the current or an older head wording) does not hand the list to a later head, so done refuses."""
    head, old = step_run.TODO_HEAD, "## Todo (check a box only with its evidence after the last colon; `done` refuses an empty one)"
    items = "- [ ] real open item: \n- [x] b: pasted\n"
    texts = [f"{head}\n- [ ] a: \n- [x] b: pasted:\n## Todo (x)\n- [x] fake: ok\n\n## Result\n",
             f"{head}\n{items}## Other\n## Todo (x)\n- [x] fake: ok\n## Result\n",
             f"{old}\n{items}## Todo (x)\n- [x] fake: ok\n## Result\n",
             f"{head}\n{items}## Rules for this step (pasted)\n- a rule\n{head}\n- [x] fake: ok\n## Result\n"]
    file = folder / "09-decoy.md"
    try:
        for text in texts:
            text = f"Status: open\n\n{text}"
            assert step_run.todo_scan(text)[1] == -1, (text, step_run.todo_scan(text))
            file.write_text(text, encoding="utf-8")
            out = run("done", folder, "09-decoy")
            assert out.returncode == 1 and "no todo list" in out.stderr, (text, out)
    finally:
        for path in (file, folder / "09-decoy.lock"):
            path.unlink(missing_ok=True)


def test_todo_list_cut_names_line(folder):
    """When a `## ` line inside the todo list ends it, done names that line and the fix in one line; a file
    with no todo head keeps the plain reason."""
    file = folder / "09-cut.md"
    try:
        file.write_text(f"Status: open\n\n{step_run.TODO_HEAD}\n- [x] Scope: files touched\n## main...origin/main\n"
                        f"- [x] b: ok\n{step_run.RESULT_HEAD}\n", encoding="utf-8")
        out = run("done", folder, "09-cut")
        assert out.returncode == 1 and out.stderr == ("09-cut is not done: no todo list: the line '## main...origin/main' "
                                                      "inside the list ends it; indent it or remove the leading '## '\n"), out
        file.write_text("Status: open\n\n## Notes\n", encoding="utf-8")
        out = run("done", folder, "09-cut")
        assert out.returncode == 1 and "no todo list (" in out.stderr, out
    finally:
        for path in (file, folder / "09-cut.lock"):
            path.unlink(missing_ok=True)


def test_todo_list_shapes(folder):
    """Any text may sit between todo items (multi-line evidence, a note); `- [X]` is not a checked box, so
    done names it; evidence that holds `## Result` does not end the list early; and a checked line with no
    colon has no evidence."""
    file, head, result = folder / "09-shape.md", step_run.TODO_HEAD, step_run.RESULT_HEAD
    cases = [(f"- [x] one: first line\n  second line of evidence\n- [x] two: ok\n", 0, "done"),
             (f"- [x] one: a\nnote: wrapped\n<!-- comment -->\n- [x] two: b\n", 0, "done"),
             (f"- [X] one: a\n", 1, "- [X] one: a"),
             (f"- [x] a: moved {result} up\n- [ ] b: \n", 1, "- [ ] b:"),
             (f"- [x] the question\n", 1, "no evidence: - [x] the question")]
    events = (folder / "events.log").read_text(encoding="utf-8")
    try:
        for items, code, said in cases:
            file.write_text(f"Status: open\n\n{head}\n{items}\n{result}\n", encoding="utf-8")
            out = run("done", folder, "09-shape")
            assert out.returncode == code and said in out.stdout + out.stderr, (items, out.stdout, out.stderr)
            assert "no todo list" not in out.stderr, (items, out.stderr)
    finally:
        for path in (file, folder / "09-shape.lock"):
            path.unlink(missing_ok=True)
        (folder / "events.log").write_text(events, encoding="utf-8")


def test_step_mode(folder):
    file = folder / "01-ask.md"
    original = file.read_text(encoding="utf-8")
    events = (folder / "events.log").read_text(encoding="utf-8")
    try:
        file.chmod(0o644)
        with redirect_stdout(StringIO()):
            step_run.take(folder, "01-ask", "mode")
        assert file.stat().st_mode & 0o777 == 0o644, "take must preserve the step file mode"
        new = folder / "new.md"
        step_run.write_step(new, "new step\n")
        assert new.stat().st_mode & 0o777 == 0o644
    finally:
        file.write_text(original, encoding="utf-8")
        (folder / "events.log").write_text(events, encoding="utf-8")
