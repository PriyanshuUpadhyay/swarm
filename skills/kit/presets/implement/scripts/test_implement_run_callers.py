# Implement test callers cases and helpers.
from test_implement_run_support import *


def test_callers_after_claim(run, folder):
    file = folder / "04-impact.md"
    original = file.read_text()
    with implement_run.step_run.step_lock(folder, "04-impact"):
        out = run("take", folder, "04-impact", "a")
        assert out.returncode == 1 and "is locked by a running take" in out.stderr, out
        assert file.read_text() == original, "a refused claim must not write callers"
    seen = []

    def write_callers(path):
        assert file.read_text(encoding="utf-8").startswith("Status: active a #"), "the claim must finish before callers are written"
        try:
            implement_run.skip(folder, "04-impact", "racing skip")
            raise AssertionError("skip must refuse during the callers write")
        except SystemExit as error:
            assert "is locked by a running skip" in str(error), error
        seen.append(path)

    with redirect_stdout(StringIO()), patch.object(implement_run, "write_callers", write_callers):
        implement_run.take(folder, "04-impact", "a")
    assert seen == [folder]
    file.write_text(original)


def test_callers_failure(folder):
    file = folder / "04-impact.md"
    original = file.read_text(encoding="utf-8")
    def failed_grep(args, **kwargs):
        kwargs["stderr"].write(b"\x1b[31mcorrupt index\nextra detail")
        return subprocess.CompletedProcess(args, 128)
    try:
        with patch.object(implement_run, "git", return_value=str(folder)), \
                patch.object(implement_run.subprocess, "run", side_effect=failed_grep):
            implement_run.write_callers(folder)
        text = file.read_text(encoding="utf-8")
        assert "callers: git grep failed: [31mcorrupt index" in text, text
        assert "0 places" not in text and "extra detail" not in text and "\x1b" not in text
    finally:
        file.write_text(original, encoding="utf-8")


def test_callers_end_only_in_impact(tmp):
    """Only 04-impact has a Callers section, so only there does its head end the todo list; in any other
    step a pasted Callers line inside the list must not hide the open items after it."""
    folder = Path(tmp, "callers-end")
    folder.mkdir()
    body = f"Status: active t #1\n\n{implement_run.TODO_HEAD}\n- [x] a: ok\n{implement_run.CALLERS_HEAD}\n%s\n{implement_run.RESULT_HEAD}\n"
    (folder / "03-contracts.md").write_text(body % "- [ ] b: ", encoding="utf-8")
    (folder / "04-impact.md").write_text(body % "- `name` 2 hits", encoding="utf-8")
    try:
        with redirect_stdout(StringIO()):
            implement_run.done(folder, "03-contracts", claim(folder, "03-contracts"))
        raise AssertionError("03-contracts with an open item after a Callers line must not be done")
    except SystemExit as error:
        assert f"no todo list: the line {implement_run.CALLERS_HEAD!r} inside the list ends it" in str(error.code), error.code
    with redirect_stdout(StringIO()):
        implement_run.done(folder, "04-impact", claim(folder, "04-impact"))
    assert (folder / "04-impact.md").read_text(encoding="utf-8").startswith("Status: done "), "04-impact ends at Callers"


def test_callers_cut_list(folder):
    """A todo list that a `## ` line cuts, or a `## Todo (` heading in the rules text, still keeps the rules text
    out: callers come from the step's own todo head on."""
    file, contracts = folder / "04-impact.md", folder / "03-contracts.md"
    original, original_contracts = file.read_text(encoding="utf-8"), contracts.read_text(encoding="utf-8")
    try:
        contracts.write_text(f"## Rules\n`rule_name()`\n{implement_run.TODO_HEAD}\n- [x] names: below\n## Open questions\n"
                             f"`real_name()`\n\n{implement_run.RESULT_HEAD}\n`result_name()`\n", encoding="utf-8")
        with patch.object(implement_run, "git", return_value=str(folder)), \
                patch.object(implement_run.subprocess, "run", return_value=subprocess.CompletedProcess([], 1, b"", b"")) as search:
            implement_run.write_callers(folder)
        assert sorted(call.args[0][6] for call in search.call_args_list) == ["real_name", "result_name"], search.call_args_list
        contracts.write_text(f"## Todo (later)\n`rule_name()`\n{implement_run.TODO_HEAD}\n- [x] names: below\n\n"
                             f"{implement_run.RESULT_HEAD}\n`real_name()`\n", encoding="utf-8")
        with patch.object(implement_run, "git", return_value=str(folder)), \
                patch.object(implement_run.subprocess, "run", return_value=subprocess.CompletedProcess([], 1, b"", b"")) as search:
            implement_run.write_callers(folder)
        assert [call.args[0][6] for call in search.call_args_list] == ["real_name"], search.call_args_list
    finally:
        file.write_text(original, encoding="utf-8")
        contracts.write_text(original_contracts, encoding="utf-8")


def test_callers_controls(repo, run, folder):
    file, app = folder / "04-impact.md", repo / "app.py"
    original, source = file.read_text(encoding="utf-8"), app.read_bytes()
    try:
        assert hasattr(implement_run.step_run, "plain") and implement_run.plain is implement_run.step_run.plain
        app.write_bytes(b"\t# \x1b]0;title\x07 \x7f \xc2\x80 parse_phone()\n")
        out = run("take", folder, "04-impact", "a")
        assert out.returncode == 0, out.stderr
        assert "]0;title" in out.stdout
        assert ":\t# " in out.stdout, "tab-indented hits must keep their indentation"
        assert not has_control(out.stdout), repr(out.stdout)
        assert "parse_phone" in file.read_text(encoding="utf-8")
    finally:
        app.write_bytes(source)
        file.write_text(original, encoding="utf-8")
