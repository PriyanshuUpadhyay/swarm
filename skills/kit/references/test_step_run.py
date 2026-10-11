"""Run: python3 test_step_run.py"""
import fcntl, os, subprocess, sys, tempfile, time
from contextlib import redirect_stdout
from io import StringIO
from pathlib import Path
from unittest.mock import patch

import step_run

SCRIPT = Path(__file__).with_name("step_run.py")
run = lambda *a: subprocess.run([sys.executable, SCRIPT, *map(str, a)], capture_output=True, text=True)


def test_command_wrong_count():
    for command, count in (("start", 2), ("status", 1), ("take", 3), ("done", 2)):
        usage = next(line.strip() for line in step_run.__doc__.splitlines()
                     if line.strip().split()[:1] == [command])
        for args in ((), ("unused",) * (count - 1), ("unused",) * (count + 1)):
            out = run(command, *args)
            assert out.returncode == 1 and usage in out.stderr, out.stderr
            assert "Traceback" not in out.stderr, out.stderr


def lock_holder(folder, step):
    code = """import sys
sys.path.insert(0, sys.argv[1])
import step_run
with step_run.step_lock(sys.argv[2], sys.argv[3]):
    print("held", flush=True)
    sys.stdin.read()
"""
    child = subprocess.Popen([sys.executable, "-c", code, str(SCRIPT.parent), str(folder), step],
                             stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    held = False
    try:
        held = child.stdout.readline() == "held\n"
    finally:
        if not held:
            child.kill()
            _, stderr = child.communicate(timeout=5)
    if not held:
        raise AssertionError(stderr)
    return child


def test_done_wrong_count(folder):
    for args in ((folder,), (folder, "01-ask", "extra")):
        out = run("done", *args)
        assert out.returncode == 1 and "done <folder> <step>" in out.stderr, out.stderr
        assert "--force" not in out.stderr and "Traceback" not in out.stderr, out.stderr


def test_done_rejects_force(folder):
    file = folder / "01-ask.md"
    original = file.read_text(encoding="utf-8")
    events = (folder / "events.log").read_text(encoding="utf-8")
    try:
        for args in ((folder, "01-ask", "--force"), ("--force", folder, "01-ask")):
            out = run("done", *args)
            assert out.returncode == 1, "done must refuse --force"
            assert "done <folder> <step>" in out.stderr and "Traceback" not in out.stderr, out.stderr
            assert file.read_text(encoding="utf-8") == original
            assert (folder / "events.log").read_text(encoding="utf-8") == events
    finally:
        file.write_text(original, encoding="utf-8")
        (folder / "events.log").write_text(events, encoding="utf-8")


def test_lock_holder_failed_handshake(folder):
    code = 'import sys; print("lock failed", file=sys.stderr, flush=True); print("not held", flush=True); sys.stdin.read()'
    child = subprocess.Popen([sys.executable, "-c", code], stdin=subprocess.PIPE,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    try:
        with patch.object(subprocess, "Popen", return_value=child):
            try:
                lock_holder(folder, "01-ask")
                raise AssertionError("lock_holder must refuse a failed handshake")
            except AssertionError as error:
                message = str(error)
        assert child.poll() is not None, "a failed handshake must kill and wait for the child"
        assert all(pipe.closed for pipe in (child.stdin, child.stdout, child.stderr)), "child pipes must close"
        assert "lock failed" in message, "the assertion must include the child stderr"
    finally:
        if child.poll() is None:
            child.kill()
        if not child.stdin.closed:
            child.communicate(timeout=5)


def test_process_lock(folder):
    file = folder / "01-ask.md"
    original = file.read_text(encoding="utf-8")
    events = (folder / "events.log").read_text(encoding="utf-8")
    child = lock_holder(folder, "01-ask")
    lock = folder / "01-ask.lock"
    try:
        # Neither an old mtime nor --force may let another process enter.
        old = time.time() - 7200
        os.utime(lock, (old, old))
        for command, args in (("take", ("b",)), ("done", ())):
            flags = (("--force",), ()) if command == "take" else ((),)
            for force in flags:
                out = run(command, folder, "01-ask", *args, *force)
                assert out.returncode == 1, out
                assert f"01-ask is locked by a running {command}; wait for it" in out.stderr, out.stderr
                assert "Traceback" not in out.stderr
        contenders = [subprocess.Popen([sys.executable, SCRIPT, "take", str(folder), "01-ask", "b", *flags],
                                       stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
                      for flags in (("--force",), ("--force",), ())]
        for contender in contenders:
            _, error = contender.communicate(timeout=5)
            assert contender.returncode == 1 and "is locked by a running take" in error, error
        assert file.read_text(encoding="utf-8") == original
        assert (folder / "events.log").read_text(encoding="utf-8") == events
    finally:
        child.communicate(timeout=5)
    assert lock.exists(), "the lock file must stay after release"


def test_dead_run_unlock(folder):
    for killed in (True, False):
        child = lock_holder(folder, "01-ask")
        if killed:
            child.kill()
        child.communicate(timeout=5)
        successor = lock_holder(folder, "01-ask")
        successor.communicate(timeout=5)
        assert successor.returncode == 0
        assert (folder / "01-ask.lock").exists()


def test_no_lock_path_mutation(folder):
    # A filesystem without hard links must not change the acquisition path.
    with patch.object(step_run.os, "link", side_effect=OSError("no hard links")), \
            patch.object(step_run.os, "rename", side_effect=AssertionError("lock rename")):
        with step_run.step_lock(folder, "01-ask"):
            pass
    assert (folder / "01-ask.lock").exists()


def test_claim_failure_log(folder):
    file = folder / "01-ask.md"
    original = file.read_text(encoding="utf-8")
    events = (folder / "events.log").read_text(encoding="utf-8")

    def failed(path):
        raise RuntimeError("callers failed")

    try:
        try:
            step_run.take(folder, "01-ask", "hook", on_claim=failed)
            raise AssertionError("claim hook must raise")
        except RuntimeError as error:
            assert str(error) == "callers failed"
        assert file.read_text(encoding="utf-8").startswith("Status: active hook\n")
        assert (folder / "events.log").read_text(encoding="utf-8").splitlines()[-1].endswith(
            "\t01-ask\ttake hook failed: callers failed")
        with step_run.step_lock(folder, "01-ask"):
            pass
    finally:
        file.write_text(original, encoding="utf-8")
        (folder / "events.log").write_text(events, encoding="utf-8")


def test_explicit_text_encoding():
    import ast
    tree = ast.parse(Path(__file__).read_text(encoding="utf-8"))
    missing = [node.lineno for node in ast.walk(tree) if isinstance(node, ast.Call)
               and isinstance(node.func, ast.Attribute) and node.func.attr in ("read_text", "write_text", "open")
               and not any(kw.arg == "encoding" for kw in node.keywords)]
    assert not missing, f"text operations without UTF-8 encoding at {missing}"


def test_plain_controls():
    assert hasattr(step_run, "plain"), "step_run must own the shared plain helper"
    text = "\tindented\nnext\r\x1b\x01\x7f\x85"
    assert step_run.plain(text) == "\tindentednext"
    assert step_run.plain(text, keep_newlines=True) == "\tindented\nnext"
    assert step_run.plain("a\u2028b\u2029c") == "abc"
    assert step_run.plain("a\u2028b\u2029c", keep_newlines=True) == "a\nb\nc"


def test_claim_failure_log_one_line(folder):
    file = folder / "01-ask.md"
    original = file.read_text(encoding="utf-8")
    events = (folder / "events.log").read_text(encoding="utf-8")

    def failed(path):
        raise RuntimeError("\x1ba\u2028b\u2029c\x7f")

    try:
        with redirect_stdout(StringIO()):
            try:
                step_run.take(folder, "01-ask", "hook", on_claim=failed)
                raise AssertionError("claim hook must raise")
            except RuntimeError:
                pass
        lines = (folder / "events.log").read_text(encoding="utf-8").splitlines()
        assert len(lines) == len(events.splitlines()) + 1, "each event must occupy one line"
        assert lines[-1].endswith("\t01-ask\ttake hook failed: a b c"), lines[-1]
    finally:
        file.write_text(original, encoding="utf-8")
        (folder / "events.log").write_text(events, encoding="utf-8")


def test_utf8_and_lock_context(folder):
    file = folder / "02-look.md"
    original = file.read_text(encoding="utf-8")
    dependency = folder / "01-ask.md"
    original_dependency = dependency.read_text(encoding="utf-8")
    original_events = (folder / "events.log").read_text(encoding="utf-8")
    dependency.write_text(original_dependency + "合意\n", encoding="utf-8")
    file.write_text(original + "合意\n", encoding="utf-8")
    # Disable C-locale coercion, UTF-8 mode, and UTF-8 stream output independently.
    env = dict(os.environ, LC_ALL="C", PYTHONUTF8="0", PYTHONCOERCECLOCALE="0", PYTHONIOENCODING="ascii")
    try:
        out = subprocess.run([sys.executable, SCRIPT, "take", str(folder), "02-look", "ascii-owner"],
                             env=env, capture_output=True, text=True)
        assert out.returncode == 0, out.stderr
        assert "\\u5408\\u610f" in out.stdout
        assert "Uses: 01-ask@" in file.read_text(encoding="utf-8")
        out = subprocess.run([sys.executable, SCRIPT, "status", str(folder)],
                             env=env, capture_output=True, text=True)
        assert out.returncode == 0, out.stderr
        file.write_text(file.read_text(encoding="utf-8").replace("- [ ] what the sources say: ",
                                                             "- [x] what the sources say: 合意"), encoding="utf-8")
        out = subprocess.run([sys.executable, SCRIPT, "done", str(folder), "02-look"],
                             env=env, capture_output=True, text=True)
        assert out.returncode == 0, out.stderr
        assert "合意" in file.read_text(encoding="utf-8")
        with step_run.step_lock(folder, "01-ask"):
            try:
                step_run.take(folder, "01-ask", "a")
                raise AssertionError("take must refuse a held lock")
            except SystemExit as error:
                assert error.__suppress_context__, "lock refusal must hide the lock error"
    finally:
        file.write_text(original, encoding="utf-8")
        dependency.write_text(original_dependency, encoding="utf-8")
        (folder / "events.log").write_text(original_events, encoding="utf-8")


def test_cli_output(folder):
    file = folder / "01-ask.md"
    original = file.read_text(encoding="utf-8")
    original_events = (folder / "events.log").read_text(encoding="utf-8")
    env = {**os.environ, "PYTHONIOENCODING": "ascii"}
    out = subprocess.run([sys.executable, SCRIPT, "take", str(folder), "01-ask", "合意"],
                         env=env, capture_output=True, text=True)
    assert out.returncode == 0, out.stderr
    assert "active \\u5408\\u610f" in out.stdout
    assert file.read_text(encoding="utf-8").startswith("Status: active 合意\n")
    file.write_text(original, encoding="utf-8")
    with subprocess.Popen([sys.executable, SCRIPT, "take", str(folder), "01-ask", "a"],
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE) as child:
        child.stdout.close()
        errors = child.stderr.read()
        assert child.wait() == 0 and not errors, errors
    file.write_text(original, encoding="utf-8")
    (folder / "events.log").write_text(original_events, encoding="utf-8")


def test_atomic_step_writes(folder):
    file = folder / "01-ask.md"
    original = file.read_text(encoding="utf-8")
    original_events = (folder / "events.log").read_text(encoding="utf-8")
    real_replace = os.replace
    previous = original
    replacements = []
    sources = []

    def check_replace(source, destination):
        nonlocal previous
        assert source.parent == file.parent and source.suffix == ".tmp" and destination == file
        with (folder / "01-ask.lock").open("a", encoding="utf-8") as contender:
            try:
                fcntl.flock(contender.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                pass
            else:
                raise AssertionError("take and done must hold the step lock during replacement")
        assert file.read_text(encoding="utf-8") == previous, "readers must keep seeing the complete old file"
        next_text = source.read_text(encoding="utf-8")
        assert next_text and next_text.startswith("Status: ")
        real_replace(source, destination)
        previous = next_text
        replacements.append(destination)
        sources.append(source)

    with redirect_stdout(StringIO()), patch.object(step_run.os, "replace", check_replace):
        step_run.take(folder, "01-ask", "atomic", need=[])
        assert replacements == [file], "take must replace the file atomically"
        step_run.done(folder, "01-ask", rev="atomic")
        assert replacements == [file, file], "done must replace the file atomically"
    assert file.read_text(encoding="utf-8") == previous and not file.with_suffix(".md.tmp").exists()
    assert len(set(sources)) == 2 and not any(source.exists() for source in sources), "each write needs its own temporary file"
    file.write_text(original, encoding="utf-8")
    (folder / "events.log").write_text(original_events, encoding="utf-8")


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


with tempfile.TemporaryDirectory() as tmp:
    skill = Path(tmp, "SKILL.md")
    skill.write_text("| File | Needs | Holds |\n|---|---|---|\n| `01-ask.md` | none | the question |\n"
                     "| `02-look.md` | ask | what the sources say |\n\nafter\n", encoding="utf-8")
    folder = Path(tmp, "run")
    run("start", folder, skill)
    assert (folder / "02-look.md").read_text(encoding="utf-8").splitlines()[1] == "Uses: 01-ask"
    assert run("take", folder, "02-look", "a").returncode != 0
    assert (folder / "02-look.lock").exists()
    assert "the question" in run("done", folder, "01-ask").stderr
    f = folder / "01-ask.md"
    f.write_text(f.read_text(encoding="utf-8").replace("- [ ] the question: ", "- [x] the question: why is the sky blue"), encoding="utf-8")
    assert run("done", folder, "01-ask").returncode == 0 and f.read_text(encoding="utf-8").startswith("Status: done ")
    test_explicit_text_encoding()
    test_command_wrong_count()
    test_plain_controls()
    test_done_wrong_count(folder)
    test_done_rejects_force(folder)
    test_lock_holder_failed_handshake(folder)
    test_process_lock(folder)
    test_dead_run_unlock(folder)
    test_no_lock_path_mutation(folder)
    test_claim_failure_log(folder)
    test_claim_failure_log_one_line(folder)
    test_step_mode(folder)
    test_utf8_and_lock_context(folder)
    test_cli_output(folder)
    test_atomic_step_writes(folder)
    out = run("take", folder, "02-look", "agent-b")
    assert out.returncode == 0 and "Uses: 01-ask@" in (folder / "02-look.md").read_text(encoding="utf-8")
    assert "(ready)" not in run("status", folder).stdout.split("02-look")[1]
    assert "stale" not in run("status", folder).stdout
    f.write_text(f.read_text(encoding="utf-8") + "a later edit\n", encoding="utf-8")
    assert "02-look: active agent-b  (stale: 01-ask)" in run("status", folder).stdout
    events = [l.split("\t")[1:] for l in (folder / "events.log").read_text(encoding="utf-8").splitlines()]
    assert events[0][0] == "01-ask" and events[0][1].startswith("done ") and events[1] == ["02-look", "take agent-b"]
    claimed = folder / "02-look.md"
    original = claimed.read_text(encoding="utf-8")
    original_events = (folder / "events.log").read_text(encoding="utf-8")
    out = run("take", folder, "02-look", "agent-c")
    assert out.returncode != 0
    assert out.stderr.strip() == "02-look is active for agent-b; use --force if that run is gone"
    assert claimed.read_text(encoding="utf-8") == original
    assert (folder / "events.log").read_text(encoding="utf-8") == original_events
    assert (folder / "02-look.lock").exists()
    out = run("take", folder, "02-look", "agent-c", "--force")
    assert out.returncode == 0 and claimed.read_text(encoding="utf-8").startswith("Status: active agent-c\n")
    assert (folder / "02-look.lock").exists()
    assert run("take", folder, "02-look", "agent-c").returncode == 0
    lock = folder / "02-look.lock"
    claimed.write_text(original.replace("Status: active agent-b", "Status: open"), encoding="utf-8")
    children = [subprocess.Popen([sys.executable, SCRIPT, "take", str(folder), "02-look", who],
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
                for who in ("agent-d", "agent-e")]
    outputs = [child.communicate() for child in children]
    assert sorted(child.returncode for child in children) == [0, 1], outputs
    winner = ("agent-d", "agent-e")[next(i for i, child in enumerate(children) if child.returncode == 0)]
    assert claimed.read_text(encoding="utf-8").startswith(f"Status: active {winner}\n")
    assert lock.exists()
    print("ok")
