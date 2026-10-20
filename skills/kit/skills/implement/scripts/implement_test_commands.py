# Implement test commands cases and helpers.
from implement_test_support import *


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
        usage = next(line.strip() for line in implement_run.__doc__.splitlines()
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

    with redirect_stdout(StringIO()), patch.object(implement_run.step_run.os, "replace", check_replace):
        implement_run.skip(folder, "02-design", "no", "UI")
    assert replaced == [file], "skip must replace the file atomically"
    assert file.read_text().startswith("Status: skipped no UI\n")
    file.write_text(original)


def test_skip_lock(run, folder):
    file = folder / "02-design.md"
    original = file.read_text(encoding="utf-8")
    events = (folder / "events.log").read_text(encoding="utf-8")
    with implement_run.step_run.step_lock(folder, "02-design"):
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
