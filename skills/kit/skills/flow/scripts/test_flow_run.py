"""Run: python3 test_flow_run.py  (uses a temporary repo and a temporary HOME)."""
import fcntl
import json
import os
import subprocess
import sys
import tempfile
from contextlib import redirect_stdout
from io import StringIO
from pathlib import Path
from unittest.mock import patch

import flow_run

SCRIPT = Path(__file__).with_name("flow_run.py")
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
    for command, args in (("start", ("extra",)), ("status", (folder, "extra")),
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
                         for l in f.read_text().splitlines()))


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
        assert not any(ord(c) < 32 and c not in "\n\t" or 127 <= ord(c) <= 159 for c in out.stdout), repr(out.stdout)
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


with tempfile.TemporaryDirectory() as tmp:
    home, repo = Path(tmp, "home"), Path(tmp, "app")
    (home / ".flow" / "app").mkdir(parents=True)
    rule = Path(tmp, "rule.md")
    rule.write_text("RULE TEXT 42\n")
    (home / ".flow" / "domains.md").write_text(f"## any\n- build: {rule}\n## web\n- build: {rule}, /nope.md\n")
    (home / ".flow" / "app" / "profile.md").write_text("Domains: any\nRepo rules: none\n")
    repo.mkdir()
    env = {**os.environ, "HOME": str(home), "GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@t",
           "GIT_COMMITTER_NAME": "t", "GIT_COMMITTER_EMAIL": "t@t"}
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
    print("ok")
