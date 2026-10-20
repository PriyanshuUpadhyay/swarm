# Implement test callers cases and helpers.
from implement_test_support import *


def test_callers_after_claim(run, folder):
    file = folder / "04-impact.md"
    original = file.read_text()
    with implement_run.step_run.step_lock(folder, "04-impact"):
        out = run("take", folder, "04-impact", "a")
        assert out.returncode == 1 and "is locked by a running take" in out.stderr, out
        assert file.read_text() == original, "a refused claim must not write callers"
    seen = []

    def write_callers(path):
        assert file.read_text().startswith("Status: active a\n"), "the claim must finish before callers are written"
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


def test_callers_timeout(folder):
    file, contracts = folder / "04-impact.md", folder / "03-contracts.md"
    original = file.read_text(encoding="utf-8")
    original_contracts = contracts.read_text(encoding="utf-8")
    calls = []
    real_run = subprocess.run

    def slow_grep(args, **kwargs):
        if args[:2] == ["git", "grep"]:
            calls.append((args, kwargs))
            if len(calls) == 1:
                raise subprocess.TimeoutExpired(args, implement_run.SEARCH_EACH_SECONDS)
            kwargs["stdout"].write(b"app.py:1:other_name()\n")
            return subprocess.CompletedProcess(args, 0)
        return real_run(args, **kwargs)

    try:
        contracts.write_text(original_contracts + "`other_name()`\n", encoding="utf-8")
        with patch.object(implement_run.subprocess, "run", slow_grep):
            implement_run.write_callers(folder)
        assert len(calls) >= 2, "a timed out search must not stop later names"
        assert all(options.get("timeout") == implement_run.SEARCH_EACH_SECONDS and not options.get("text") for _, options in calls)
        text = file.read_text(encoding="utf-8")
        assert "callers: git grep timed out" in text and "app.py:1:other_name()" in text
    finally:
        file.write_text(original, encoding="utf-8")
        contracts.write_text(original_contracts, encoding="utf-8")


def callers_stub_output(folder, name, stub_body):
    import tracemalloc
    file, contracts = folder / "04-impact.md", folder / "03-contracts.md"
    original, original_contracts = file.read_text(encoding="utf-8"), contracts.read_text(encoding="utf-8")
    try:
        contracts.write_text(f"`{name}()`", encoding="utf-8")
        with tempfile.TemporaryDirectory() as directory:
            stub = Path(directory) / "git"
            stub.write_text(f"#!{sys.executable}\nimport sys\n{stub_body}", encoding="utf-8")
            stub.chmod(0o755)
            with patch.object(implement_run, "git", return_value=str(folder)), \
                    patch.dict(os.environ, {"PATH": directory + os.pathsep + os.environ["PATH"]}):
                tracemalloc.start()
                try:
                    implement_run.write_callers(folder)
                    _, peak = tracemalloc.get_traced_memory()
                finally:
                    tracemalloc.stop()
        return peak, file.read_text(encoding="utf-8")
    finally:
        file.write_text(original, encoding="utf-8")
        contracts.write_text(original_contracts, encoding="utf-8")


def test_callers_large_output(folder):
    peak, text = callers_stub_output(folder, "many_hits",
                                    "for index in range(10000):\n"
                                    "    print(f'app.py:{index + 1}:many_hits() ' + 'x' * 500)\n")
    assert "### many_hits (10000 places)" in text and f"... and {10000 - implement_run.MAX_HITS} more" in text, text
    hits = [line for line in text.splitlines() if line.startswith("app.py:")]
    assert len(hits) == implement_run.MAX_HITS and hits[-1].startswith(f"app.py:{implement_run.MAX_HITS}:")
    assert all(len(hit) <= implement_run.HIT_CHARS for hit in hits)
    assert peak < CALLERS_MEMORY_LIMIT, f"caller search held the full output in memory: peak {peak} bytes"


def test_callers_long_line(folder):
    peak, text = callers_stub_output(folder, "long_hit",
                                    "sys.stdout.buffer.write(b'bundle.js:1:long_hit() ')\n"
                                    "for _ in range(50):\n"
                                    "    sys.stdout.buffer.write(b'x' * 1024 * 1024)\n"
                                    "sys.stdout.buffer.write(b'\\n\\napp.py:2:long_hit()\\napp.py:3:long_hit()')\n")
    assert "### long_hit (3 places)" in text, text
    hits = [line for line in text.splitlines() if line.startswith(("bundle.js:", "app.py:"))]
    assert len(hits) == 3 and len(hits[0]) == implement_run.HIT_CHARS, hits
    assert hits[1:] == ["app.py:2:long_hit()", "app.py:3:long_hit()"], hits
    assert peak < CALLERS_MEMORY_LIMIT, f"caller search held a whole line in memory: peak {peak} bytes"


def test_callers_long_stderr(folder):
    peak, text = callers_stub_output(folder, "failed_hit",
                                    "for _ in range(50):\n"
                                    "    sys.stderr.buffer.write(b'x' * 1024 * 1024)\n"
                                    "sys.exit(128)\n")
    prefix = "callers: git grep failed: "
    errors = [line for line in text.splitlines() if line.startswith(prefix)]
    assert len(errors) == 1 and len(errors[0]) <= len(prefix) + implement_run.LINE_CHUNK, "git grep error line exceeds its chunk limit"
    assert "0 places" not in text
    assert peak < CALLERS_MEMORY_LIMIT, f"caller search held a whole stderr line in memory: peak {peak} bytes"


def test_callers_stderr_utf8_cut(folder):
    _, text = callers_stub_output(folder, "failed_hit",
                                  "sys.stderr.buffer.write(('€' * 2000).encode('utf-8'))\n"
                                  "sys.exit(128)\n")
    errors = [line for line in text.splitlines() if line.startswith("callers: git grep failed: ")]
    assert len(errors) == 1 and errors[0].endswith("€") and "\\x" not in errors[0], repr(errors)[-80:]


def test_callers_stdout_utf8_cut(folder):
    with patch.object(implement_run, "HIT_CHARS", implement_run.LINE_CHUNK):
        _, text = callers_stub_output(folder, "long_hit",
                                      "sys.stdout.buffer.write(('€' * 2000 + '\\n').encode('utf-8'))\n")
    hits = [line for line in text.splitlines() if line.startswith("€")]
    assert "### long_hit (1 places)" in text, text
    assert len(hits) == 1 and hits[0].endswith("€") and "\\x" not in hits[0], repr(hits)[-80:]


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


def test_callers_deadline(folder):
    file, contracts = folder / "04-impact.md", folder / "03-contracts.md"
    original, original_contracts = file.read_text(encoding="utf-8"), contracts.read_text(encoding="utf-8")
    try:
        contracts.write_text("`one_name()` `two_name()` `three_name()`", encoding="utf-8")
        with patch.object(implement_run, "git", return_value=str(folder)), \
                patch("time.monotonic", side_effect=[0, 119, 120]), \
                patch.object(implement_run.subprocess, "run", return_value=subprocess.CompletedProcess([], 1, b"", b"")) as search:
            implement_run.write_callers(folder)
        assert search.call_count == 1, "the total deadline must stop later searches"
        assert "callers: search stopped after 120 s, 2 names not searched" in file.read_text(encoding="utf-8")
        assert search.call_args.kwargs["timeout"] == 1, "each search must use the remaining total budget"
    finally:
        file.write_text(original, encoding="utf-8")
        contracts.write_text(original_contracts, encoding="utf-8")


def test_callers_name_cap(folder):
    file, contracts = folder / "04-impact.md", folder / "03-contracts.md"
    original, original_contracts = file.read_text(encoding="utf-8"), contracts.read_text(encoding="utf-8")
    try:
        contracts.write_text(" ".join(f"`name_{i}()`" for i in range(implement_run.NAME_CAP + 3)), encoding="utf-8")
        with patch.object(implement_run, "git", return_value=str(folder)), \
                patch.object(implement_run.subprocess, "run", return_value=subprocess.CompletedProcess([], 1, b"", b"")) as search:
            implement_run.write_callers(folder)
        assert search.call_count == implement_run.NAME_CAP
        assert "... and 3 more names not searched" in file.read_text(encoding="utf-8")
    finally:
        file.write_text(original, encoding="utf-8")
        contracts.write_text(original_contracts, encoding="utf-8")


def test_callers_end_only_in_impact(tmp):
    """Only 04-impact has a Callers section, so only there does its head end the todo list; in any other
    step a pasted Callers line inside the list must not hide the open items after it."""
    folder = Path(tmp, "callers-end")
    folder.mkdir()
    body = f"Status: open\n\n{implement_run.TODO_HEAD}\n- [x] a: ok\n{implement_run.CALLERS_HEAD}\n%s\n{implement_run.RESULT_HEAD}\n"
    (folder / "03-contracts.md").write_text(body % "- [ ] b: ", encoding="utf-8")
    (folder / "04-impact.md").write_text(body % "- `name` 2 hits", encoding="utf-8")
    try:
        with redirect_stdout(StringIO()):
            implement_run.done(folder, "03-contracts")
        raise AssertionError("03-contracts with an open item after a Callers line must not be done")
    except SystemExit as error:
        assert f"no todo list: the line {implement_run.CALLERS_HEAD!r} inside the list ends it" in str(error.code), error.code
    with redirect_stdout(StringIO()):
        implement_run.done(folder, "04-impact")
    assert (folder / "04-impact.md").read_text(encoding="utf-8").startswith("Status: done "), "04-impact ends at Callers"


def test_callers_cut_list(folder):
    """A todo list that a `## ` line cuts still keeps the rules text out: callers come from the todo head on."""
    file, contracts = folder / "04-impact.md", folder / "03-contracts.md"
    original, original_contracts = file.read_text(encoding="utf-8"), contracts.read_text(encoding="utf-8")
    try:
        contracts.write_text(f"## Rules\n`rule_name()`\n{implement_run.TODO_HEAD}\n- [x] names: below\n## Open questions\n"
                             f"`real_name()`\n\n{implement_run.RESULT_HEAD}\n`result_name()`\n", encoding="utf-8")
        with patch.object(implement_run, "git", return_value=str(folder)), \
                patch.object(implement_run.subprocess, "run", return_value=subprocess.CompletedProcess([], 1, b"", b"")) as search:
            implement_run.write_callers(folder)
        assert sorted(call.args[0][6] for call in search.call_args_list) == ["real_name", "result_name"], search.call_args_list
    finally:
        file.write_text(original, encoding="utf-8")
        contracts.write_text(original_contracts, encoding="utf-8")


def test_callers_below_rules(folder):
    """Rules text may hold a `## Todo (` heading; callers come only from below the step's own todo head."""
    file, contracts = folder / "04-impact.md", folder / "03-contracts.md"
    original, original_contracts = file.read_text(encoding="utf-8"), contracts.read_text(encoding="utf-8")
    try:
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
