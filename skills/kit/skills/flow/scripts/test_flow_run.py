"""Run: python3 test_flow_run.py  (uses a temporary repo and a temporary HOME). Needs Python 3.11 or later,
because start reads TOML with tomllib."""
import fcntl
import hashlib
import json
import os
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
from contextlib import redirect_stdout
from io import StringIO
from pathlib import Path
from unittest.mock import patch

if sys.version_info < (3, 11):
    raise SystemExit(f"test_flow_run.py needs Python 3.11 or later (tomllib); this is Python {sys.version.split()[0]}")

sys.path.insert(0, str(Path(__file__).resolve().parent))  # python3 -I does not add the script's folder
import flow_run  # noqa: E402

SCRIPT = Path(__file__).resolve().with_name("flow_run.py")  # the kit path, also when run through a linked skill folder
CALLERS_MEMORY_LIMIT = 1_000_000


def test_skip_text_encoding():
    import ast
    tree = ast.parse(Path(__file__).read_text(encoding="utf-8"))
    missing = [call.lineno for node in tree.body if isinstance(node, ast.FunctionDef)
               and node.name in ("test_skip_lock", "test_skip_rejects_force")
               for call in ast.walk(node) if isinstance(call, ast.Call)
               and isinstance(call.func, ast.Attribute) and call.func.attr in ("read_text", "write_text")
               and not any(kw.arg == "encoding" for kw in call.keywords)]
    assert not missing, f"skip test reads or writes without UTF-8 encoding at {missing}"


def test_command_wrong_count(run, folder):
    for command, args in (("start", ("extra",)), ("start", ("--profile",)), ("status", (folder, "extra")),
                          ("take", ()), ("take", (folder, "01-frame", "a", "extra")),
                          ("done", ()), ("skip", ()), ("skip", (folder,)),
                          ("skip", (folder, "02-design"))):
        out = run(command, *args)
        usage = next(line.strip() for line in flow_run.__doc__.splitlines()
                     if line.strip().split()[:1] == [command])
        assert out.returncode == 1 and usage in out.stderr, out.stderr
        assert "Traceback" not in out.stderr, out.stderr
    assert run("status").returncode == 0, "status without a folder must use the latest run"


def test_done_wrong_count(run, folder):
    for args in ((folder,), (folder, "01-frame", "extra")):
        out = run("done", *args)
        assert out.returncode == 1 and "done <folder> <step>" in out.stderr, out.stderr
        assert "--force" not in out.stderr and "Traceback" not in out.stderr, out.stderr


def test_skip_plain_reason(run, folder):
    file = folder / "02-design.md"
    original = file.read_text(encoding="utf-8")
    events = (folder / "events.log").read_text(encoding="utf-8")
    try:
        out = run("skip", folder, "02-design", "\x1bno\nUI\tchange\x7f")
        assert out.returncode == 0, out.stderr
        text = file.read_text(encoding="utf-8")
        assert text.splitlines()[0] == "Status: skipped no UI change", text
        assert text.split("\n", 1)[1] == original.split("\n", 1)[1], "skip must preserve line 2 and the body"
        lines = (folder / "events.log").read_text(encoding="utf-8").splitlines()
        assert len(lines) == len(events.splitlines()) + 1 and lines[-1].endswith("\tskip no UI change"), lines
        assert "\x1b" not in out.stdout and "\x7f" not in out.stdout
    finally:
        file.write_text(original, encoding="utf-8")
        (folder / "events.log").write_text(events, encoding="utf-8")


def fill(f, evidence="evidence"):
    """Check every todo of a step file with evidence, as an agent would."""
    f.write_text("".join(l.replace("- [ ]", "- [x]").rstrip() + (f" {evidence}\n" if l.startswith("- [") and l.rstrip().endswith(":") else "\n")
                         for l in f.read_text(encoding="utf-8").splitlines()), encoding="utf-8")


def test_take_force(run, folder):
    file = folder / "01-frame.md"
    original = file.read_text()
    assert run("take", folder, "01-frame", "agent-a").returncode == 0
    refused = run("take", folder, "01-frame", "agent-b")
    assert refused.returncode != 0 and "active for agent-a" in refused.stderr
    for args, who in (([folder, "01-frame", "agent-b", "--force"], "agent-b"),
                      (["--force", folder, "01-frame", "agent-c"], "agent-c"),
                      ([folder, "01-frame", "--force", "agent-d"], "agent-d")):
        out = run("take", *args)
        assert out.returncode == 0, out.stderr
        assert file.read_text().startswith(f"Status: active {who}\n")
        assert (folder / "01-frame.lock").exists()
        assert (folder / "events.log").read_text().splitlines()[-1].endswith(f"\t01-frame\ttake {who}")
    file.write_text(original)


def test_take_requires_who(run, folder):
    file = folder / "01-frame.md"
    original = file.read_text()
    events = (folder / "events.log").read_text()
    for args in ((folder, "01-frame"), (folder, "01-frame", "--force")):
        out = run("take", *args)
        assert out.returncode != 0, "take without who must refuse the claim"
        assert "who" in out.stderr and "Traceback" not in out.stderr, out.stderr
        assert file.read_text() == original
        assert (folder / "events.log").read_text() == events


def test_done_rejects_force(run, folder):
    file = folder / "01-frame.md"
    original = file.read_text(encoding="utf-8")
    events = (folder / "events.log").read_text(encoding="utf-8")
    try:
        fill(file)
        filled = file.read_text(encoding="utf-8")
        for args in ((folder, "01-frame", "--force"), ("--force", folder, "01-frame")):
            out = run("done", *args)
            assert out.returncode == 1, "done must refuse --force"
            assert "done <folder> <step>" in out.stderr and "Traceback" not in out.stderr, out.stderr
            assert file.read_text(encoding="utf-8") == filled
            assert (folder / "events.log").read_text(encoding="utf-8") == events
    finally:
        file.write_text(original, encoding="utf-8")
        (folder / "events.log").write_text(events, encoding="utf-8")


def test_atomic_skip(folder):
    file = folder / "02-design.md"
    original = file.read_text()
    real_replace = os.replace
    replaced = []

    def check_replace(source, destination):
        assert file.read_text() == original, "skip must preserve the old file until replacement"
        assert source.parent == file.parent and source.suffix == ".tmp" and destination == file
        with (folder / "02-design.lock").open("a") as contender:
            try:
                fcntl.flock(contender.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                pass
            else:
                raise AssertionError("skip must hold the step lock during replacement")
        real_replace(source, destination)
        replaced.append(destination)

    with redirect_stdout(StringIO()), patch.object(flow_run.step_run.os, "replace", check_replace):
        flow_run.skip(folder, "02-design", "no", "UI")
    assert replaced == [file], "skip must replace the file atomically"
    assert file.read_text().startswith("Status: skipped no UI\n")
    file.write_text(original)


def test_skip_lock(run, folder):
    file = folder / "02-design.md"
    original = file.read_text(encoding="utf-8")
    events = (folder / "events.log").read_text(encoding="utf-8")
    with flow_run.step_run.step_lock(folder, "02-design"):
        out = run("skip", folder, "02-design", "no", "UI")
        assert out.returncode == 1 and "02-design is locked by a running skip; wait for it" in out.stderr, out
        assert file.read_text(encoding="utf-8") == original
        assert (folder / "events.log").read_text(encoding="utf-8") == events


def test_skip_rejects_force(run, folder):
    file = folder / "02-design.md"
    original = file.read_text(encoding="utf-8")
    events = (folder / "events.log").read_text(encoding="utf-8")
    try:
        for args in ((folder, "02-design", "no", "UI", "--force"),
                     ("--force", folder, "02-design", "no", "UI")):
            file.write_text(original.replace("Status: open", "Status: active dead-owner"), encoding="utf-8")
            out = run("skip", *args)
            assert out.returncode == 1, "skip must refuse --force"
            assert "skip accepts no --force" in out.stderr and "Traceback" not in out.stderr, out.stderr
            assert file.read_text(encoding="utf-8") == original.replace("Status: open", "Status: active dead-owner")
            assert (folder / "events.log").read_text(encoding="utf-8") == events
    finally:
        file.write_text(original, encoding="utf-8")
        (folder / "events.log").write_text(events, encoding="utf-8")


def test_callers_after_claim(run, folder):
    file = folder / "04-impact.md"
    original = file.read_text()
    with flow_run.step_run.step_lock(folder, "04-impact"):
        out = run("take", folder, "04-impact", "a")
        assert out.returncode == 1 and "is locked by a running take" in out.stderr, out
        assert file.read_text() == original, "a refused claim must not write callers"
    seen = []

    def write_callers(path):
        assert file.read_text().startswith("Status: active a\n"), "the claim must finish before callers are written"
        try:
            flow_run.skip(folder, "04-impact", "racing skip")
            raise AssertionError("skip must refuse during the callers write")
        except SystemExit as error:
            assert "is locked by a running skip" in str(error), error
        seen.append(path)

    with redirect_stdout(StringIO()), patch.object(flow_run, "write_callers", write_callers):
        flow_run.take(folder, "04-impact", "a")
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
                raise subprocess.TimeoutExpired(args, flow_run.SEARCH_EACH_SECONDS)
            kwargs["stdout"].write(b"app.py:1:other_name()\n")
            return subprocess.CompletedProcess(args, 0)
        return real_run(args, **kwargs)

    try:
        contracts.write_text(original_contracts + "`other_name()`\n", encoding="utf-8")
        with patch.object(flow_run.subprocess, "run", slow_grep):
            flow_run.write_callers(folder)
        assert len(calls) >= 2, "a timed out search must not stop later names"
        assert all(options.get("timeout") == flow_run.SEARCH_EACH_SECONDS and not options.get("text") for _, options in calls)
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
            with patch.object(flow_run, "git", return_value=str(folder)), \
                    patch.dict(os.environ, {"PATH": directory + os.pathsep + os.environ["PATH"]}):
                tracemalloc.start()
                try:
                    flow_run.write_callers(folder)
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
    assert "### many_hits (10000 places)" in text and f"... and {10000 - flow_run.MAX_HITS} more" in text, text
    hits = [line for line in text.splitlines() if line.startswith("app.py:")]
    assert len(hits) == flow_run.MAX_HITS and hits[-1].startswith(f"app.py:{flow_run.MAX_HITS}:")
    assert all(len(hit) <= flow_run.HIT_CHARS for hit in hits)
    assert peak < CALLERS_MEMORY_LIMIT, f"caller search held the full output in memory: peak {peak} bytes"


def test_callers_long_line(folder):
    peak, text = callers_stub_output(folder, "long_hit",
                                    "sys.stdout.buffer.write(b'bundle.js:1:long_hit() ')\n"
                                    "for _ in range(50):\n"
                                    "    sys.stdout.buffer.write(b'x' * 1024 * 1024)\n"
                                    "sys.stdout.buffer.write(b'\\n\\napp.py:2:long_hit()\\napp.py:3:long_hit()')\n")
    assert "### long_hit (3 places)" in text, text
    hits = [line for line in text.splitlines() if line.startswith(("bundle.js:", "app.py:"))]
    assert len(hits) == 3 and len(hits[0]) == flow_run.HIT_CHARS, hits
    assert hits[1:] == ["app.py:2:long_hit()", "app.py:3:long_hit()"], hits
    assert peak < CALLERS_MEMORY_LIMIT, f"caller search held a whole line in memory: peak {peak} bytes"


def test_callers_long_stderr(folder):
    peak, text = callers_stub_output(folder, "failed_hit",
                                    "for _ in range(50):\n"
                                    "    sys.stderr.buffer.write(b'x' * 1024 * 1024)\n"
                                    "sys.exit(128)\n")
    prefix = "callers: git grep failed: "
    errors = [line for line in text.splitlines() if line.startswith(prefix)]
    assert len(errors) == 1 and len(errors[0]) <= len(prefix) + flow_run.LINE_CHUNK, "git grep error line exceeds its chunk limit"
    assert "0 places" not in text
    assert peak < CALLERS_MEMORY_LIMIT, f"caller search held a whole stderr line in memory: peak {peak} bytes"


def test_callers_stderr_utf8_cut(folder):
    _, text = callers_stub_output(folder, "failed_hit",
                                  "sys.stderr.buffer.write(('€' * 2000).encode('utf-8'))\n"
                                  "sys.exit(128)\n")
    errors = [line for line in text.splitlines() if line.startswith("callers: git grep failed: ")]
    assert len(errors) == 1 and errors[0].endswith("€") and "\\x" not in errors[0], repr(errors)[-80:]


def test_callers_stdout_utf8_cut(folder):
    with patch.object(flow_run, "HIT_CHARS", flow_run.LINE_CHUNK):
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
        with patch.object(flow_run, "git", return_value=str(folder)), \
                patch.object(flow_run.subprocess, "run", side_effect=failed_grep):
            flow_run.write_callers(folder)
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
        with patch.object(flow_run, "git", return_value=str(folder)), \
                patch("time.monotonic", side_effect=[0, 119, 120]), \
                patch.object(flow_run.subprocess, "run", return_value=subprocess.CompletedProcess([], 1, b"", b"")) as search:
            flow_run.write_callers(folder)
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
        contracts.write_text(" ".join(f"`name_{i}()`" for i in range(flow_run.NAME_CAP + 3)), encoding="utf-8")
        with patch.object(flow_run, "git", return_value=str(folder)), \
                patch.object(flow_run.subprocess, "run", return_value=subprocess.CompletedProcess([], 1, b"", b"")) as search:
            flow_run.write_callers(folder)
        assert search.call_count == flow_run.NAME_CAP
        assert "... and 3 more names not searched" in file.read_text(encoding="utf-8")
    finally:
        file.write_text(original, encoding="utf-8")
        contracts.write_text(original_contracts, encoding="utf-8")


def test_callers_end_only_in_impact(tmp):
    """Only 04-impact has a Callers section, so only there does its head end the todo list; in any other
    step a pasted Callers line inside the list must not hide the open items after it."""
    folder = Path(tmp, "callers-end")
    folder.mkdir()
    body = f"Status: open\n\n{flow_run.TODO_HEAD}\n- [x] a: ok\n{flow_run.CALLERS_HEAD}\n%s\n{flow_run.RESULT_HEAD}\n"
    (folder / "03-contracts.md").write_text(body % "- [ ] b: ", encoding="utf-8")
    (folder / "04-impact.md").write_text(body % "- `name` 2 hits", encoding="utf-8")
    try:
        with redirect_stdout(StringIO()):
            flow_run.done(folder, "03-contracts")
        raise AssertionError("03-contracts with an open item after a Callers line must not be done")
    except SystemExit as error:
        assert f"no todo list: the line {flow_run.CALLERS_HEAD!r} inside the list ends it" in str(error.code), error.code
    with redirect_stdout(StringIO()):
        flow_run.done(folder, "04-impact")
    assert (folder / "04-impact.md").read_text(encoding="utf-8").startswith("Status: done "), "04-impact ends at Callers"


def test_callers_cut_list(folder):
    """A todo list that a `## ` line cuts still keeps the rules text out: callers come from the todo head on."""
    file, contracts = folder / "04-impact.md", folder / "03-contracts.md"
    original, original_contracts = file.read_text(encoding="utf-8"), contracts.read_text(encoding="utf-8")
    try:
        contracts.write_text(f"## Rules\n`rule_name()`\n{flow_run.TODO_HEAD}\n- [x] names: below\n## Open questions\n"
                             f"`real_name()`\n\n{flow_run.RESULT_HEAD}\n`result_name()`\n", encoding="utf-8")
        with patch.object(flow_run, "git", return_value=str(folder)), \
                patch.object(flow_run.subprocess, "run", return_value=subprocess.CompletedProcess([], 1, b"", b"")) as search:
            flow_run.write_callers(folder)
        assert sorted(call.args[0][6] for call in search.call_args_list) == ["real_name", "result_name"], search.call_args_list
    finally:
        file.write_text(original, encoding="utf-8")
        contracts.write_text(original_contracts, encoding="utf-8")


def test_callers_below_rules(folder):
    """Rules text may hold a `## Todo (` heading; callers come only from below the step's own todo head."""
    file, contracts = folder / "04-impact.md", folder / "03-contracts.md"
    original, original_contracts = file.read_text(encoding="utf-8"), contracts.read_text(encoding="utf-8")
    try:
        contracts.write_text(f"## Todo (later)\n`rule_name()`\n{flow_run.TODO_HEAD}\n- [x] names: below\n\n"
                             f"{flow_run.RESULT_HEAD}\n`real_name()`\n", encoding="utf-8")
        with patch.object(flow_run, "git", return_value=str(folder)), \
                patch.object(flow_run.subprocess, "run", return_value=subprocess.CompletedProcess([], 1, b"", b"")) as search:
            flow_run.write_callers(folder)
        assert [call.args[0][6] for call in search.call_args_list] == ["real_name"], search.call_args_list
    finally:
        file.write_text(original, encoding="utf-8")
        contracts.write_text(original_contracts, encoding="utf-8")


def test_callers_controls(repo, run, folder):
    file, app = folder / "04-impact.md", repo / "app.py"
    original, source = file.read_text(encoding="utf-8"), app.read_bytes()
    try:
        assert hasattr(flow_run.step_run, "plain") and flow_run.plain is flow_run.step_run.plain
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


def test_impact_closed_stdout(repo, env, folder):
    file = folder / "04-impact.md"
    original = file.read_text()
    file.write_text(original + "large output\n" * 10000)
    try:
        with subprocess.Popen([sys.executable, SCRIPT, "take", str(folder), "04-impact", "pipe"],
                              cwd=repo, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE) as child:
            with subprocess.Popen(["head", "-1"], stdin=child.stdout, stdout=subprocess.PIPE) as consumer:
                child.stdout.close()
                output, _ = consumer.communicate()
                assert consumer.returncode == 0 and output == b"Status: active pipe\n"
            errors = child.stderr.read()
            assert child.wait() == 0 and not errors, errors
        assert flow_run.CALLERS_HEAD in file.read_text(), "callers must be saved before printing to a closed pipe"
    finally:
        file.write_text(original)


def test_impact_utf8(repo, env, folder):
    file = folder / "04-impact.md"
    contracts = folder / "03-contracts.md"
    original = file.read_text(encoding="utf-8")
    original_contracts = contracts.read_text(encoding="utf-8")
    app = repo / "app.py"
    original_app = app.read_bytes()
    # Disable C-locale coercion, UTF-8 mode, and UTF-8 stream output independently.
    ascii_env = {**env, "LC_ALL": "C", "PYTHONUTF8": "0", "PYTHONCOERCECLOCALE": "0", "PYTHONIOENCODING": "ascii"}
    try:
        app.write_bytes(original_app + "# 合意 parse_phone\n".encode("utf-8") + b"# \xff parse_phone\n")
        for note in ("", "合意\n"):
            file.write_text(original, encoding="utf-8")
            contracts.write_text(original_contracts + note, encoding="utf-8")
            out = subprocess.run([sys.executable, SCRIPT, "take", str(folder), "04-impact", "ascii-owner"],
                                 cwd=repo, env=ascii_env, capture_output=True, text=True)
            assert out.returncode == 0, out.stderr
            assert "\\u5408\\u610f" in out.stdout and "\\xff" in out.stdout and flow_run.CALLERS_HEAD in out.stdout
            assert file.read_text(encoding="utf-8").startswith("Status: active ascii-owner\n")
            assert flow_run.CALLERS_HEAD in file.read_text(encoding="utf-8")
    finally:
        file.write_text(original, encoding="utf-8")
        contracts.write_text(original_contracts, encoding="utf-8")
        app.write_bytes(original_app)


def test_closed_stdout(repo, env, folder):
    file = folder / "01-frame.md"
    original = file.read_text()
    file.write_text(original + "large output\n" * 10000)
    try:
        with subprocess.Popen([sys.executable, SCRIPT, "take", str(folder), "01-frame", "pipe"],
                              cwd=repo, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE) as child:
            with subprocess.Popen(["head", "-c", "10"], stdin=child.stdout, stdout=subprocess.PIPE) as consumer:
                child.stdout.close()
                output, _ = consumer.communicate()
                assert consumer.returncode == 0 and len(output) == 10
            errors = child.stderr.read()
            assert child.wait() == 0 and not errors, errors
    finally:
        file.write_text(original)


# Every git call and flow run in this file: no global or system git config (signing, hooks, aliases), a fixed author,
# and no GIT_* variable of the caller, such as the GIT_DIR a git hook sets. Build each env with git_env().
GIT_ENV = {"GIT_CONFIG_GLOBAL": os.devnull, "GIT_CONFIG_NOSYSTEM": "1", "GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@t",
           "GIT_COMMITTER_NAME": "t", "GIT_COMMITTER_EMAIL": "t@t"}


def git_env():
    return {**{k: v for k, v in os.environ.items() if not k.startswith("GIT_")}, **GIT_ENV}

PARAMS = '[params]\nsource_of_truth = "docs/spec.md"\nsite = "site"\nshots = "tmp/shots"\n'
RED_BUILD = "- build: block:red-first-check\n"  # the catalog line most block tests start from


def blocks_start(tmp, name, catalog, repo_toml=PARAMS, *args, env=None, python=sys.executable):
    """Start a flow in a new repo whose catalog `any` section has the given step lines, with the kit's blocks."""
    home, repo = Path(tmp, name, "home"), Path(tmp, name, "app")
    (home / ".flow" / "app").mkdir(parents=True)
    (home / ".flow" / "domains.md").write_text("## any\n" + catalog, encoding="utf-8")
    (home / ".flow" / "app" / "profile.md").write_text("Domains: any\nRepo rules: none\n", encoding="utf-8")
    repo.mkdir(exist_ok=True)
    subprocess.run(["git", "init", "-q", "-b", "main"], cwd=repo, env=git_env(), check=True)
    if repo_toml is not None:
        (repo / ".blocks.toml").write_text(repo_toml, encoding="utf-8")
    run = lambda *a: subprocess.run([python, SCRIPT, *map(str, a)], cwd=repo,
                                    env={**git_env(), "HOME": str(home), **(env or {})}, capture_output=True, text=True)
    out = run("start", *args)
    folder = Path(out.stdout.splitlines()[0]) if out.returncode == 0 else None
    return out, folder, repo, run


def step_text(folder, step):
    return (folder / f"{step}.md").read_text(encoding="utf-8")


def has_control(text, keep="\t\n"):
    """True if text holds a C0 or C1 control character or DEL, other than those in keep."""
    return any(ord(c) < 32 and c not in keep or 127 <= ord(c) <= 159 for c in text)


def test_git_env_ignores_caller_git_dir(tmp):
    """A run from a git hook inherits GIT_DIR, GIT_WORK_TREE, and GIT_INDEX_FILE; the tests' git calls and flow
    runs still use their own repo, so the caller's repo gets no commit and no tmp/flow folder."""
    decoy = Path(tmp, "decoy")
    subprocess.run(["git", "init", "-q", "-b", "main", str(decoy)], env=git_env(), check=True)
    hook = {"GIT_DIR": str(decoy / ".git"), "GIT_WORK_TREE": str(decoy), "GIT_INDEX_FILE": str(decoy / ".git" / "index")}
    with patch.dict(os.environ, hook):
        out, folder, repo, _ = blocks_start(tmp, "hooked", RED_BUILD)
        subprocess.run(["git", "commit", "-q", "--allow-empty", "-m", "one"], cwd=repo, env=git_env(), check=True)
    assert out.returncode == 0 and folder.resolve().is_relative_to(repo.resolve()), out
    log = subprocess.run(["git", "log", "--oneline"], cwd=decoy, env=git_env(), capture_output=True, text=True)
    assert log.stdout == "" and not (decoy / "tmp").exists(), log


def test_block_in_step(tmp):
    """A catalog step line naming block:<id> writes that block into the step file, and the
    Skills read line names the profile and the block with its content hash."""
    out, folder, repo, run = blocks_start(tmp, "d1", RED_BUILD)
    assert out.returncode == 0, out.stderr
    build = step_text(folder, "05-build")
    assert "===== block:red-first-check =====" in build and "Rank each claim on the ladder" in build, build
    skills = next(line for line in build.splitlines() if line.startswith("Skills read:"))
    assert "profile default" in skills, skills
    digest = hashlib.sha256((flow_run.BLOCKS / "red-first-check.md").read_bytes()).hexdigest()[:8]
    assert f"block:red-first-check@{digest}" in [item.strip() for item in skills.removeprefix("Skills read:").split(",")], skills
    assert "Rank each claim on the ladder" not in step_text(folder, "02-design")
    # A second start on the folder writes nothing, even when the catalog now fails.
    (repo.parent / "home" / ".flow" / "domains.md").write_text("## any\n- build: block:nope\n", encoding="utf-8")
    again = run("start")
    assert again.returncode == 0 and step_text(folder, "05-build") == build, again.stderr


def test_block_alias_and_base(tmp):
    """An alias expands to its members, duplicates drop, and base joins every step once."""
    out, folder, _, _ = blocks_start(tmp, "d2", "- design: block:tech-diagram, block:html-diagram\n" + RED_BUILD)
    assert out.returncode == 0, out.stderr
    design = step_text(folder, "02-design")
    for block in ("first-principles", "tech-diagram-concept", "html-diagram"):
        assert design.count(f"===== block:{block} =====") == 1, (block, design)
    # html-diagram comes twice, through the alias and alone; a merged group prints one header either way.
    skills = next(line for line in design.splitlines() if line.startswith("Skills read: "))
    assert skills.count("block:html-diagram@") == 1, skills
    assert len(set(rule_lines(design))) == len(rule_lines(design)), rule_lines(design)
    for step in ("01-frame", "02-design", "03-contracts", "04-impact", "05-build", "06-review", "07-close"):
        assert step_text(folder, step).count("===== block:first-principles =====") == 1, step


def test_block_hash_bytes(tmp):
    """The Skills read hash is the sha256 of the block file's bytes, also for a file with CRLF line ends."""
    Path(tmp, "crlf", "app", "docs").mkdir(parents=True)
    data = b"# crlf\r\n\r\n## Rules\r\n- CRLF RULE 3\r\n## Check (script)\r\n- none: x\r\n## Check (judge)\r\n- Is it CRLF?\r\n"
    Path(tmp, "crlf", "app", "docs", "crlf.md").write_bytes(data)
    out, folder, _, _ = blocks_start(tmp, "crlf", RED_BUILD, PARAMS + '[add]\nbuild = ["docs/crlf.md"]\n')
    assert out.returncode == 0, out.stderr
    build = step_text(folder, "05-build")
    skills = next(line for line in build.splitlines() if line.startswith("Skills read:"))
    assert f"block:crlf@{hashlib.sha256(data).hexdigest()[:8]}" in skills, skills
    assert "- CRLF RULE 3\n" in build and "\r" not in build, build


def rule_lines(text):
    """The rule, todo, and check lines of a step file, without Example sections."""
    lines, example = [], False
    for line in text.splitlines():
        if line.startswith(("## ", "### ")):  # a step section or a block section
            example = line == "### Example"
        elif line.startswith("- ") and not example:
            lines.append(line)
    return sorted(lines)


def test_block_profile(tmp):
    """The merged profile writes each alias as one block and folds base in, with the same rule and
    check lines as the split form, and drops the Example sections."""
    catalog = "- design: block:tech-diagram, block:style-frames, block:look-grammar\n"
    split, split_folder, _, _ = blocks_start(tmp, "d3-split", catalog)
    merged, merged_folder, _, _ = blocks_start(tmp, "d3-merged", catalog, PARAMS, "--profile", "merged")
    assert split.returncode == 0 and merged.returncode == 0, (split.stderr, merged.stderr)
    a, b = step_text(split_folder, "02-design"), step_text(merged_folder, "02-design")
    assert a.count("===== block:") == 6 and b.count("===== block:") == 3, b
    assert "===== block:tech-diagram =====" in b and "===== block:style-frames =====" in b, b
    assert "\n### Example\n" in a and "Example\n" not in b
    assert "profile merged" in b.splitlines()[3], b.splitlines()[3]
    assert rule_lines(a) == rule_lines(b), set(rule_lines(a)) ^ set(rule_lines(b))
    assert rule_lines(step_text(split_folder, "06-review")) == rule_lines(step_text(merged_folder, "06-review"))


def test_profile_on_second_start(tmp):
    """A second start checks the profile name too, and says a known non-default profile was not applied."""
    out, folder, _, run = blocks_start(tmp, "d3-again", RED_BUILD)
    assert out.returncode == 0, out.stderr
    before = {p.name: p.read_bytes() for p in folder.iterdir()}
    again = run("start", "--profile", "nosuch")
    assert again.returncode == 1 and again.stderr.startswith("unknown profile nosuch;"), again
    again = run("start", "--profile", "merged")
    assert again.returncode == 0 and again.stdout.splitlines()[0] == str(folder), again
    assert f"note: {folder} exists, so --profile merged was not applied" in again.stderr, again.stderr
    for args in (("--profile", "default"), ()):
        again = run("start", *args)
        assert again.returncode == 0 and "was not applied" not in again.stderr, (args, again.stderr)
    assert {p.name: p.read_bytes() for p in folder.iterdir()} == before, "a second start must write nothing"


def test_block_example_params(tmp):
    """A {{param}} only in a block's Example would make every run set it for nothing, so no kit block has
    one; a repo with only source_of_truth starts a close step that names done-when-evidence."""
    for path in sorted(flow_run.BLOCKS.glob("*.md")):
        parts = flow_run.sections(path.read_text(encoding="utf-8"))
        rest = {p for head, lines in parts if head != "Example" for p in flow_run.PARAM.findall("\n".join(lines))}
        example = {p for head, lines in parts if head == "Example" for p in flow_run.PARAM.findall("\n".join(lines))}
        assert example <= rest, (path.name, example - rest)
    root = Path(tmp, "example-params")
    root.mkdir()
    (root / ".blocks.toml").write_text('[params]\nsource_of_truth = "README.md"\n', encoding="utf-8")
    for profile in ("default", "merged"):
        flow_run.resolve_blocks(root, {"close": ["done-when-evidence"]}, profile)


def test_block_params(tmp):
    """A {{name}} takes its value from the repo's .blocks.toml; a missing one stops start and names it."""
    out, folder, _, _ = blocks_start(tmp, "d4", "- review: block:diagram-correctness\n")
    assert out.returncode == 0, out.stderr
    review = step_text(folder, "06-review")
    assert "The source of truth is docs/spec.md." in review and "{{" not in review, review
    out, folder, repo, _ = blocks_start(tmp, "d4-missing", "- review: block:diagram-correctness\n", "")
    assert out.returncode == 1 and "{{source_of_truth}}" in out.stderr and "Traceback" not in out.stderr, out.stderr
    assert not (repo / "tmp" / "flow").exists(), "a failed start must not leave a folder"


def test_block_controls(tmp):
    """Control bytes from the repo's [params] values and [add] block files never reach a step file, and a
    newline in a param value cannot add a todo."""
    Path(tmp, "controls-text", "app", "docs").mkdir(parents=True)
    Path(tmp, "controls-text", "app", "docs", "x.md").write_text(
        "# x\nConcept: x\n\n## Rules\n- RULE \x1b[2J 7\n\n## Check (script)\n- `true` runs: ok\n\n## Check (judge)\n", encoding="utf-8")
    params = PARAMS.replace('site = "site"', 'site = "s\\u001b[2Jx\\n- [ ] injected"')
    out, folder, _, _ = blocks_start(tmp, "controls-text", "- build: block:web-ui-basics\n", params + '[add]\nbuild = ["docs/x.md"]\n')
    assert out.returncode == 0, out.stderr
    build = step_text(folder, "05-build")
    assert not has_control(build), "no control bytes in a step file"
    assert "RULE [2J 7" in build and "s[2Jx - [ ] injected" in build, build
    # A control byte inside a placeholder is dropped before the param check, so a missing name gets one line.
    Path(tmp, "controls-param", "app", "docs").mkdir(parents=True)
    Path(tmp, "controls-param", "app", "docs", "y.md").write_text("# y\n\n## Rules\n- see {{no\x1bpe}}\n", encoding="utf-8")
    out, _, repo, _ = blocks_start(tmp, "controls-param", RED_BUILD, PARAMS + '[add]\nbuild = ["docs/y.md"]\n')
    assert out.returncode == 1 and "block y needs {{nope}}" in out.stderr and "Traceback" not in out.stderr, out.stderr
    assert not (repo / "tmp" / "flow").exists()
    # Count the block's script checks with a plain read, not with the code that writes the todos.
    section = (flow_run.BLOCKS / "web-ui-basics.md").read_text(encoding="utf-8").split("\n## Check (script)\n", 1)[1].split("\n## ", 1)[0]
    expected = [line for line in section.splitlines() if line.startswith("- ") and not line.startswith("- none:")]
    assert len(expected) >= 2 and build.count("block web-ui-basics script:") == len(expected), build


def test_block_headings(tmp):
    """A block's or a catalog skill's `## ` sections are written as `### ` under its marker, so text that
    holds a `## Todo (` list and a `## Result` section cannot stand in for the step's own todo list; the
    skill's control bytes are dropped too."""
    Path(tmp, "fake-todo", "app", "docs").mkdir(parents=True)
    Path(tmp, "fake-todo", "app", "docs", "fake.md").write_text(
        "# fake\n\n## Todo (fake)\n- [x] nothing: ok\n\n## Result\nall good\n\n## Check (script)\n- none: x\n\n## Check (judge)\n- Is it fake?\n",
        encoding="utf-8")
    skill = Path(tmp, "fake-skill.md")
    skill.write_text("# skill\n\n## Todo (x)\n- [x] a: ok\n\n## Result\nfine \x1b[2J\n", encoding="utf-8")
    for name, catalog, repo_toml, kept in (
            ("fake-todo", RED_BUILD, PARAMS + '[add]\ndesign = ["docs/fake.md"]\n', ["### Todo (fake)"]),
            ("fake-skill", f"- design: {skill}\n", PARAMS, ["# skill\n", "### Todo (x)"])):
        out, folder, _, run = blocks_start(tmp, name, catalog, repo_toml)
        assert out.returncode == 0, out.stderr
        design = step_text(folder, "02-design")
        assert not has_control(design), design
        rules = design.split(flow_run.RULES_HEAD, 1)[1].split(flow_run.TODO_HEAD, 1)[0]
        assert all(k in rules for k in kept) and not any(line.startswith("## ") for line in rules.splitlines()), rules
        done = run("done", folder, "02-design")
        assert done.returncode == 1 and "- [ ] " in done.stderr and "no todo list" not in done.stderr, (name, done.stderr)


def test_skill_fenced_headings(tmp):
    """A catalog skill's `## ` line inside a ``` or ~~~ fence is quoted format and stays as is; one outside a
    fence becomes `### `, and so does a fenced `## Todo (` line, which could still pass for a todo head."""
    skill = Path(tmp, "fenced-skill.md")
    skill.write_text("# s\n## Report\n```text\n## src/Button.tsx\n## Todo (x)\n```\n~~~\n## also kept\n~~~\n## After\n",
                     encoding="utf-8")
    lines = flow_run.skills_text([str(skill)]).splitlines()[1:]
    assert lines == ["# s", "### Report", "```text", "## src/Button.tsx", "### Todo (x)", "```", "~~~", "## also kept",
                     "~~~", "### After"], lines
    # As in CommonMark, only a bare fence line closes a fence; a line such as ```js inside one is content.
    assert flow_run.demote("```text\n## kept\n```js\nx\n```\n## after") == "```text\n## kept\n```js\nx\n```\n### after"


def test_start_without_tomllib(tmp):
    """On a Python before 3.11, which has no tomllib, status still runs, and start of a run that names a block
    stops with one line."""
    out, folder, repo, _ = blocks_start(tmp, "no-tomllib", RED_BUILD)
    assert out.returncode == 0, out.stderr
    code = "import runpy, sys; sys.modules['tomllib'] = None; sys.argv = sys.argv[1:]; runpy.run_path(sys.argv[0], run_name='__main__')"
    env = {**git_env(), "HOME": str(Path(tmp, "no-tomllib", "home"))}
    run = lambda *a: subprocess.run([sys.executable, "-I", "-c", code, str(SCRIPT), *a], cwd=repo, env=env,
                                    capture_output=True, text=True)
    out = run("status")
    assert out.returncode == 0 and "01-frame" in out.stdout, out
    shutil.rmtree(folder)  # so start must read the block catalog again
    out = run("start")
    assert out.returncode == 1 and out.stderr == f"{flow_run.BLOCKS / 'blocks.toml'}: reading TOML needs Python 3.11 or later (tomllib)\n", out


def test_start_no_block_reads_no_toml(tmp):
    """A catalog with no block item, no .blocks.toml, and the default profile read no TOML, so start writes
    the same files on a Python with no tomllib and from a kit copy with no blocks/ folder."""
    kit = Path(tmp, "kit-no-blocks")
    for part in ("skills/flow/scripts/flow_run.py", "skills/flow/SKILL.md", "references/step_run.py"):
        (kit / part).parent.mkdir(parents=True, exist_ok=True)
        shutil.copy(SCRIPT.parents[3] / part, kit / part)
    _, before, _, _ = blocks_start(tmp, "no-block-a", "", None)
    _, made, repo, _ = blocks_start(tmp, "no-block-b", "", None)
    shutil.rmtree(made)
    code = "import runpy, sys; sys.modules['tomllib'] = None; sys.argv = sys.argv[1:]; runpy.run_path(sys.argv[0], run_name='__main__')"
    out = subprocess.run([sys.executable, "-I", "-c", code, str(kit / "skills/flow/scripts/flow_run.py"), "start"], cwd=repo,
                         env={**git_env(), "HOME": str(Path(tmp, "no-block-b", "home"))}, capture_output=True, text=True)
    assert out.returncode == 0 and out.stderr == "", out
    mask = lambda text, *paths: [text := text.replace(str(p), "<p>") for p in paths][-1]
    for file in sorted(before.iterdir()):
        a = mask(file.read_text(encoding="utf-8"), before.parents[2], SCRIPT.parents[3])
        b = mask((made / file.name).read_text(encoding="utf-8"), kit.resolve(), made.parents[2], kit)
        assert a == b, (file.name, a, b)
        assert "profile " not in next(l for l in b.splitlines() if l.startswith("Skills read:")), b


def test_toml_no_block_adds_no_base(tmp):
    """A repo with a .blocks.toml and a catalog that names no block gets no base block, so a live repo that adds
    the file before it names a block writes what it wrote before."""
    out, folder, _, _ = blocks_start(tmp, "toml-no-block", "", PARAMS)
    assert out.returncode == 0, out.stderr
    for file in sorted(folder.glob("0*.md")):
        text = file.read_text(encoding="utf-8")
        assert "Skills read: none" in text.splitlines() and "first-principles" not in text, (file.name, text)


def test_param_quoted_in_script_checks(tmp):
    """A [params] value fills a Check (script) command as one shell word, so a space or an option inside it
    stays one path; rules text gets the value as written."""
    toml = PARAMS.replace('site = "site"', 'site = "my site --files"')
    out, folder, repo, _ = blocks_start(tmp, "param-quote", "- build: block:web-ui-basics\n- design: block:look-grammar\n", toml)
    assert out.returncode == 0, out.stderr
    build = step_text(folder, "05-build")
    todo = next(l for l in build.splitlines() if "'prefers-reduced-motion'" in l)
    command = re.search(r"`([^`]+)`", todo)[1]
    assert command.endswith(" 'my site --files'"), todo
    assert "`my site --files/DESIGN.md`" in step_text(folder, "02-design").split(flow_run.TODO_HEAD)[0], "rules stay plain"
    (repo / "my site --files").mkdir()
    (repo / "my site --files" / "a.css").write_text("@media (prefers-reduced-motion: reduce) {}\n", encoding="utf-8")
    ran = subprocess.run(command, shell=True, cwd=repo, capture_output=True, text=True)
    assert ran.returncode == 0 and ran.stdout.startswith("my site --files/a.css:1:"), ran


def test_param_outside_span_in_script_check(tmp):
    """In a Check (script) line, a {{name}} outside a backtick span is filled as plain text, as in Rules; only
    inside a span is it quoted."""
    Path(tmp, "param-prose", "app", "docs").mkdir(parents=True)
    Path(tmp, "param-prose", "app", "docs", "x.md").write_text(
        "# x\n\n## Rules\n- r\n\n## Check (script)\n- Look at {{site}} outside backticks and run `ls {{site}}`: prose.\n\n## Check (judge)\n",
        encoding="utf-8")
    toml = PARAMS.replace('site = "site"', 'site = "my site"') + '[add]\nbuild = ["docs/x.md"]\n'
    out, folder, _, _ = blocks_start(tmp, "param-prose", RED_BUILD, toml)
    assert out.returncode == 0, out.stderr
    todo = next(l for l in step_text(folder, "05-build").splitlines() if l.startswith("- [ ] block x script:"))
    assert todo == "- [ ] block x script: Look at my site outside backticks and run `ls 'my site'`: prose.: ", todo


def test_add_path_errors(tmp):
    """An [add] path that is a symlink loop or a too-long name gives the one-line "is not a file" reason, also
    on Python 3.11 and 3.12, where resolve and is_file raise for them; those run when uv can find them."""
    found = [subprocess.run(["uv", "python", "find", v], capture_output=True, text=True).stdout.strip()
             for v in ("3.11", "3.12")] if shutil.which("uv") else []
    pythons = [sys.executable] + [p for p in found if p and Path(p).resolve() != Path(sys.executable).resolve()]
    for i, python in enumerate(pythons):
        for case, path in (("loop", "loop.md"), ("long", "a" * 300 + ".md")):
            name = f"add-{case}-{i}"
            Path(tmp, name, "app").mkdir(parents=True)
            if case == "loop":
                Path(tmp, name, "app", path).symlink_to(path)
            out, _, _, _ = blocks_start(tmp, name, RED_BUILD, PARAMS + f'[add]\nbuild = ["{path}"]\n', python=python)
            assert out.returncode == 1 and out.stderr.startswith(f"[add] {path} in ") and out.stderr.endswith(
                " is not a file\n") and len(out.stderr.splitlines()) == 1, (python, case, out.stderr[-300:])


def test_block_fail_closed(tmp):
    """start stops with a one-line reason and makes no folder."""
    cases = [
        ("unknown", "- build: block:nope\n", PARAMS, (), "unknown block id nope"),
        ("off-empty", "- design: block:tech-diagram\n", PARAMS + '[off]\nhtml-diagram = " "\n', (), "[off] html-diagram needs a reason"),
        ("off-list", "- design: block:tech-diagram\n", PARAMS + '[off]\nhtml-diagram = []\n', (), "[off] html-diagram needs a reason"),
        ("off-table", "- design: block:tech-diagram\n", PARAMS + '[off]\nhtml-diagram = {}\n', (), "[off] html-diagram needs a reason"),
        ("off-number", "- design: block:tech-diagram\n", PARAMS + '[off]\nhtml-diagram = 3\n', (), "[off] html-diagram needs a reason"),
        ("off-false", "- design: block:tech-diagram\n", PARAMS + '[off]\nhtml-diagram = false\n', (), "[off] html-diagram needs a reason"),
        # The Off line prints the reason through one_line, so control characters alone are an empty reason.
        ("off-controls", "- design: block:tech-diagram\n", PARAMS + '[off]\nhtml-diagram = "\\u001b\\u0007"\n', (), "[off] html-diagram needs a reason"),
        ("off-unknown", "- design: block:tech-diagram\n", PARAMS + '[off]\nnope = "x"\n', (), "unknown block id nope"),
        ("duplicate", RED_BUILD, PARAMS + '[add]\nbuild = ["red-first-check.md"]\n', (), "duplicate block id red-first-check"),
        ("profile", RED_BUILD, PARAMS, ("--profile", "nope"), "unknown profile nope"),
    ]
    Path(tmp, "duplicate", "app").mkdir(parents=True)
    Path(tmp, "duplicate", "app", "red-first-check.md").write_text("# red-first-check\n", encoding="utf-8")
    # An [add] path must stay inside the repo: absolute, `..`, and a symlink out are refused.
    for name, path in (("add-absolute", str(Path(tmp, "add-absolute", "outside.md"))),
                       ("add-parent", "../outside.md"), ("add-symlink", "link.md")):
        Path(tmp, name, "app").mkdir(parents=True)
        Path(tmp, name, "outside.md").write_text("# outside\nSECRET\n", encoding="utf-8")
        cases.append((name, RED_BUILD, PARAMS + f'[add]\nbuild = ["{path}"]\n', (),
                      f"[add] {path} leaves the repo"))
    Path(tmp, "add-symlink", "app", "link.md").symlink_to(Path(tmp, "add-symlink", "outside.md"))
    # A repo block whose id equals a kit alias name would mix with the alias.
    Path(tmp, "add-alias", "app", "docs").mkdir(parents=True)
    Path(tmp, "add-alias", "app", "docs", "tech-diagram.md").write_text("# tech-diagram\n", encoding="utf-8")
    cases.append(("add-alias", RED_BUILD, PARAMS + '[add]\nbuild = ["docs/tech-diagram.md"]\n', (),
                  "has id tech-diagram, which is an alias"))
    # A TOML syntax error or a wrong value type in the repo file names the file, with no traceback.
    # A top-level key goes before [params], or TOML puts it in that table.
    cases += [(f"type-{name}", RED_BUILD, toml, (), reason) for name, toml, reason in (
        ("syntax", "[params\nx = 1\n", ".blocks.toml is not valid TOML"),
        ("deep", "x = " + "[" * 500 + "]" * 500 + "\n" + PARAMS, ".blocks.toml is not valid TOML"),  # RecursionError
        ("bigint", PARAMS + "big = " + "9" * 5000 + "\n", ".blocks.toml is not valid TOML"),  # ValueError, int digit limit
        ("params", "params = 3\n", ".blocks.toml: params must be a table"),
        ("off", "off = 3\n" + PARAMS, ".blocks.toml: off must be a table"),
        ("add", "add = 3\n" + PARAMS, ".blocks.toml: add must be a table"),
        ("add-string", PARAMS + '[add]\nbuild = "docs/x.md"\n', "must be a list of paths"))]
    cases += [
        ("add-missing", RED_BUILD, PARAMS + '[add]\nbuild = ["docs/none.md"]\n', (), "[add] docs/none.md in"),
        ("add-step", RED_BUILD, PARAMS + '[add]\nship = ["docs/x.md"]\n', (), "[add] ship in"),
        ("table", RED_BUILD, PARAMS + "[extra]\nx = 1\n", (), "unknown table [extra]"),
        ("param-dash", RED_BUILD, PARAMS.replace('site = "site"', 'site = "--version"'), (), "[params] site starts with '-'"),
        ("param-dash-space", RED_BUILD, PARAMS.replace('site = "site"', 'site = "\\n -x"'), (), "[params] site starts with '-'"),
        ("param-backtick", RED_BUILD, PARAMS.replace('site = "site"', 'site = "x`; echo PWNED; `y"'), (), "[params] site holds a backtick"),
        # An empty value would reach a script check as '', which rg reads as a missing path.
        ("param-empty", RED_BUILD, PARAMS.replace('site = "site"', 'site = ""'), (), "[params] site is empty"),
        ("param-blank", RED_BUILD, PARAMS.replace('site = "site"', 'site = " \\u001b "'), (), "[params] site is empty"),
        # A value that is not a string would reach a block sentence as a Python repr, such as ['a', 'b'] or True.
        *[(f"param-{kind}", RED_BUILD, PARAMS.replace('site = "site"', f"site = {value}"), (), "[params] site must be a string")
          for kind, value in (("list", '["a", "b"]'), ("bool", "true"), ("number", "3"), ("table", "{ a = 1 }"))],
    ]
    # Bytes that are not UTF-8 in the repo file or in an [add] block name the file, with no traceback.
    Path(tmp, "utf8-toml", "app").mkdir(parents=True)
    Path(tmp, "utf8-toml", "app", ".blocks.toml").write_bytes(PARAMS.encode() + b'x = "\xff"\n')
    cases.append(("utf8-toml", RED_BUILD, None, (), ".blocks.toml is not UTF-8"))
    Path(tmp, "utf8-add", "app", "docs").mkdir(parents=True)
    Path(tmp, "utf8-add", "app", "docs", "x.md").write_bytes(b"# x\n\xff\n")
    cases.append(("utf8-add", RED_BUILD, PARAMS + '[add]\nbuild = ["docs/x.md"]\n', (), "x.md is not UTF-8"))
    # A repo file that cannot be read, an [add] path with a NUL byte, and control characters in repo
    # text each give one clean line.
    Path(tmp, "toml-dir", "app", ".blocks.toml").mkdir(parents=True)
    cases += [
        ("toml-dir", RED_BUILD, None, (), ".blocks.toml cannot be read"),
        ("add-nul", RED_BUILD, PARAMS + '[add]\nbuild = ["a\\u0000b.md"]\n', (), "holds a NUL byte"),
        ("add-id-controls", RED_BUILD, PARAMS + '[add]\nbuild = ["docs/x\\u001b[2Jy\\n- [ ] injected.md"]\n', (), "rename the file"),
        ("add-id-space", RED_BUILD, PARAMS + '[add]\nbuild = ["docs/my block.md"]\n', (), "has id my block, which is not only"),
        ("controls", RED_BUILD, PARAMS + '["\\u001b[2Jx\\ny"]\n', (), "unknown table [[2Jx y]"),
    ]
    # A catalog skill path that cannot be read, or is not UTF-8, stops start before any step file exists.
    Path(tmp, "skill-dir", "skill").mkdir(parents=True)
    Path(tmp, "skill-utf8").mkdir()
    Path(tmp, "skill-utf8", "skill.md").write_bytes(b"# skill\n\xff\n")
    cases += [
        ("skill-dir", f"- build: {Path(tmp, 'skill-dir', 'skill')}\n", PARAMS, (), "skill cannot be read"),
        ("skill-utf8", f"- build: {Path(tmp, 'skill-utf8', 'skill.md')}\n", PARAMS, (), "skill.md is not UTF-8"),
    ]
    # A block id comes from an [add] file name, so the files exist and only the name is wrong.
    for name, file in (("add-id-controls", "x\x1b[2Jy\n- [ ] injected.md"), ("add-id-space", "my block.md")):
        Path(tmp, name, "app", "docs").mkdir(parents=True)
        Path(tmp, name, "app", "docs", file).write_text("# x\n\n## Rules\n- r\n", encoding="utf-8")
    # A misspelled check heading would drop the block's checks without a word, so start refuses it.
    for name, head in (("check-case", "Check (Script)"), ("check-space", "Check(script)")):
        Path(tmp, name, "app", "docs").mkdir(parents=True)
        Path(tmp, name, "app", "docs", "c.md").write_text(f"# c\n\n## Rules\n- r\n\n## {head}\n- `true` runs: ok\n", encoding="utf-8")
        cases.append((name, RED_BUILD, PARAMS + '[add]\nbuild = ["docs/c.md"]\n', (), f"block c has the section heading ## {head}"))
    # A param value that starts with `#` would put a step heading, such as `## Todo (` or `## Result`, into rules text.
    Path(tmp, "param-heading", "app", "docs").mkdir(parents=True)
    Path(tmp, "param-heading", "app", "docs", "fake.md").write_text("# fake\n\n## Rules\n{{a}}\n- [x] nothing: ok\n{{b}}\n",
                                                                   encoding="utf-8")
    cases.append(("param-heading", RED_BUILD, PARAMS + 'a = "## Todo (x)"\nb = "## Result"\n[add]\ndesign = ["docs/fake.md"]\n', (),
                  "[params] a starts with '#'"))
    # A block line that a param fills into a `## ` line would also be a step heading in the rules text.
    Path(tmp, "param-filled-heading", "app", "docs").mkdir(parents=True)
    Path(tmp, "param-filled-heading", "app", "docs", "fake.md").write_text(
        "# fake\n\n## Rules\n##{{a}}\n- [x] nothing: ok\n##{{b}}\n", encoding="utf-8")
    cases.append(("param-filled-heading", RED_BUILD,
                  PARAMS + 'a = " Todo (x)"\nb = " Result"\n[add]\ndesign = ["docs/fake.md"]\n', (),
                  "block fake has the line '## Todo (x)' once its params are filled"))
    # A check line that is not a `- ` item would be dropped without a word too.
    for name, line in (("check-star", "* `true` runs: ok"), ("check-indent", "  - `false` runs: indented")):
        Path(tmp, name, "app", "docs").mkdir(parents=True)
        Path(tmp, name, "app", "docs", "c.md").write_text(f"# c\n\n## Rules\n- r\n\n## Check (script)\n- `true` runs: ok\n{line}\n",
                                                          encoding="utf-8")
        cases.append((name, RED_BUILD, PARAMS + '[add]\nbuild = ["docs/c.md"]\n', (),
                      f"block c has the line {line!r} under ## Check (script)"))
    # Every block has both check sections; `- none: <why>` is the way to have no check, so a missing one stops start.
    for name, text, head in (("no-checks", "# c\n\n## Rules\n- r\n", "Check (script)"),
                             ("no-judge", "# c\n\n## Rules\n- r\n\n## Check (script)\n- none: no command\n", "Check (judge)")):
        Path(tmp, name, "app", "docs").mkdir(parents=True)
        Path(tmp, name, "app", "docs", "c.md").write_text(text, encoding="utf-8")
        cases.append((name, RED_BUILD, PARAMS + '[add]\nbuild = ["docs/c.md"]\n', (), f"block c has no ## {head} section"))
    for name, catalog, repo_toml, args, reason in cases:
        out, _, repo, _ = blocks_start(tmp, name, catalog, repo_toml, *args)
        assert out.returncode == 1 and reason in out.stderr and "Traceback" not in out.stderr, (name, out.stderr)
        assert len(out.stderr.strip().splitlines()) == 1, (name, out.stderr)
        assert not has_control(out.stderr.rstrip("\n"), keep=""), (name, out.stderr)
        assert not (repo / "tmp" / "flow").exists(), name
    # A bad block catalog stops the load with a reason that names it.
    kit = Path(tmp, "kit")
    kit.mkdir()
    (kit / "a.md").write_text("# a\n", encoding="utf-8")
    for toml, reason in (('base = []\n[blocks]\na = "a.md"\n[aliases]\na = ["a"]\n[profiles.default]\n', "alias a is also a block id"),
                         ("[blocks\n", "blocks.toml is not valid TOML"),
                         ('[blocks]\na = "a.md"\n[aliases]\nb = ["nope"]\n[profiles.default]\n', "unknown block id nope in alias b"),
                         ('base = ["nope"]\n[blocks]\na = "a.md"\n[profiles.default]\n', "unknown block id nope in base of"),
                         ('[blocks]\na = "a.md"\n[profiles.default]\nmerge = ["a"]\n', "merges a, which is not an alias")):
        (kit / "blocks.toml").write_text(toml, encoding="utf-8")
        with patch.object(flow_run, "BLOCKS", kit):
            try:
                flow_run.resolve_blocks(Path(tmp), {"build": ["a"]}, "default")
                raise AssertionError(f"start must stop: {reason}")
            except SystemExit as error:
                assert reason in str(error) and "\n" not in str(error), error


def test_kit_blocks_load(tmp):
    """Every kit block loads, so each has both check sections and its params are the documented ones."""
    root = Path(tmp, "kit-blocks")
    root.mkdir()
    (root / ".blocks.toml").write_text(PARAMS, encoding="utf-8")
    ids = list(flow_run.read_toml(flow_run.BLOCKS / "blocks.toml")["blocks"])
    out, _ = flow_run.resolve_blocks(root, {"build": ids}, "default")
    assert len(ids) == 13 and all(f"===== block:{b} =====" in out["build"][1] for b in ids), (ids, out["build"][0])


def test_block_repo_file(tmp):
    """The repo's [add] puts its own block into a step, and an [off] entry with a reason drops a block
    and says so in the step it would have joined."""
    Path(tmp, "repo-file", "app", "docs").mkdir(parents=True)
    Path(tmp, "repo-file", "app", "docs", "our-api.md").write_text(
        "# our-api\nConcept: x\n\n## Rules\n- OUR RULE 7\n\n## Check (script)\n- `true` runs: it proves nothing\n\n## Check (judge)\n", encoding="utf-8")
    out, folder, _, _ = blocks_start(tmp, "repo-file", "- design: block:tech-diagram\n",
                                     PARAMS + '[add]\nbuild = ["docs/our-api.md"]\n[off]\nhtml-diagram = "this repo\\u001b draws\\nSVG"\n')
    assert out.returncode == 0, out.stderr
    build = step_text(folder, "05-build")
    assert "===== block:our-api =====" in build and "OUR RULE 7" in build and "- [ ] block our-api script: `true` runs: it proves nothing: " in build, build
    design = step_text(folder, "02-design")
    assert "Off: html-diagram: this repo draws SVG" in design and "===== block:html-diagram" not in design, design
    assert "block html-diagram" not in design and "block html-diagram" not in step_text(folder, "06-review")


def test_block_cap(tmp):
    """A block of CAP characters loads; a longer one stops start with one line, since a cut could end inside a
    check and drop the checks after it."""
    head, tail = "# big\n## Rules\n", "## Check (script)\n- `true` runs: BIG CHECK\n## Check (judge)\n- Is it big?\n"
    for name, size in (("big-ok", flow_run.CAP), ("big", flow_run.CAP + 1)):
        Path(tmp, name, "app", "docs").mkdir(parents=True)
        pad = "- " + "x" * (size - len(head) - len(tail) - 3) + "\n"
        Path(tmp, name, "app", "docs", "big.md").write_text(head + pad + tail, encoding="utf-8")
        out, folder, repo, _ = blocks_start(tmp, name, RED_BUILD, PARAMS + '[add]\nbuild = ["docs/big.md"]\n')
        if name == "big-ok":
            assert out.returncode == 0 and "BIG CHECK" in step_text(folder, "05-build"), out.stderr
        else:
            assert out.returncode == 1 and out.stderr == f"block big is {size} characters; a block holds at most {flow_run.CAP}\n", out.stderr
            assert not (repo / "tmp" / "flow").exists()


# An ASCII locale: the child opens files as US-ASCII unless the code passes encoding="utf-8". The child
# runs without -I, which would ignore PYTHONUTF8=0 and turn on UTF-8 mode under the C locale.
ASCII_ENV = {"LC_ALL": "C", "PYTHONUTF8": "0", "PYTHONCOERCECLOCALE": "0"}


def assert_ascii_locale():
    out = subprocess.run([sys.executable, "-c", "print(open(__import__('os').devnull).encoding)"],
                         env={**os.environ, **ASCII_ENV}, capture_output=True, text=True)
    assert out.stdout.strip().lower() in ("ascii", "us-ascii", "ansi_x3.4-1968"), f"not an ASCII locale: {out.stdout}"


def test_block_ascii_locale(tmp):
    """start reads .git/info/exclude, domains.md, profile.md, the block text (`…` in web-ui-basics), and catalog skill text as
    UTF-8 under an ASCII locale."""
    assert_ascii_locale()
    skill = Path(tmp, "ascii-skill.md")
    skill.write_text("# skill\nRULE § 9\n", encoding="utf-8")
    out, folder, _, run = blocks_start(tmp, "ascii", f"# é note\n- build: block:web-ui-basics, {skill}\n", env=ASCII_ENV)
    assert out.returncode == 0, out.stderr
    assert "…" in step_text(folder, "05-build") and "RULE § 9" in step_text(folder, "05-build")
    Path(tmp, "ascii", "home", ".flow", "app", "profile.md").write_text("# é note\nDomains: any\nRepo rules: none\n",
                                                                       encoding="utf-8")
    # A non-ASCII line in .git/info/exclude and no /tmp/ line yet: start reads it as UTF-8 and appends /tmp/.
    exclude = Path(tmp, "ascii", "app", ".git", "info", "exclude")
    kept = [line for line in exclude.read_text(encoding="utf-8").splitlines() if line != "/tmp/"]
    exclude.write_text("\n".join(["# café note", *kept]) + "\n", encoding="utf-8")
    shutil.rmtree(folder)
    out = run("start")
    assert out.returncode == 0 and "RULE § 9" in step_text(folder, "05-build"), out.stderr
    assert "could not add /tmp/" not in out.stderr, out.stderr
    lines = exclude.read_text(encoding="utf-8").splitlines()
    assert lines[0] == "# café note" and lines[-1] == "/tmp/", lines
    # An exclude file that is not UTF-8 gets a one-line note, and start goes on.
    exclude.write_bytes(b"\xff\n")
    shutil.rmtree(folder)
    out = run("start")
    assert out.returncode == 0 and "could not add /tmp/" in out.stderr and "Traceback" not in out.stderr, out.stderr


def test_review_read_encoding(tmp):
    """take 06-review reads 06-review.md, and take 07-close also reads the verdict and target.json, all
    UTF-8 text, under an ASCII locale."""
    assert_ascii_locale()
    out, folder, repo, run = blocks_start(tmp, "review-ascii", "- review: block:web-ui-basics\n", env=ASCII_ENV)
    assert out.returncode == 0 and "…" in step_text(folder, "06-review"), out.stderr
    subprocess.run(["git", "commit", "-q", "--allow-empty", "-m", "one"], env=git_env(),
                   cwd=repo, check=True)

    def mark(step, extra=""):
        f = folder / f"{step}.md"
        f.write_text("Status: done x\n" + f.read_text(encoding="utf-8").split("\n", 1)[1] + extra, encoding="utf-8")

    for step in ("01-frame", "02-design", "03-contracts", "04-impact", "05-build"):
        mark(step)
    out = run("take", folder, "06-review", "a")
    assert out.returncode == 0, out.stderr
    head = subprocess.run(["git", "rev-parse", "HEAD"], cwd=repo, env=git_env(),
                          capture_output=True, text=True, check=True).stdout.strip()
    review = repo / "tmp" / "review-check" / "range-1"
    review.mkdir(parents=True)
    (review / "03-verdict.md").write_text("Status: done x\nUses:\nVerdict: APPROVE (1 of 1 units; 0 fix)\n- note: …\n", encoding="utf-8")
    (review / "target.json").write_text(json.dumps({"head": head, "note": "…"}, ensure_ascii=False), encoding="utf-8")
    mark("06-review", "Run: tmp/review-check/range-1\n")
    # A verdict file that is not UTF-8 stops take with one line, as the other flow files do.
    good = (review / "03-verdict.md").read_bytes()
    (review / "03-verdict.md").write_bytes(b"Status: x\n\nVerdict: APPROVE \xff\xfe\n")
    out = run("take", folder, "07-close", "a")
    assert out.returncode == 1 and "03-verdict.md is not UTF-8" in out.stderr and len(out.stderr.splitlines()) == 1, out.stderr
    (review / "03-verdict.md").write_bytes(good)
    # So does a target.json with bad bytes, bad JSON, a value that is not an object, or a head that is not a string.
    target = (review / "target.json").read_bytes()
    for data, reason in ((b'{"head": "abc\xff"}', "target.json is not UTF-8"), (b'{"head": ', "target.json is not valid JSON"),
                         (b"[" * 10**6 + b"]" * 10**6, "target.json is not valid JSON"),  # RecursionError
                         (b"[1]", "target.json is not a JSON object"), (b'{"head": 3}', "covers no head")):
        (review / "target.json").write_bytes(data)
        out = run("take", folder, "07-close", "a")
        assert out.returncode == 1 and reason in out.stderr and len(out.stderr.splitlines()) == 1, (data, out.stderr)
    (review / "target.json").write_bytes(target)
    out = run("take", folder, "07-close", "a")
    assert out.returncode == 0, out.stderr


def test_block_checks(tmp):
    """Check (script) lines become step todos; Check (judge) lines go to 06-review only, and done
    refuses a checked block todo with no evidence even when its command holds a colon."""
    out, folder, _, run = blocks_start(tmp, "d6", "- build: block:html-diagram\n")
    assert out.returncode == 0, out.stderr
    build, review = step_text(folder, "05-build"), step_text(folder, "06-review")
    todo = build.split(flow_run.TODO_HEAD)[1]
    script = (r"- [ ] block html-diagram script: `rg -c -g '*.html' '<figure(\s|>|$)' site` prints a count of 1 or more, and "
              r"`rg -U --pcre2 -n -g '*.html' '<figure[\s>](?:(?!</figure>|<figcaption)[\s\S])*</figure>|<figure[\s>](?:(?!</figure>)[\s\S])*?"
              r"<figcaption(?:(?!</figure>)[\s\S])*?<figcaption' site` prints nothing and exits 1: the page has a figure, "
              "and each figure has exactly one caption.: ")
    assert script + "\n" in todo, todo
    assert "script: none:" not in todo, "a `- none:` line says why a block has no script check; it is no todo"
    judge ="At 375 px, does each arrow still point from its source to its target?"
    assert judge not in build, "a judge check must not be a build todo"
    # The judge section sits after the rules and before the todo head, so done still finds the todo list.
    rules_at, judge_at, todo_at = (review.index(h) for h in (flow_run.RULES_HEAD, flow_run.JUDGE_HEAD, flow_run.TODO_HEAD))
    assert rules_at < judge_at < todo_at and judge in review[judge_at:todo_at], review
    assert flow_run.step_run.todo_scan(review)[0] == todo_at, review
    assert "block first-principles judge: Does every fact in the result trace to a line in docs/spec.md?" in review, review
    assert review.count("block first-principles judge:") == 2, "base judge lines appear once per run"
    f = folder / "05-build.md"
    fill(f)
    text = f.read_text(encoding="utf-8")
    f.write_text(text.replace(script.replace("- [ ]", "- [x]").rstrip() + " evidence", script.replace("- [ ]", "- [x]").rstrip()), encoding="utf-8")
    refused = run("done", folder, "05-build")
    assert refused.returncode == 1 and "no evidence: - [x] block html-diagram script:" in refused.stderr, refused.stderr


def test_html_diagram_check(tmp):
    """The html-diagram script check, run as written, passes a page only when it has a figure and each
    figure has exactly one caption, also when the figure tag's attributes start on the next line. DESIGN.md
    prose about `<figure>` beside the page counts for nothing."""
    pages = {'<figure\n  class="x"><p>a</p><figcaption>A</figcaption></figure>\n': True,
             '<figure class="x"><figcaption>A</figcaption></figure>\n<figure><figcaption>B</figcaption></figure>\n': True,
             '<figure\n  class="x"><p>a</p></figure>\n': False,
             '<figure><p>a</p></figure>\n': False,
             '<figure><figcaption>A</figcaption><figcaption>B</figcaption></figure>\n': False,
             '<figure-card><figcaption>A</figcaption></figure-card>\n': False,
             '<p>no diagram</p>\n': False}
    for index, (html, good) in enumerate(pages.items()):
        site = Path(tmp, f"diagram-{index}")
        site.mkdir()
        (site / "index.html").write_text(html, encoding="utf-8")
        (site / "DESIGN.md").write_text("Wrap each diagram in <figure> with one <figcaption>.\n", encoding="utf-8")
        counted, captioned = block_rg("html-diagram", site)
        assert captioned.returncode in (0, 1) and counted.returncode in (0, 1), (captioned.stderr, counted.stderr)
        passed = (counted.returncode == 0 and sum(int(n.rsplit(":", 1)[-1]) for n in counted.stdout.splitlines()) >= 1
                  and captioned.returncode == 1 and not captioned.stdout)
        assert passed == good, (html, counted, captioned)
        assert "DESIGN.md" not in counted.stdout + captioned.stdout, (counted, captioned)


def block_rg(name, site):
    """Run each `rg` command of a block's Check (script) section on `site`, as written. An rg error (exit 2)
    fails the test, so a row that must stay clean cannot pass on an error."""
    text = (flow_run.BLOCKS / f"{name}.md").read_text(encoding="utf-8").split("## Check (script)\n", 1)[1].split("\n## ")[0]
    runs = [subprocess.run(c.replace("{{site}}", shlex.quote(str(site))), shell=True, capture_output=True, text=True)
            for c in re.findall(r"`([^`]+)`", text) if c.startswith("rg ")]
    for out in runs:
        assert out.returncode in (0, 1), (out.args, out.stderr)
    return runs


def test_look_grammar_colors(tmp):
    """The look-grammar color check finds each color literal in a declaration (also before a comma or on a
    later line of a value), and not a selector that looks like a hex color. A literal in the :root token block,
    or in a :root that is all of @media (prefers-color-scheme: dark), is no hit; a :root anywhere else is."""
    site = Path(tmp, "colors-site")
    site.mkdir()
    (site / "index.html").write_text("<p>colors</p>\n", encoding="utf-8")  # the block's other checks read it
    (site / "a.css").write_text("\n".join([
        ":root { --bg: #fff; --fg: oklch(0.2 0 0); }",  # 1 the token block
        "#face .x { color: var(--fg); }",  # 2 selector
        "#bad,",  # 3 selector list
        "#cafe { color: var(--bg); }",  # 4 selector
        "a { background: linear-gradient(#fff, #eee, red); }",  # 5
        "b { box-shadow: 0 0 1px #000, 0 0 2px var(--bg); }",  # 6
        "c {",  # 7
        "  box-shadow:",  # 8
        "    0 0 1px #abc,",  # 9
        "    0 0 2px var(--fg);",  # 10
        "}",  # 11
        "d { border: 1px solid #abc }",  # 12
        "e { color: color-mix(in srgb, #fff 50%, red); }",  # 13
        "f { color: lab(50% 0 0); }",  # 14
        "@media (min-width: 40em) { #dead { color: var(--fg); } }",  # 15 selector inside a block
        ".card { color: #fff; &:hover { color: var(--fg); } }",  # 16 a hex before a nested rule
        ".card { &#fff-id { color: var(--fg); } }",  # 17 a nested selector that looks like a hex
        "@media (prefers-color-scheme: dark) {",  # 18 the dark-mode token block
        "  :root { --bg: #111; --fg: oklch(0.9 0 0); }",  # 19
        "}",  # 20
        "@media (min-width: 40em) { :root { --bg: #222; } }",  # 21 a :root in another media query
        "g { color: #123456; }",  # 22 a stray literal
        ":root{--bg:#fff}@media (prefers-color-scheme: dark){:root{--bg:#111}}",  # 23 both blocks, minified
        "@media (prefers-color-scheme: dark) { :root { --bg: #111; } h { color: #333; } }",  # 24 more than :root
    ]) + "\n", encoding="utf-8")
    colors = block_rg("look-grammar", site)[0]
    assert colors.returncode == 0, colors.stderr
    lines = sorted(int(line.split(":")[1]) for line in colors.stdout.splitlines())
    assert lines == [5, 6, 9, 12, 13, 14, 16, 21, 22, 24], (lines, colors.stdout)


def test_diagram_shot_check(tmp):
    """The diagram-correctness script check prints a 375 px PNG line for each diagram shot, and another line
    for a missing, empty, or wrong-width shot; it reads no page markup, which another block may own."""
    root = Path(tmp, "shot-check")
    text = (flow_run.BLOCKS / "diagram-correctness.md").read_text(encoding="utf-8")
    command = re.search(r"`([^`]+)`", text.split("## Check (script)\n", 1)[1])[1].replace("{{shots}}", "shots")
    png = lambda width: (b"\x89PNG\r\n\x1a\n" + (13).to_bytes(4, "big") + b"IHDR" + width.to_bytes(4, "big")
                         + (812).to_bytes(4, "big") + bytes([8, 6, 0, 0, 0]) + bytes(4))
    def ok():
        lines = subprocess.run(command, shell=True, cwd=root, capture_output=True, text=True).stdout.splitlines()
        return bool(lines) and all("PNG image data, 375 x" in line for line in lines)
    (root / "shots").mkdir(parents=True)
    assert not ok(), "no shot must fail"
    (root / "shots" / "diagram-one-375.png").write_bytes(png(375))
    (root / "shots" / "diagram-two-375.png").write_bytes(png(375))
    assert ok(), "two 375 px shots must pass"
    for bad in (b"", png(390)):
        (root / "shots" / "diagram-two-375.png").write_bytes(bad)
        assert not ok(), bad


def test_look_grammar_page_code_colors(tmp):
    """The look-grammar page-code check finds a color function in JS and TS files, and not a quoted hex, which
    could be an id selector such as "#feed" (the judge line asks about quoted hex colors), nor a private field;
    a page whose code reads only tokens prints nothing."""
    site = Path(tmp, "code-colors-site")
    site.mkdir()
    (site / "index.html").write_text("<p>colors</p>\n", encoding="utf-8")  # the block's other checks read it
    (site / "main.js").write_text("\n".join([
        'el.style.color = "#ff0000";',  # 1 a quoted hex, for the judge
        'ctx.fillStyle = "rgb(1,2,3)";',  # 2
        'document.querySelector("#main");',  # 3 not a hex
        "class A { #abc = 1; }",  # 4 a private field
        "el.style.color = 'var(--fg)';",  # 5
        'document.querySelector("#feed");',  # 6 an id selector made of hex letters
    ]) + "\n", encoding="utf-8")
    (site / "view.tsx").write_text("const c = `#abc`;\nconst d = hsl(1, 2, 3);\n", encoding="utf-8")
    code = block_rg("look-grammar", site)[2]
    hits = sorted(line.split(":", 2)[0].rsplit("/", 1)[1] + ":" + line.split(":", 2)[1] for line in code.stdout.splitlines())
    assert hits == ["main.js:2", "view.tsx:2"], (hits, code.stdout)
    (site / "main.js").write_text("el.style.color = 'var(--fg)';\nclass A { #abc = 1; }\n", encoding="utf-8")
    (site / "view.tsx").unlink()
    code = block_rg("look-grammar", site)[2]
    assert code.returncode == 1 and not code.stdout, code


def test_look_grammar_dashes(tmp):
    """The look-grammar dash check finds an em or en dash as a character, a named entity, or a numeric
    entity, and passes a page with only hyphens and other entities."""
    site = Path(tmp, "dash-site")
    site.mkdir()
    dashes = ("—", "–", "&mdash;", "&ndash;", "&#8212;", "&#8211;", "&#x2014;", "&#X2013;",
              "&#08212;", "&#x002014;", "&#0008211;", "&#X02013;")
    for text, found in [(d, True) for d in dashes] + [("a - b &amp; &#8210; &#821; &#08210; &#x02012;", False), ("", False)]:
        (site / "index.html").write_text(f"<p>tool {text} text</p>\n", encoding="utf-8")
        out = block_rg("look-grammar", site)[1]
        assert (out.stdout.strip() != "") == found, (text, out)


def test_web_ui_basics_checks(tmp):
    """The web-ui-basics rg checks read the page files, not DESIGN.md prose beside them."""
    site = Path(tmp, "basics-site")
    site.mkdir()
    (site / "DESIGN.md").write_text("Never use outline: none. prefers-reduced-motion is nice.\n", encoding="utf-8")
    (site / "index.html").write_text("<h1>A</h1>\n", encoding="utf-8")
    (site / "style.css").write_text("h1 { color: red; }\n", encoding="utf-8")
    banned, motion = block_rg("web-ui-basics", site)[:2]
    assert banned.returncode == 1 and not banned.stdout, banned
    assert motion.returncode == 1 and not motion.stdout, "DESIGN.md prose must not count as handling reduced motion"
    (site / "style.css").write_text("@media (prefers-reduced-motion: reduce) { * { animation: none; } }\n", encoding="utf-8")
    banned, motion = block_rg("web-ui-basics", site)[:2]
    assert motion.returncode == 0 and "style.css" in motion.stdout, motion
    # The banned-pattern check hits ease-in at a line end and maximum-scale=1 (or 1.0), not 1.5 or ease-in-out.
    (site / "style.css").write_text("a { transition: opacity 200ms ease-in\n}\nb { transition: color 200ms ease-in-out; }\n",
                                    encoding="utf-8")
    (site / "index.html").write_text('<meta content="maximum-scale=1.5">\n<meta content="maximum-scale=1">\n'
                                     '<meta content="maximum-scale=1.0, width=device-width">\n', encoding="utf-8")
    hits = [line.split(":", 1)[1] for line in block_rg("web-ui-basics", site)[0].stdout.splitlines()]
    assert sorted(hits) == ["1:a { transition: opacity 200ms ease-in", '2:<meta content="maximum-scale=1">',
                            '3:<meta content="maximum-scale=1.0, width=device-width">'], hits
    # An image tag whose attributes sit on later lines shows all of them.
    (site / "index.html").write_text('<h1>A</h1>\n<img\n  src="a.png"\n  alt="A"\n  width="10"\n  height="10">\n'
                                     '<img src="b.png">\n', encoding="utf-8")
    (site / "DESIGN.md").write_text("Use <img with alt.\n", encoding="utf-8")
    images = block_rg("web-ui-basics", site)[3]
    assert all(attr in images.stdout for attr in ('alt="A"', 'width="10"', 'height="10"', '<img src="b.png">')), images
    assert "DESIGN.md" not in images.stdout, images


with tempfile.TemporaryDirectory() as tmp:
    home, repo = Path(tmp, "home"), Path(tmp, "app")
    (home / ".flow" / "app").mkdir(parents=True)
    rule = Path(tmp, "rule.md")
    rule.write_text("RULE TEXT 42\n")
    (home / ".flow" / "domains.md").write_text(f"## any\n- build: {rule}\n## web\n- build: {rule}, /nope.md\n")
    (home / ".flow" / "app" / "profile.md").write_text("Domains: any\nRepo rules: none\n")
    repo.mkdir()
    env = {**git_env(), "HOME": str(home)}
    run = lambda *a: subprocess.run([sys.executable, SCRIPT, *map(str, a)], cwd=repo, env=env, capture_output=True, text=True)
    git = lambda *a: subprocess.run(["git", *a], cwd=repo, env=env, capture_output=True, text=True, check=True).stdout.strip()
    git("init", "-q", "-b", "main")
    (repo / "app.py").write_text("def parse_phone(s):\n    return s\n\nprint(parse_phone('1'))\n")
    git("add", "app.py")
    git("commit", "-q", "-m", "one")

    folder = Path(run("start").stdout.splitlines()[0])
    assert "RULE TEXT 42" in (folder / "05-build.md").read_text()
    assert "RULE TEXT 42" not in (folder / "02-design.md").read_text()
    assert sorted(p.name for p in folder.glob("0*.md"))[0] == "01-frame.md"
    assert "Branch: main" in (folder / "01-frame.md").read_text()
    assert "Take with:" in (folder / "05-build.md").read_text()
    assert "/tmp/" in (repo / ".git" / "info" / "exclude").read_text()
    # The folder carries its step graph on each Uses line, and 05-build names its revision rule,
    # so a reader without this script (the Swarm app) can draw the graph and judge stale.
    uses = {p.stem: p.read_text().splitlines()[1] for p in folder.glob("0*.md")}
    assert uses["01-frame"] == "Uses:" and uses["04-impact"] == "Uses: 03-contracts", uses
    assert uses["05-build"] == "Uses: 04-impact, 02-design" and uses["07-close"] == "Uses: 06-review", uses
    assert "\nRevision: HEAD\n" in (folder / "05-build.md").read_text()
    assert "Revision:" not in (folder / "06-review.md").read_text()
    # A catalog with no block item writes no block, no profile, and no judge section (old behaviour).
    assert all("block:" not in text and "Judge checks" not in text
               and "profile " not in next(l for l in text.splitlines() if l.startswith("Skills read:"))
               for text in (p.read_text(encoding="utf-8") for p in folder.glob("0*.md")))

    test_skip_text_encoding()
    test_command_wrong_count(run, folder)
    test_done_wrong_count(run, folder)
    test_take_force(run, folder)
    test_take_requires_who(run, folder)
    test_done_rejects_force(run, folder)
    test_closed_stdout(repo, env, folder)

    blocked = run("take", folder, "05-build", "a")
    assert blocked.returncode != 0 and "needs 04-impact, 02-design" in blocked.stderr

    # Only design, contracts, and impact can be skipped, and only with a reason.
    assert "cannot be skipped" in run("skip", folder, "05-build", "small").stderr
    assert "skip <folder> <step> <why>" in run("skip", folder, "02-design").stderr
    test_skip_plain_reason(run, folder)
    test_skip_lock(run, folder)
    test_skip_rejects_force(run, folder)
    test_atomic_skip(folder)
    assert run("skip", folder, "02-design", "no", "UI").returncode == 0
    assert (folder / "02-design.md").read_text().startswith("Status: skipped no UI\n")

    for step in ("01-frame", "03-contracts"):
        assert run("take", folder, step, "a").returncode == 0
        fill(folder / f"{step}.md")
        assert run("done", folder, step).returncode == 0, step
    c = folder / "03-contracts.md"
    c.write_text(c.read_text() + "`parse_phone()` rejects an empty string. `app.py` and `init` stay.\n")

    # take 04-impact writes the places that name each code name of the contracts.
    test_callers_after_claim(run, folder)
    test_impact_closed_stdout(repo, env, folder)
    test_impact_utf8(repo, env, folder)
    test_callers_timeout(folder)
    test_callers_long_stderr(folder)
    test_callers_stdout_utf8_cut(folder)
    test_callers_stderr_utf8_cut(folder)
    test_callers_large_output(folder)
    test_callers_long_line(folder)
    test_callers_failure(folder)
    test_callers_deadline(folder)
    test_callers_name_cap(folder)
    test_callers_below_rules(folder)
    test_callers_cut_list(folder)
    test_callers_end_only_in_impact(tmp)
    test_callers_controls(repo, run, folder)
    out = run("take", folder, "04-impact", "a")
    assert out.returncode == 0, out.stderr
    assert flow_run.CALLERS_HEAD in out.stdout, "take must print the complete impact file"
    assert out.stdout == (folder / "04-impact.md").read_text() + "\n"
    impact = (folder / "04-impact.md").read_text()
    assert "### parse_phone (2 places)" in impact and "app.py:1:" in impact and "### init" not in impact
    assert impact.index("## Callers") < impact.index("## Result")
    fill(folder / "04-impact.md")
    assert run("done", folder, "04-impact").returncode == 0

    out = run("take", folder, "05-build", "agent-b")
    assert out.returncode == 0 and "RULE TEXT 42" in out.stdout
    text = (folder / "05-build.md").read_text()
    assert text.startswith("Status: active agent-b") and f"Skills read: {rule}" in text and text.rstrip().endswith("## Result")
    assert "05-build: active agent-b" in run("status", folder).stdout
    refused = run("done", folder, "05-build")
    assert refused.returncode != 0 and "Failing check first" in refused.stderr
    f = folder / "05-build.md"
    f.write_text(f.read_text().replace("- [ ] Task file path and Base commit (pair or deliver): ",
                                       "- [x] Task file path and Base commit (pair or deliver): "))
    assert "no evidence" in run("done", folder, "05-build").stderr
    fill(f)
    head = git("rev-parse", "HEAD")
    assert run("done", folder, "05-build").returncode == 0 and f.read_text().startswith(f"Status: done {head[:12]}\n")

    # Close needs the review-check run's own APPROVE verdict on the current head.
    out = run("take", folder, "06-review", "a")
    assert out.returncode == 0 and "bring the open rows" not in out.stdout
    r = folder / "06-review.md"
    review_text = r.read_text()
    for count in (1, 2, 3):
        r.write_text(review_text + "".join(f"Run: tmp/review-check/range-{i}\n" for i in range(count)))
        out = run("take", folder, "06-review", "a")
        assert out.returncode == 0, out.stderr
        warning = f"06-review already lists {count} runs; bring the open rows to the user before a new range"
        assert out.stdout.splitlines().count(warning) == (1 if count >= 2 else 0)
        if count < 2:
            assert "bring the open rows" not in out.stdout
        assert r.read_text().startswith("Status: active a\n")
        assert [line for line in r.read_text().splitlines() if line.startswith("Run:")] == [
            f"Run: tmp/review-check/range-{i}" for i in range(count)], "take must keep one Run: line per round, newest last"
    r.write_text(review_text)
    review = Path(repo, "tmp", "review-check", "range-1")
    review.mkdir(parents=True)
    (review / "target.json").write_text(json.dumps({"head": head}))
    (review / "03-verdict.md").write_text("Status: done x\nUses:\nVerdict: REQUEST CHANGES (1 of 1 units; 1 fix)\n")
    fill(r)
    assert run("done", folder, "06-review").returncode == 0
    assert "needs a `Run:" in run("take", folder, "07-close", "a").stderr
    r.write_text(r.read_text() + "Run: tmp/review-check/range-1\n")
    assert "REQUEST CHANGES" in run("take", folder, "07-close", "a").stderr
    (review / "03-verdict.md").write_text("Status: done x\nUses:\nVerdict: NEEDS DISCUSSION (1 of 1 units; 1 ask)\n")
    assert "NEEDS DISCUSSION" in run("take", folder, "07-close", "a").stderr
    r.write_text(r.read_text() + "User accepted: ship it, the ask is a follow-up\n")
    refused = run("take", folder, "07-close", "a")
    assert refused.returncode != 0 and "NEEDS DISCUSSION" in refused.stderr, "User accepted: must not bypass the run's verdict"
    r.write_text(r.read_text().replace("User accepted: ship it, the ask is a follow-up\n", ""))
    (review / "03-verdict.md").write_text("Status: done x\nUses:\nVerdict: APPROVE (1 of 1 units; 0 fix)\n")

    # A new commit moves the build revision, so the review is stale and close refuses.
    (repo / "app.py").write_text((repo / "app.py").read_text() + "# fix\n")
    git("commit", "-q", "-am", "two")
    assert "06-review: done" in run("status", folder).stdout and "(stale: 05-build)" in run("status", folder).stdout
    assert "covers" in run("take", folder, "07-close", "a").stderr
    (review / "target.json").write_text(json.dumps({"head": git("rev-parse", "HEAD")}))
    assert run("take", folder, "07-close", "a").returncode == 0

    events = (folder / "events.log").read_text()
    assert "02-design\tskip no UI" in events and "07-close\ttake a" in events
    test_git_env_ignores_caller_git_dir(tmp)
    test_block_in_step(tmp)
    test_block_hash_bytes(tmp)
    test_block_alias_and_base(tmp)
    test_block_profile(tmp)
    test_profile_on_second_start(tmp)
    test_block_example_params(tmp)
    test_block_params(tmp)
    test_block_controls(tmp)
    test_block_headings(tmp)
    test_skill_fenced_headings(tmp)
    test_start_without_tomllib(tmp)
    test_start_no_block_reads_no_toml(tmp)
    test_toml_no_block_adds_no_base(tmp)
    test_param_quoted_in_script_checks(tmp)
    test_param_outside_span_in_script_check(tmp)
    test_add_path_errors(tmp)
    test_block_fail_closed(tmp)
    test_kit_blocks_load(tmp)
    test_block_repo_file(tmp)
    test_block_cap(tmp)
    test_block_checks(tmp)
    test_html_diagram_check(tmp)
    test_look_grammar_colors(tmp)
    test_look_grammar_page_code_colors(tmp)
    test_diagram_shot_check(tmp)
    test_look_grammar_dashes(tmp)
    test_web_ui_basics_checks(tmp)
    test_block_ascii_locale(tmp)
    test_review_read_encoding(tmp)
    print("ok")
