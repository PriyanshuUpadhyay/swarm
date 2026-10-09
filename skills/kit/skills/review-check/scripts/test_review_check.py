"""Run: python3 test_review_check.py. Builds a throwaway repo and checks that the verdict is earned:
a unit with no row, a rule with no result, a made-up quote, a quote at the wrong line, or a bare "ok"
proof blocks it; a language with no ctags support still gets units; a changed function that others
call gets REF; the build gate owns the IDs the repo's lint config proves, and only for this head."""
import json, os, runpy, shutil, signal, subprocess, sys, tempfile, time
from contextlib import redirect_stdout
from io import BytesIO, StringIO, TextIOWrapper
from pathlib import Path
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import review_check as rc


def sh(*cmd):
    return subprocess.run(cmd, check=True, capture_output=True, text=True).stdout.strip()


def commit(files, msg):
    for name, text in files.items():
        if text is None:
            Path(name).unlink()
        else:
            Path(name).parent.mkdir(parents=True, exist_ok=True)
            Path(name).write_text(text)
    sh("git", "add", "-A")
    sh("git", "commit", "-qm", msg)
    return sh("git", "rev-parse", "HEAD")


def rows(run, *lines):
    checklist = load_checklist(run)
    files = {"02-review.md": []}
    for line in lines:
        cells = [cell.strip() for cell in line.split("|")[1:-1]]
        uid, rules = cells[:2]
        owned = checklist[uid].get("owners", {})
        groups = {}
        for rule in rules.split(","):
            owner = owned.get(rule.strip().strip("`"))
            name = f"02-review-{owner}.md" if owner else "02-review.md"
            groups.setdefault(name, []).append(rule.strip())
        for name, ids in groups.items():
            files.setdefault(name, []).append("| " + " | ".join([uid, ", ".join(ids), *cells[2:]]) + " |")
    for name, values in files.items():
        (run / name).write_text("Status: done x\nUses: 01-units@x\n\n" + "".join(l + "\n" for l in values))


def load_checklist(run):
    value = json.loads((run / "checklist.json").read_text(encoding="utf-8"))
    value.pop("seats", None)
    return value


def main():
    base = commit({"pane.swift": "func resize() {\n  a()\n}\n", "gone.py": "def guard():\n    return 1\n",
                   "util.py": "def helper():\n    return 1\n", "app.py": "from util import helper\nhelper()\n"}, "base")
    head = commit({"pane.swift": "func resize() {\n  a()\n  b()\n}\n", "gone.py": None,
                   "util.py": "def helper():\n    return 2\n"}, "change")
    rc.start(f"{base}..{head}")
    run = next(Path("tmp/review-check").iterdir())
    units = {u["file"]: u for u in json.loads((run / "units.json").read_text())}
    checklist = load_checklist(run)
    assert set(units) == {"pane.swift", "gone.py", "util.py"}, units  # swift has no ctags parser here
    assert units["gone.py"]["symbol"] == "deleted file"
    s, g, h = units["pane.swift"]["id"], units["gone.py"]["id"], units["util.py"]["id"]
    assert "REF" in checklist[h]["rules"] and "app.py:1" in checklist[h]["refs"]["helper"], checklist[h]

    def all_rules(uid, quote):  # one n/a row may cover many rules with one reason
        ids = ", ".join(checklist[uid]["rules"]) or "-"
        return f"| {uid} | {ids} | x | `{quote}` | n/a | | the test code has no case these rules name |"

    rows(run, f"| {s} | - | pane.swift:3 | `b()` | pass | | b() runs after a(), no shared state |",
         f"| {g} | - | gone.py:1 | `def guard` | pass | | no caller imports guard |",
         f"| {h} | - | util.py:2 | `return 2` | pass | | app.py ignores the return value |")
    assert rc.verdict(run.name) == (1 if any(c["rules"] for c in checklist.values()) else 0), "rules without a result"

    rows(run, all_rules(s, "b()"), all_rules(g, "def guard"), all_rules(h, "return 2"),
         f"| {s} | - | pane.swift:3 | `b()` | pass | | ok |")
    assert rc.verdict(run.name) == 1, "a bare ok is not a proof"

    rows(run, all_rules(s, "b()"), all_rules(g, "def guard"), all_rules(h, "return 2"),
         f"| {s} | - | pane.swift:3 | `c()` | fix | made up | x |")
    assert rc.verdict(run.name) == 1, "a quote that is not in the diff"

    rows(run, all_rules(s, "b()"), all_rules(g, "def guard"), all_rules(h, "return 2"),
         f"| {h} | REF | util.py:1 | `return 2` | fix | helper now returns 2 | app.py:2 expects 1 |")
    assert rc.verdict(run.name) == 1, "the quote is in the diff, but not at the line the row names"

    rows(run, all_rules(s, "b()"), all_rules(g, "def guard"), all_rules(h, "return 2"),
         f"| {h} | REF | util.py:2 | `return 2` | ask | helper now returns 2 | ok |")
    assert rc.verdict(run.name) == 1, "a bare ok is not a proof on an ask row either"

    rows(run, all_rules(s, "b()"), all_rules(g, "def guard"), all_rules(h, "return 2"),
         f"| {h} | REF | util.py:2 | `return 2` | fix | helper now returns 2 | app.py:2 expects 1 |")
    assert rc.verdict(run.name) == 0
    lines = (run / "03-verdict.md").read_text().splitlines()
    assert lines[0].startswith("Status: done ") and lines[2].startswith("Verdict: REQUEST CHANGES (3 of 3 units"), lines
    assert lines[2].endswith("build: none)"), lines[2]


def test_build_gate():
    """The repo's lint config decides which rule IDs the build owns, and the verdict counts them only from
    build.json for this head."""
    rules = Path.home() / ".review-check" / "rules" / f"{Path.cwd().name}.md"
    rules.parent.mkdir(parents=True, exist_ok=True)
    rules.write_text("Files: `**/*.ts`\n\nCI: `Lint` runs `true`; config `.oxlintrc.json`\n")
    config = {"ignorePatterns": ["vendor"], "rules": {
        "@typescript-eslint/no-floating-promises": "error", "@typescript-eslint/await-thenable": "error",
        "no-fallthrough": ["error", {"commentPattern": "skip"}]}}  # options: the fixtures do not speak for it
    base = commit({".oxlintrc.json": json.dumps(config), "a.ts": "const m = new Map();\nexport const v = m.get(1)!;\n",
                   "vendor/b.ts": "export {};\n"}, "lint base")
    head = commit({"a.ts": "const m = new Map();\nexport const v = m.get(1)!;\nexport async function f() { await g(); }\n",
                   "vendor/b.ts": "export async function f() { await g(); }\n"}, "lint change")
    rc.start(f"{base}..{head}")
    run = Path("tmp/review-check") / f"range-{base[:7]}-{head[:7]}-01"
    units = {u["file"]: u["id"] for u in json.loads((run / "units.json").read_text())}
    checklist = load_checklist(run)
    a, v = checklist[units["a.ts"]], checklist[units["vendor/b.ts"]]
    assert set(a["tool"]) == {"TS-1", "TS-24"} and not {"TS-1", "TS-24"} & set(a["rules"]), a
    assert "TS-6" not in a["rules"], "Scope: added, and the `!` is on a line the change did not add"
    assert not v["tool"] and "TS-24" in v["rules"], "an ignored path keeps its IDs with the seats"

    def answers(*extra):
        rows(run, *(f"| {uid} | {', '.join(c['rules']) or '-'} | x | `f()` | n/a | | the test code has no such case |"
                    for uid, c in checklist.items()), *extra)

    answers()
    assert rc.verdict(run.name) == 1, "no build.json"
    (run / "build.json").write_text(json.dumps({"head": base, "digest": None, "source": "check-run", "name": "Lint",
                                                "conclusion": "success", "tail": []}))
    assert rc.verdict(run.name) == 1, "a build result for another head"
    (run / "build.json").write_text(json.dumps({"head": head, "digest": None, "source": "check-run", "name": "Lint",
                                                "conclusion": "success", "tail": []}))
    answers(f"| {units['a.ts']} | TS-24 | a.ts:3 | `await g()` | pass | | g returns a promise |")
    assert rc.verdict(run.name) == 1, "a seat row for an ID the build owns"
    answers()
    assert rc.verdict(run.name) == 0
    assert (run / "03-verdict.md").read_text().splitlines()[2].startswith("Verdict: APPROVE"), "build pass counts tool IDs"
    Path("a.ts").write_text("dirty\n")
    rc.build(run.name, "--run")  # the live tree can move while the frozen head is checked
    verify = Path("tmp/review-check/verify").resolve()
    assert sh("git", "-C", str(verify), "rev-parse", "HEAD") == head
    assert (verify / "a.ts").read_text() != "dirty\n"
    assert Path("a.ts").read_text() == "dirty\n", "build changed the live tree"
    sh("git", "checkout", "--", "a.ts")
    rc.build(run.name, "--run")  # the persistent worktree is reused
    assert json.loads((run / "build.json").read_text())["conclusion"] == "success"
    (run / "build.json").write_text(json.dumps({"head": head, "digest": None, "source": "command", "name": "true",
                                                "conclusion": "failure", "tail": ["a.ts(3,30): error TS2304"]}))
    assert rc.verdict(run.name) == 0
    line3 = (run / "03-verdict.md").read_text().splitlines()[2]
    assert line3.startswith("Verdict: REQUEST CHANGES") and line3.endswith("build: fail)"), line3


def test_shards():
    catalog = [{"id": f"C-{n}", "source": "lens.md", "aspect": "lens"} for n in range(1, 501)]
    catalog += [{"id": "PY-1", "source": "lens-python.md", "aspect": "lang"},
                {"id": "T-1", "source": "project.md", "aspect": "repo"}]
    checklist = {f"u{i}": {"rules": [f"C-{n}" for n in range(1, size + 1)], "tool": [], "refs": {}}
                 for i, size in enumerate([80, 70, 70, 70, 70, 70, 70], 1)}
    checklist["u1"]["rules"] += ["PY-1", "T-1", "REF"]
    seats = rc.seat_map(checklist, catalog)
    shards = [p for seat, p in seats.items() if seat.startswith("lens-")]
    assert len(shards) == 3 and sum(p["checks"] for p in shards) == 500, seats
    assert [uid for p in shards for uid in p["units"]] == list(checklist), "units split or out of order"
    assert max(p["checks"] for p in shards) == 210, "whole-unit ranges are not balanced"
    for uid, entry in checklist.items():
        assert set(entry["owners"]) == set(entry["rules"]), "a pair has no owner"
        for rule, owner in entry["owners"].items():
            assert uid in seats[owner]["units"]
            aspect = "lens" if rule.startswith("C-") else {"PY-1": "lang", "T-1": "repo", "REF": "refs"}[rule]
            assert sum(uid in p["units"] for seat, p in seats.items() if seat.split("-")[0] == aspect) == 1
    assert seats["refs"]["route"] == "review.deep", seats
    small = {"u1": {"rules": [f"C-{n}" for n in range(1, 101)]}}
    assert list(rc.seat_map(small, catalog)) == ["lens"], "a single shard must keep its old name"
    large_unit = {"u1": {"rules": [f"C-{n}" for n in range(1, 501)]}}
    assert list(rc.seat_map(large_unit, catalog)) == ["lens"], "a whole unit cannot be split"


def test_repo_rules_origin():
    previous = Path.cwd()
    with tempfile.TemporaryDirectory() as folder:
        try:
            repo = Path(folder) / "lens-api"
            repo.mkdir()
            os.chdir(repo)
            sh("git", "init", "-qb", "main")
            sh("git", "config", "user.email", "t@t")
            sh("git", "config", "user.name", "t")
            base = commit({"value.py": "value = 1\n"}, "base")
            head = commit({"value.py": "value = 2\n"}, "head")
            rules = Path.home() / ".review-check" / "rules" / "lens-api.md"
            rules.parent.mkdir(parents=True, exist_ok=True)
            rules.write_text("Files: `**/*.py`\n- `REPO-901` a project rule\n")
            rc.start(f"{base}..{head}")
            run = Path("tmp/review-check") / f"range-{base[:7]}-{head[:7]}-01"
            checklist = load_checklist(run)
            unit = next(iter(checklist.values()))
            assert unit["owners"]["REPO-901"] == "repo", unit
            seats = json.loads((run / "checklist.json").read_text())["seats"]
            assert seats["repo"]["checks"] == 1 and seats["repo"]["route"] == "review.check"
        finally:
            os.chdir(previous)


def test_seat_gaps():
    previous = Path.cwd()
    with tempfile.TemporaryDirectory() as folder:
        try:
            os.chdir(folder)
            sh("git", "init", "-qb", "main")
            sh("git", "config", "user.email", "t@t")
            sh("git", "config", "user.name", "t")
            base = commit({"util.py": "value = 1\n", "README.md": "old docs\n"}, "base")
            head = commit({"util.py": "value = 2\n", "README.md": "new docs\n"}, "head")
            catalog = [{"id": f"C-{n}", "source": "lens.md", "aspect": "lens", "files": ["**/*.py"], "applies": None,
                        "check": [], "scope": None} for n in range(1, 6)]
            with patch.object(rc, "load_rules", return_value=catalog):
                rc.start(f"{base}..{head}")
            run = Path("tmp/review-check") / f"range-{base[:7]}-{head[:7]}-01"
            checklist = load_checklist(run)
            units = {u["file"]: u["id"] for u in json.loads((run / "units.json").read_text())}
            uid, docs = units["util.py"], units["README.md"]
            # Keep this test small, with two named owners and one foreign answer.
            checklist[uid]["rules"] = ["C-1", "PY-3"]
            checklist[uid]["owners"] = {"C-1": "lens-1", "PY-3": "lang"}
            seats = {"lens-1": {"units": [uid], "checks": 1}, "lang": {"units": [uid], "checks": 1}}
            (run / "checklist.json").write_text(json.dumps(checklist | {"seats": seats}))
            row = f"| {uid} | PY-3 | util.py:1 | `value = 2` | pass | | the value has no caller |\n"
            (run / "02-review-lens-1.md").write_text(row)
            # A stray seat cannot fill a named owner's gap. The chair covers a unit with no rules.
            (run / "02-review-stray.md").write_text(row)
            rows(run, f"| {docs} | - | README.md:1 | `new docs` | pass | | this unit only changes prose |")
            with redirect_stdout(StringIO()) as out:
                assert rc.verdict(run.name) == 1
            text = out.getvalue()
            assert "seat lens-1:" in text and "checks owned by another seat" in text, text
            assert f"seat lens-1: {uid}: no result for C-1" in text, text
            assert f"seat lang: {uid}: no result for PY-3" in text, text
            assert text.index("no result for C-1") < text.index("no result for PY-3"), text
            (run / "02-review-lens-1.md").write_text(row.replace("PY-3", "C-1"))
            chair = run / "02-review.md"
            docs_row = chair.read_text()
            chair.write_text(docs_row + row)
            with redirect_stdout(StringIO()) as out:
                assert rc.verdict(run.name) == 1, "a chair row cannot replace the lang seat's answer"
            text = out.getvalue()
            assert "chair row names PY-3 owned by seat lang" in text, text
            assert f"seat lang: {uid}: no result for PY-3" in text, text
            assert "Verdict: APPROVE" not in (run / "03-verdict.md").read_text()
            chair.write_text(docs_row)
            (run / "02-review-lang.md").write_text(row)
            with redirect_stdout(StringIO()):
                assert rc.verdict(run.name) == 0
            assert "Verdict: APPROVE" in (run / "03-verdict.md").read_text()
        finally:
            os.chdir(previous)


def test_missing_verify_directory(run, verify, head):
    other = verify.parent / "other"
    sh("git", "worktree", "add", "--detach", str(other), head)
    shutil.rmtree(other)
    shutil.rmtree(verify)
    rc.build(run.name, "--run")
    assert f"worktree {other}\n" in sh("git", "worktree", "list", "--porcelain"), "another missing worktree was pruned"
    assert sh("git", "-C", str(verify), "rev-parse", "HEAD") == head
    assert json.loads((run / "build.json").read_text())["conclusion"] == "success"


def test_verify_tree_cleanup(run, verify, head):
    (verify / "value.txt").write_text("dirty verify tree\n")
    (verify / "staged.txt").write_text("staged output\n")
    sh("git", "-C", str(verify), "add", "staged.txt")
    (verify / "output.txt").write_text("untracked output\n")
    (verify / "ignored.txt").write_text("cached input\n")
    exclude = Path(sh("git", "rev-parse", "--git-common-dir")) / "info" / "exclude"
    with exclude.open("a") as file:
        file.write("ignored.txt\n")
    rc.build(run.name, "--run")
    assert sh("git", "-C", str(verify), "rev-parse", "HEAD") == head
    assert (verify / "value.txt").read_text() == "later\n"
    assert not (verify / "staged.txt").exists() and not (verify / "output.txt").exists()
    assert (verify / "ignored.txt").read_text() == "cached input\n"
    assert json.loads((run / "build.json").read_text())["conclusion"] == "success"


def test_verify_build_lock(run, verify):
    lock = verify.with_suffix(".lock")
    original = (run / "build.json").read_text()
    head = sh("git", "-C", str(verify), "rev-parse", "HEAD")
    with rc.step_run.step_lock(lock.parent, "verify", command="build --run"):
        old = time.time() - 7200
        os.utime(lock, (old, old))
        try:
            rc.build(run.name, "--run")
            raise AssertionError("build must refuse a live verify lock regardless of age")
        except SystemExit as error:
            assert str(error) == "verify is locked by a running build --run; wait for it", error
        assert (run / "build.json").read_text() == original
        assert sh("git", "-C", str(verify), "rev-parse", "HEAD") == head
    try:
        rc.build(run.name, "--run", "--force")
        raise AssertionError("build --run --force must be rejected")
    except SystemExit as error:
        assert "--force" in str(error), error
    target = json.loads((run / "target.json").read_text())
    checking_lock = {**target, "ci": {**target["ci"], "cmd": "test -f ../verify.lock"}}
    (run / "target.json").write_text(json.dumps(checking_lock))
    try:
        for command, conclusion in (("test -f ../verify.lock", "success"), ("false", "failure")):
            checking_lock["ci"]["cmd"] = command
            (run / "target.json").write_text(json.dumps(checking_lock))
            rc.build(run.name, "--run")
            assert json.loads((run / "build.json").read_text())["conclusion"] == conclusion
            with rc.step_run.step_lock(lock.parent, "verify"):
                pass
            assert lock.exists(), "build must leave the harmless lock file"
    finally:
        (run / "target.json").write_text(json.dumps(target))


def wait_pid(pidfile, process, message):
    deadline = time.monotonic() + 5
    while True:
        text = pidfile.read_text(encoding="utf-8").strip() if pidfile.exists() else ""
        if text:
            return int(text)
        assert process.poll() is None and time.monotonic() < deadline, message
        time.sleep(0.01)


def kill_pid(pidfile, pid=None, *, group=False):
    if pid is None:
        text = pidfile.read_text(encoding="utf-8").strip() if pidfile.exists() else ""
        pid = int(text) if text else None
    if pid is not None:
        try:
            (os.killpg if group else os.kill)(pid, signal.SIGKILL)
        except ProcessLookupError:
            pass


def test_pid_helpers():
    from unittest.mock import Mock
    with tempfile.TemporaryDirectory() as folder:
        pidfile = Path(folder) / "child.pid"
        process = Mock()
        process.poll.return_value = None

        def ready(_):
            pidfile.write_text("12345\n", encoding="utf-8")

        pidfile.write_text("", encoding="utf-8")
        with patch.object(time, "sleep", side_effect=ready):
            assert wait_pid(pidfile, process, "child did not start") == 12345
        pidfile.write_text("", encoding="utf-8")
        process.poll.return_value = 1
        try:
            wait_pid(pidfile, process, "child did not start")
            raise AssertionError("a dead child must stop the wait")
        except AssertionError as error:
            assert str(error) == "child did not start", error
        process.poll.return_value = None
        with patch.object(time, "monotonic", side_effect=[0, 5]):
            try:
                wait_pid(pidfile, process, "child timed out")
                raise AssertionError("the deadline must stop the wait")
            except AssertionError as error:
                assert str(error) == "child timed out", error
        with patch.object(os, "kill") as kill:
            kill_pid(pidfile)
            kill.assert_not_called()
            pidfile.write_text("12345\n", encoding="utf-8")
            kill_pid(pidfile)
            kill.assert_called_once_with(12345, signal.SIGKILL)
        with patch.object(os, "killpg") as killpg:
            kill_pid(pidfile, group=True)
            killpg.assert_called_once_with(12345, signal.SIGKILL)
        with patch.object(os, "kill", side_effect=ProcessLookupError):
            kill_pid(pidfile)
        pidfile.unlink()
        kill_pid(pidfile)


def test_build_child_signal_mask(run, verify):
    import shlex
    target = json.loads((run / "target.json").read_text(encoding="utf-8"))
    code = "import json, signal; print(json.dumps(sorted(signal.pthread_sigmask(signal.SIG_BLOCK, ()))))"
    command = f"{shlex.quote(sys.executable)} -I -c {shlex.quote(code)}"
    (run / "target.json").write_text(json.dumps({**target, "ci": {**target["ci"], "cmd": command}}), encoding="utf-8")
    try:
        rc.build(run.name, "--run")
        result = json.loads((run / "build.json").read_text(encoding="utf-8"))
        assert result["conclusion"] == "success", result
        blocked = set(json.loads(result["tail"][-1]))
        assert not blocked.intersection((signal.SIGTERM, signal.SIGHUP, signal.SIGINT)), blocked
    finally:
        (run / "target.json").write_text(json.dumps(target), encoding="utf-8")


def test_build_interrupt(run, verify, signum=signal.SIGINT):
    target = json.loads((run / "target.json").read_text(encoding="utf-8"))
    pidfile = verify / "interrupt-group.pid"
    pidfile.unlink(missing_ok=True)
    command = ": > interrupt-group.pid; sleep 0.1; echo $$ > interrupt-group.pid; exec sleep 30"
    (run / "target.json").write_text(json.dumps({**target, "ci": {**target["ci"], "cmd": command}}), encoding="utf-8")
    code = """import signal, sys
signal.signal(signal.SIGINT, signal.default_int_handler)
signal.signal(signal.SIGTERM, signal.SIG_DFL)
signal.signal(signal.SIGHUP, signal.SIG_DFL)
print('CI 合意', file=sys.stderr, flush=True)
sys.path.insert(0, sys.argv[1])
import review_check as rc
rc.build(sys.argv[2], '--run')
"""
    # The child must own Ctrl-C handling even when the test runner ignores SIGINT.
    previous = signal.getsignal(signal.SIGINT)
    signal.signal(signal.SIGINT, signal.SIG_IGN)
    try:
        child = subprocess.Popen([sys.executable, "-c", code, str(HERE), run.name],
                                 stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, encoding="utf-8",
                                 errors="replace", start_new_session=True)
    finally:
        signal.signal(signal.SIGINT, previous)
    group = None
    try:
        group = wait_pid(pidfile, child, "CI did not start")
        child.send_signal(signum)
        try:
            _, errors = child.communicate(timeout=2)
        except subprocess.TimeoutExpired:
            raise AssertionError(f"signal {signum} did not stop the CI group promptly") from None
        assert child.returncode != 0 and "合意" in errors, errors
        if signum == signal.SIGINT:
            assert "KeyboardInterrupt" in errors, errors
        try:
            os.killpg(group, 0)
        except ProcessLookupError:
            pass
        else:
            raise AssertionError(f"signal {signum} left the CI process group alive")
        with rc.step_run.step_lock(verify.parent, "verify"):
            pass
    finally:
        kill_pid(pidfile, group, group=True)
        if child.poll() is None:
            child.kill()
        child.communicate(timeout=5)
        (run / "target.json").write_text(json.dumps(target), encoding="utf-8")


def test_build_ignored_hangup():
    previous = signal.getsignal(signal.SIGHUP)
    signal.signal(signal.SIGHUP, signal.SIG_IGN)
    try:
        with rc.build_signals():
            assert signal.getsignal(signal.SIGHUP) == signal.SIG_IGN, "build must keep ignored SIGHUP"
        assert signal.getsignal(signal.SIGHUP) == signal.SIG_IGN
    finally:
        signal.signal(signal.SIGHUP, previous)


def test_build_interrupt_ignored_hangup(run, verify):
    code = """import os, signal, sys
signal.signal(signal.SIGHUP, signal.SIG_IGN)
sys.path.insert(0, sys.argv[1])
import review_check as rc
communicate = rc.subprocess.Popen.communicate
hangups = []
def hangup(process, *args, **kwargs):
    if process.args[:2] == ['bash', '-c']:
        hangups.append(True)
        os.kill(os.getpid(), signal.SIGHUP)
        assert signal.getsignal(signal.SIGHUP) == signal.SIG_IGN
    return communicate(process, *args, **kwargs)
rc.subprocess.Popen.communicate = hangup
rc.build(sys.argv[2], '--run')
assert hangups == [True]
assert signal.getsignal(signal.SIGHUP) == signal.SIG_IGN
print('ignored hangup survived')
"""
    child = subprocess.run([sys.executable, "-c", code, str(HERE), run.name],
                           capture_output=True, text=True, timeout=5)
    assert child.returncode == 0, child.stderr
    assert "ignored hangup survived" in child.stdout, child.stdout


def test_build_launch_deferred_signal(run, verify):
    original = (run / "build.json").read_text(encoding="utf-8")
    real_popen, real_killpg = subprocess.Popen, os.killpg
    for signum in (signal.SIGTERM, signal.SIGHUP, signal.SIGINT):
        running = None
        previous_mask = signal.pthread_sigmask(signal.SIG_BLOCK, ())
        previous_handler = signal.getsignal(signum)
        signal.signal(signum, signal.default_int_handler if signum == signal.SIGINT else signal.SIG_DFL)

        def launch(args, **kwargs):
            nonlocal running
            running = real_popen(["bash", "-c", "exec sleep 30"], **kwargs)
            os.kill(os.getpid(), signum)
            return running

        try:
            with patch.object(rc, "home", return_value=verify.parent), \
                    patch.object(rc, "verify_tree", return_value=verify), \
                    patch.object(rc.subprocess, "Popen", side_effect=launch), \
                    patch.object(rc.os, "killpg", wraps=real_killpg) as killpg:
                try:
                    rc.build(run.name, "--run")
                    raise AssertionError("the deferred launch signal must escape")
                except (SystemExit, KeyboardInterrupt) as error:
                    if signum == signal.SIGINT:
                        assert isinstance(error, KeyboardInterrupt), error
                    else:
                        assert isinstance(error, SystemExit) and error.code == 128 + signum, error
            killpg.assert_called_once_with(running.pid, signal.SIGKILL)
            assert running.poll() is not None, "the launched shell must be reaped"
            assert running.stdout.closed and running.stderr.closed
            assert signal.pthread_sigmask(signal.SIG_BLOCK, ()) == previous_mask, "the launch must restore its signal mask"
            assert (run / "build.json").read_text(encoding="utf-8") == original
            with rc.step_run.step_lock(verify.parent, "verify", command="build --run"):
                pass
        finally:
            signal.pthread_sigmask(signal.SIG_SETMASK, previous_mask)
            signal.signal(signum, previous_handler)
            if running is not None:
                if running.poll() is None:
                    real_killpg(running.pid, signal.SIGKILL)
                running.communicate(timeout=5)


def test_build_launch_interrupt(run, verify):
    original = (run / "build.json").read_text(encoding="utf-8")
    previous = signal.getsignal(signal.SIGTERM)
    with patch.object(rc, "home", return_value=verify.parent), \
            patch.object(rc, "verify_tree", return_value=verify), \
            patch.object(rc.subprocess, "Popen", side_effect=SystemExit(128 + signal.SIGTERM)), \
            patch.object(rc.os, "killpg") as killpg:
        try:
            rc.build(run.name, "--run")
            raise AssertionError("launch interruption must escape")
        except SystemExit as error:
            assert error.code == 128 + signal.SIGTERM, error
    killpg.assert_not_called()
    assert signal.getsignal(signal.SIGTERM) == previous, "launch interruption must restore signals"
    assert (run / "build.json").read_text(encoding="utf-8") == original
    with rc.step_run.step_lock(verify.parent, "verify", command="build --run"):
        pass


def test_build_after_launch_interrupt(run, verify):
    from unittest.mock import MagicMock
    process = MagicMock()
    process.pid = 12345
    process.stdout.closed = process.stderr.closed = False
    original = (run / "build.json").read_text(encoding="utf-8")

    def interrupt_after_launch(frame, event, arg):
        if (frame.f_code is rc.build.__code__ and event == "line" and frame.f_locals.get("process") is process
                and signal.SIGTERM not in signal.pthread_sigmask(signal.SIG_BLOCK, ())):
            # Deliver the interruption only after the parent restores its signal mask.
            sys.settrace(None)
            os.kill(os.getpid(), signal.SIGTERM)
        return interrupt_after_launch

    previous_trace = sys.gettrace()
    with patch.object(rc, "home", return_value=verify.parent), \
            patch.object(rc, "verify_tree", return_value=verify), \
            patch.object(rc.subprocess, "Popen", return_value=process), \
            patch.object(rc.os, "killpg") as killpg:
        try:
            sys.settrace(interrupt_after_launch)
            rc.build(run.name, "--run")
            raise AssertionError("post-launch interruption must escape")
        except SystemExit as error:
            assert error.code == 128 + signal.SIGTERM, error
        finally:
            sys.settrace(previous_trace)
    killpg.assert_called_once_with(12345, signal.SIGKILL)
    process.communicate.assert_called_once_with(timeout=rc.CLEANUP_SECONDS)
    process.stdout.close.assert_called_once()
    process.stderr.close.assert_called_once()
    assert (run / "build.json").read_text(encoding="utf-8") == original


def test_build_timeout_bounded_cleanup(run, verify):
    from unittest.mock import MagicMock
    process = MagicMock()
    process.pid = 12345
    process.communicate.side_effect = subprocess.TimeoutExpired("ci", 1)
    with patch.object(rc, "home", return_value=verify.parent), \
            patch.object(rc, "CLEANUP_SECONDS", 0.05), \
            patch.object(rc, "verify_tree", return_value=verify), \
            patch.object(rc.subprocess, "Popen", return_value=process), \
            patch.object(rc.os, "killpg") as killpg:
        rc.build(run.name, "--run")
    assert [call.kwargs.get("timeout") for call in process.communicate.call_args_list] == [rc.BUILD_TIMEOUT, 0.05]
    process.kill.assert_called_once()
    process.wait.assert_called_once_with(timeout=0.05)
    killpg.assert_called_once_with(12345, signal.SIGKILL)
    process.stdout.close.assert_called_once()
    process.stderr.close.assert_called_once()
    result = json.loads((run / "build.json").read_text(encoding="utf-8"))
    assert result["conclusion"] == "failure" and result["tail"][-1] == f"timed out after {rc.BUILD_TIMEOUT} s"


def test_build_timeout_escaped_child(run, verify):
    target = json.loads((run / "target.json").read_text(encoding="utf-8"))
    # This child keeps the pipes open in a separate session, beyond the group kill.
    code = "import subprocess; from pathlib import Path; p=subprocess.Popen(['sleep','15'], start_new_session=True); Path('escaped.pid').write_text(str(p.pid))"
    import shlex
    command = f"{shlex.quote(sys.executable)} -c {shlex.quote(code)}; sleep 30"
    (run / "target.json").write_text(json.dumps({**target, "ci": {**target["ci"], "cmd": command}}), encoding="utf-8")
    escaped = None
    pidfile = verify / "escaped.pid"
    real_popen = subprocess.Popen
    running = None

    def ready_process(args, **kwargs):
        nonlocal running, escaped
        process = real_popen(args, **kwargs)
        if args[:2] == ["bash", "-c"]:
            running = process
            escaped = wait_pid(pidfile, process, "escaped child did not start")
        return process

    try:
        started = time.monotonic()
        with patch.object(rc, "BUILD_TIMEOUT", 0.1), patch.object(rc, "CLEANUP_SECONDS", 0.05), \
                patch.object(rc.subprocess, "Popen", side_effect=ready_process):
            rc.build(run.name, "--run")
        elapsed = time.monotonic() - started
        assert elapsed < 8, f"escaped pipe holder delayed cleanup for {elapsed} s"
        result = json.loads((run / "build.json").read_text(encoding="utf-8"))
        assert result["tail"][-1] == "timed out after 0.1 s"
        with rc.step_run.step_lock(verify.parent, "verify"):
            pass
    finally:
        kill_pid(pidfile, escaped)
        if running is not None and running.poll() is None:
            os.killpg(running.pid, signal.SIGKILL)
            running.communicate(timeout=5)
        (run / "target.json").write_text(json.dumps(target), encoding="utf-8")


def test_build_timeout(run, verify):
    target = json.loads((run / "target.json").read_text())
    slow = {**target, "ci": {**target["ci"], "cmd": "sleep 5"}}
    (run / "target.json").write_text(json.dumps(slow))
    try:
        with patch.object(rc, "BUILD_TIMEOUT", 0.05, create=True):
            rc.build(run.name, "--run")
        result = json.loads((run / "build.json").read_text())
        assert result["conclusion"] == "failure", result
        assert result["tail"][-1] == "timed out after 0.05 s", result
        with rc.step_run.step_lock(verify.parent, "verify"):
            pass
    finally:
        (run / "target.json").write_text(json.dumps(target))


def test_build_timeout_children(run, verify):
    target = json.loads((run / "target.json").read_text())
    command = "sleep 30 >/dev/null 2>&1 & echo $! > timeout-child.pid; wait"
    (run / "target.json").write_text(json.dumps({**target, "ci": {**target["ci"], "cmd": command}}))
    child = None
    try:
        with patch.object(rc, "BUILD_TIMEOUT", 0.2):
            rc.build(run.name, "--run")
        child = int((verify / "timeout-child.pid").read_text())
        deadline = time.monotonic() + 2
        while True:
            try:
                os.kill(child, 0)
            except ProcessLookupError:
                break
            assert time.monotonic() < deadline, f"timed out build left sleep {child} alive"
            time.sleep(0.01)
        result = json.loads((run / "build.json").read_text())
        assert result["conclusion"] == "failure" and result["tail"][-1] == "timed out after 0.2 s"
        with rc.step_run.step_lock(verify.parent, "verify"):
            pass
    finally:
        if child is not None:
            try:
                os.kill(child, signal.SIGKILL)
            except ProcessLookupError:
                pass
        (run / "target.json").write_text(json.dumps(target))


def test_pushed_build_without_verify(run, verify, head):
    verify.mkdir()
    gh_result = {"check_runs": [{"id": 1, "status": "completed", "conclusion": "success",
                                 "html_url": "https://example.invalid/check/1"}]}
    real_run = subprocess.run
    calls = []

    def stub_gh(args, **kwargs):
        if args[0] == "gh":
            calls.append((args, kwargs))
            return subprocess.CompletedProcess(args, 0, json.dumps(gh_result), "")
        return real_run(args, **kwargs)

    with patch.object(rc.subprocess, "run", stub_gh):
        rc.build(run.name)
    result = json.loads((run / "build.json").read_text())
    assert result["source"] == "check-run" and result["head"] == head
    assert result["conclusion"] == "success" and result["tail"] == ["https://example.invalid/check/1"]
    assert len(calls) == 1 and Path(calls[0][1]["cwd"]).resolve() == Path.cwd().resolve()
    assert calls[0][1].get("timeout") == rc.BUILD_TIMEOUT
    assert verify.is_dir() and not (verify.parent / "verify.lock").exists()


def test_pushed_build_timeout(run):
    original = (run / "build.json").read_text(encoding="utf-8")
    real_run = subprocess.run
    calls = []

    def timeout_gh(args, **kwargs):
        if args[0] == "gh":
            calls.append((args, kwargs))
            raise subprocess.TimeoutExpired(args, rc.BUILD_TIMEOUT)
        return real_run(args, **kwargs)

    with patch.object(rc.subprocess, "run", timeout_gh):
        try:
            rc.build(run.name)
            raise AssertionError("a timed out gh call must refuse build proof")
        except SystemExit as error:
            assert str(error) == f"gh: timed out after {rc.BUILD_TIMEOUT} s", error
            assert error.__cause__ is None and error.__suppress_context__, "gh timeout must suppress exception context"
    assert len(calls) == 1 and calls[0][1].get("timeout") == rc.BUILD_TIMEOUT
    assert (run / "build.json").read_text(encoding="utf-8") == original


def test_pushed_build_missing_gh(run):
    original = (run / "build.json").read_text(encoding="utf-8")
    real_run = subprocess.run
    calls = []

    def missing_gh(args, **kwargs):
        if args[0] == "gh":
            calls.append(args)
            raise FileNotFoundError("gh")
        return real_run(args, **kwargs)

    with patch.object(rc.subprocess, "run", missing_gh):
        try:
            rc.build(run.name)
            raise AssertionError("a missing gh must refuse build proof")
        except SystemExit as error:
            assert str(error) == "gh: not found", error
            assert error.__cause__ is None and error.__suppress_context__, "missing gh must suppress exception context"
    assert len(calls) == 1
    assert (run / "build.json").read_text(encoding="utf-8") == original


def test_pushed_build_missing_gh_not_a_directory(run):
    original = (run / "build.json").read_text(encoding="utf-8")
    real_run = subprocess.run
    calls = []

    def missing_gh(args, **kwargs):
        if args[0] == "gh":
            calls.append(args)
            raise NotADirectoryError(20, "Not a directory")
        return real_run(args, **kwargs)

    with patch.object(rc.subprocess, "run", missing_gh):
        try:
            rc.build(run.name)
            raise AssertionError("a missing gh must refuse build proof")
        except SystemExit as error:
            assert str(error) == "gh: not found", error
            assert error.__cause__ is None and error.__suppress_context__, "missing gh must suppress exception context"
    assert len(calls) == 1
    assert (run / "build.json").read_text(encoding="utf-8") == original


def test_pushed_build_permission_denied(run):
    original = (run / "build.json").read_text(encoding="utf-8")
    real_run = subprocess.run
    calls = []

    def denied_gh(args, **kwargs):
        if args[0] == "gh":
            calls.append(args)
            raise PermissionError(13, "Permission denied")
        return real_run(args, **kwargs)

    with patch.object(rc.subprocess, "run", denied_gh):
        try:
            rc.build(run.name)
            raise AssertionError("a gh permission error must refuse build proof")
        except SystemExit as error:
            assert str(error) == "gh: Permission denied", error
            assert error.__cause__ is None and error.__suppress_context__, "gh errors must suppress exception context"
    assert len(calls) == 1
    assert (run / "build.json").read_text(encoding="utf-8") == original


def test_frozen_build():
    previous = Path.cwd()
    with tempfile.TemporaryDirectory() as folder:
        try:
            os.chdir(folder)
            sh("git", "init", "-qb", "main")
            sh("git", "config", "user.email", "t@t")
            sh("git", "config", "user.name", "t")
            base = commit({"value.txt": "base\n"}, "base")
            head = commit({"value.txt": "reviewed\n"}, "reviewed")
            command = "test $(cat value.txt) = reviewed && pwd && git rev-parse HEAD"
            ci = {"name": "Check", "cmd": command, "config": None}
            with patch.object(rc, "ci_of", return_value=ci):
                rc.start(f"{base}..{head}")
            run = Path("tmp/review-check") / f"range-{base[:7]}-{head[:7]}-01"
            verify = Path("tmp/review-check/verify").resolve()
            (run / "build.json").write_text("{}\n", encoding="utf-8")
            test_pushed_build_missing_gh(run)
            test_pushed_build_missing_gh_not_a_directory(run)
            test_pushed_build_permission_denied(run)
            test_pushed_build_timeout(run)
            test_pushed_build_without_verify(run, verify, head)
            try:
                rc.build(run.name, "--run")
                raise AssertionError("an ordinary directory was used as a verify worktree")
            except SystemExit as e:
                assert "verify path is not its own git worktree" in str(e), e
            assert sh("git", "symbolic-ref", "--short", "HEAD") == "main", "the live branch was detached"
            verify.rmdir()
            moved = commit({"value.txt": "later\n"}, "later")
            Path("value.txt").write_text("dirty live tree\n")
            rc.build(run.name, "--run")
            result = json.loads((run / "build.json").read_text())
            assert result["conclusion"] == "success" and result["tail"] == [str(verify), head], result
            assert sh("git", "rev-parse", "HEAD") == moved and Path("value.txt").read_text() == "dirty live tree\n"
            assert len(sh("git", "worktree", "list", "--porcelain").split("worktree ")) == 3
            rc.build(run.name, "--run")
            assert len(sh("git", "worktree", "list", "--porcelain").split("worktree ")) == 3, "verify was not reused"
            test_missing_verify_directory(run, verify, head)
            test_verify_build_lock(run, verify)
            test_build_child_signal_mask(run, verify)
            test_build_launch_interrupt(run, verify)
            test_build_launch_deferred_signal(run, verify)
            test_build_after_launch_interrupt(run, verify)
            for signum in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
                test_build_interrupt(run, verify, signum)
            test_build_interrupt_ignored_hangup(run, verify)
            test_build_timeout_bounded_cleanup(run, verify)
            test_build_timeout_escaped_child(run, verify)
            test_build_timeout(run, verify)
            test_build_timeout_children(run, verify)
            target = json.loads((run / "target.json").read_text())
            target["head"] = moved
            target["ci"]["cmd"] = "test $(cat value.txt) = later"
            (run / "target.json").write_text(json.dumps(target))
            rc.build(run.name, "--run")
            assert sh("git", "-C", str(verify), "rev-parse", "HEAD") == moved, "verify did not move to the next head"
            test_verify_tree_cleanup(run, verify, moved)
            assert Path("value.txt").read_text() == "dirty live tree\n", "cleanup changed the live tree"
            target["head"] = None
            target["ci"]["cmd"] = 'test "$(cat value.txt)" = "dirty live tree"'
            (run / "target.json").write_text(json.dumps(target))
            with redirect_stdout(StringIO()) as out:
                rc.build(run.name, "--run")
            assert "proof is from the live tree" in out.getvalue()
            local = json.loads((run / "build.json").read_text())
            assert local["conclusion"] == "success" and local["head"] is None and local["digest"] == rc.tree_digest(), local
        finally:
            os.chdir(previous)


def test_answers():
    """Only a whole new line at the ask's location lets an answer close it."""
    previous = Path.cwd()
    with tempfile.TemporaryDirectory() as d:
        try:
            os.chdir(d)
            sh("git", "init", "-qb", "main")
            sh("git", "config", "user.email", "t@t")
            sh("git", "config", "user.name", "t")
            base = commit({"util.py": "def helper():\n    return 1\n"}, "answer base")
            source_with_separator = 'x = f("`: ", 1)'
            head = commit({"util.py": f"def helper():\n    return 2\n    {source_with_separator}\n"}, "answer change")
            rc.start(f"{base}..{head}")
            run = Path("tmp/review-check") / f"range-{base[:7]}-{head[:7]}-01"
            assert json.loads((run / "target.json").read_text())["patch"] is False

            def ask(run, quote, line=2):
                checklist = load_checklist(run)
                uid, checks = next(iter(checklist.items()))
                rows(run, f"| {uid} | {', '.join(checks['rules']) or '-'} | x | `{quote}` | n/a | | "
                     "the test code has no case these rules name |",
                     f"| {uid} | - | util.py:{line} | `{quote}` | ask | may change a caller | caller expects 1 |")

            ask(run, "return 2")
            answer = "- util.py `return 2`: user approved the return value in the contract\n"
            (run / "answers.md").write_text(answer)
            assert rc.verdict(run.name) == 0
            verdict = (run / "03-verdict.md").read_text()
            assert verdict.splitlines()[2].startswith("Verdict: APPROVE"), verdict
            assert "0 fix, 0 ask, 0 note, 1 answered" in verdict, verdict
            assert "Answered: util.py `return 2` closes 1 asks" in verdict, verdict
            assert "- answered `util.py:2` may change a caller (answer: user approved" in verdict, verdict
            assert f"answers@{rc.revision(run / 'answers.md')}" in verdict.splitlines()[1], verdict
            approved_uses = verdict.splitlines()[1]

            review = run / "02-review.md"
            escaped_problem = "\x1b[31mmay change a caller"
            review.write_text(review.read_text().replace("may change a caller", escaped_problem))
            output = StringIO()
            with redirect_stdout(output):
                assert rc.verdict(run.name) == 0
            assert "\x1b" not in output.getvalue(), "terminal output must remove the ESC byte from a row"
            assert "[31mmay change a caller" in output.getvalue()
            assert escaped_problem in (run / "03-verdict.md").read_text(), "the verdict file must keep the exact row text"
            ask(run, "return 2")
            control_problem = "\x7fmay\x85 change a caller"
            review.write_text(review.read_text().replace("may change a caller", control_problem))
            output = StringIO()
            with redirect_stdout(output):
                assert rc.verdict(run.name) == 0, "C1 controls inside a row must not split the row"
            assert "\x7f" not in output.getvalue() and "\x85" not in output.getvalue(), "terminal output must remove DEL and C1 controls"
            assert "may change a caller" in output.getvalue()
            assert control_problem in (run / "03-verdict.md").read_text(), "the verdict file must keep DEL and C1 controls"
            ask(run, "return 2")

            (run / "answers.md").write_text("- util.py `return 2`: user approved `helper`: returns 2\n")
            assert rc.verdict(run.name) == 0
            verdict = (run / "03-verdict.md").read_text()
            assert verdict.splitlines()[2].startswith("Verdict: APPROVE"), "answer code followed by a colon must close the ask"
            assert "Answered: util.py `return 2` closes 1 asks" in verdict, verdict

            ask(run, source_with_separator, 3)
            source_answer = f"- util.py `{source_with_separator}`: user approved the separator in the contract\n"
            (run / "answers.md").write_text(source_answer)
            assert rc.verdict(run.name) == 0
            verdict = (run / "03-verdict.md").read_text()
            assert "0 fix, 0 ask, 0 note, 1 answered" in verdict, "a source separator must not leave the ask open"
            assert verdict.count("Answered:") == 1, "alternatives belong to one answer line"
            assert f"Answered: util.py `{source_with_separator}` closes 1 asks" in verdict, verdict
            (run / "answers.md").unlink()
            (run / "answers-before.md").write_text(source_answer)
            assert rc.verdict(run.name) == 0
            verdict = (run / "03-verdict.md").read_text()
            assert verdict.count("History:") == 1 and f"History: util.py `{source_with_separator}`: user approved" in verdict, verdict
            (run / "answers-before.md").unlink()
            ask(run, "return 2")
            (run / "answers.md").write_text("- util.py `return 2`: user approved `helper`: ok\n")
            assert rc.verdict(run.name) == 0, "only the matched answer text gets the GENERIC check"
            verdict = (run / "03-verdict.md").read_text()
            assert "answer: user approved `helper`: ok" in verdict, verdict
            (run / "answers.md").write_text("- other.py `return 2`: user approved `helper`: ok\n")
            assert rc.verdict(run.name) == 1, "an unmatched line checks its shortest answer text"
            verdict = (run / "03-verdict.md").read_text()
            assert "Answered: other.py `return 2` closes 0 asks" in verdict, "unmatched lines display the first quote"

            unicode_answer = "- util.py `return 2`: user approved the value — 合意\n"
            for separator in ("\x85", "\u2028", "\u2029", "\x0b", "\x0c", "\x1c", "\x1d", "\x1e"):
                text = f"user approved{separator}the value"
                (run / "answers.md").write_text(f"- util.py `return 2`: {text}\n", encoding="utf-8")
                assert rc.answers(run / "answers.md") == [[("util.py", "return 2", text)]], repr(separator)
            (run / "answers.md").write_text(unicode_answer, encoding="utf-8")
            read_text = Path.read_text

            def ascii_default(path, *args, **kwargs):
                kwargs.setdefault("encoding", "ascii")
                return read_text(path, *args, **kwargs)

            write_text = Path.write_text
            written_encodings = {}

            def ascii_write(path, text, *args, **kwargs):
                written_encodings[path.name] = kwargs.get("encoding")
                kwargs.setdefault("encoding", "ascii")
                return write_text(path, text, *args, **kwargs)

            with patch.object(Path, "read_text", ascii_default):
                assert rc.answers(run / "answers.md") == [[("util.py", "return 2", "user approved the value — 合意")]]
                assert rc.revision(run / "answers.md") == rc.hashlib.sha1(unicode_answer.encode("utf-8")).hexdigest()[:12]
            originals = {}
            for filename in ("01-units.md", "diff.patch", "02-review.md", "units.json", "checklist.json", "target.json"):
                file = run / filename
                originals[filename] = file.read_text(encoding="utf-8")
                text = originals[filename]
                if filename.endswith(".json"):
                    value = json.loads(text)
                    if filename == "units.json":
                        value[0]["symbol"] += " — 合意"
                    elif filename == "checklist.json":
                        next(iter(value.values()))["note"] = "合意"
                    else:
                        value["note"] = "合意"
                    text = json.dumps(value, ensure_ascii=False)
                else:
                    text += "# 合意\n"
                file.write_text(text, encoding="utf-8")
            original_source = Path("util.py").read_text()
            Path("util.py").write_text("def helper():\n    return 3\n")
            with patch.object(Path, "read_text", ascii_default), patch.object(Path, "write_text", ascii_write):
                assert rc.verdict(run.name) == 0
                assert "合意" in (run / "03-verdict.md").read_text(encoding="utf-8")
                rc.start("local")
            assert written_encodings["03-verdict.md"] == written_encodings["target.json"] == "utf-8"
            Path("util.py").write_text(original_source)
            for filename, text in originals.items():
                (run / filename).write_text(text, encoding="utf-8")

            for text in ("ok", "fix: ok", "Fix: ok", "FIX: ok", "fix:ok", "fix:", "FIX:   ", " Fix: ok. "):
                (run / "answers.md").write_text(f"- util.py `return 2`: {text}\n")
                assert rc.verdict(run.name) == 1, "a bare ok is not an answer, even with fix:"
                verdict = (run / "03-verdict.md").read_text()
                assert "an answer needs" in verdict and "INCOMPLETE" in verdict, verdict
                assert approved_uses != verdict.splitlines()[1], "an edit to the first answer line changes Uses"

            for text in ("- util.py `return`: user approved this value\n",
                         "- other.py `return 2`: user approved this value\n"):
                (run / "answers.md").write_text(text)
                assert rc.verdict(run.name) == 0
                verdict = (run / "03-verdict.md").read_text()
                assert "NEEDS DISCUSSION" in verdict and "closes 0 asks" in verdict, verdict
            ask(run, "return 1")
            (run / "answers.md").write_text("- util.py `return 1`: user approved the old value\n")
            assert rc.verdict(run.name) == 0
            verdict = (run / "03-verdict.md").read_text()
            assert "NEEDS DISCUSSION" in verdict and "closes 0 asks" in verdict, "removed lines cannot close asks"
            ask(run, "return 2")
            with (run / "02-review.md").open("a") as f:
                f.write("| u1 | - | util.py:2 | `return 2` | ask | another caller | second caller expects 1 |\n")
            (run / "answers.md").write_text(answer)
            assert rc.verdict(run.name) == 0
            verdict = (run / "03-verdict.md").read_text()
            assert "2 answered" in verdict and "closes 2 asks" in verdict, verdict
            ask(run, "return 2")
            (run / "answers.md").write_text("- util.py `return 2`: fix: test_helper fails at " + head[:7] + "\n")
            assert rc.verdict(run.name) == 0
            verdict = (run / "03-verdict.md").read_text()
            assert verdict.splitlines()[2].startswith("Verdict: REQUEST CHANGES"), verdict
            assert f"- fix `util.py:2` fix: test_helper fails at {head[:7]} (proof: caller expects 1)" in verdict, verdict
            for text in (f"Fix:test_helper fails at {head[:7]}", f"FIX: test_helper fails at {head[:7]}"):
                (run / "answers.md").write_text(answer + f"- util.py `return 2`: {text}\n")
                assert rc.verdict(run.name) == 0
                verdict = (run / "03-verdict.md").read_text()
                assert verdict.splitlines()[2].startswith("Verdict: REQUEST CHANGES"), verdict
                assert f"- fix `util.py:2` {text} (proof: caller expects 1)" in verdict, verdict

            (run / "answers.md").write_text(answer)
            assert rc.verdict(run.name) == 0
            rc.start(f"{base}..{head}")
            rerun = Path("tmp/review-check") / f"range-{base[:7]}-{head[:7]}-02"
            assert (rerun / "answers-before.md").read_text() == answer
            ask(rerun, "return 2")
            assert rc.verdict(rerun.name) == 0
            verdict = (rerun / "03-verdict.md").read_text()
            assert verdict.splitlines()[2].startswith("Verdict: NEEDS DISCUSSION"), verdict
            assert "History: util.py `return 2`: user approved" in verdict, verdict
            changed = commit({"util.py": "def helper():\n    return 3\n"}, "answer line changed")
            target_file = rerun / "target.json"
            metadata = json.loads(target_file.read_text())
            metadata["target"] += " — 合意"
            target_file.write_text(json.dumps(metadata, ensure_ascii=False), encoding="utf-8")
            (rerun / "03-verdict.md").write_text((rerun / "03-verdict.md").read_text() + "— 合意\n", encoding="utf-8")
            with patch.object(Path, "read_text", ascii_default):
                try:
                    rc.start(f"{head}..{changed}")
                    raise AssertionError("UTF-8 predecessor files must still block an open ask")
                except SystemExit as e:
                    assert rerun.name in str(e), e
            Path("util.py").write_text("def helper():\n    return 4\n")
            folders = set(Path("tmp/review-check").iterdir())
            result = rerun / "03-verdict.md"
            open_verdict = result.read_text()
            for prior_verdict in (None, "Status: blocked gate failed\nUses:\nVerdict: INCOMPLETE\n", open_verdict):
                if prior_verdict is None:
                    result.unlink()
                else:
                    result.write_text(prior_verdict)
                for target in (f"{head}..{changed}", "local"):
                    try:
                        rc.start(target)
                        raise AssertionError("a new range above an unfinished review must be refused")
                    except SystemExit as e:
                        assert rerun.name in str(e) and "answers.md" in str(e) and f"verdict {rerun.name}" in str(e), e
                    assert set(Path("tmp/review-check").iterdir()) == folders, "a refused start creates no folder"
            patch_file = Path("tmp/part.patch")
            patch_file.write_text(sh("git", "diff", *rc.PLAIN, head, changed) + "\n")
            rc.start(f"{head}..{changed}", "--patch", str(patch_file))
            partial = Path("tmp/review-check") / f"range-{head[:7]}-{changed[:7]}-01"
            assert json.loads((partial / "target.json").read_text())["patch"] is True
            assert not (partial / "answers-before.md").exists(), "--patch bypasses guard and carry-forward"
            ask(partial, "return 3")
            assert rc.verdict(partial.name) == 0
            newer = partial.stat().st_mtime + 10
            os.utime(partial, (newer, newer))
            latest_answer = "- util.py `return 2`: user approved the value in an earlier answer\n"
            (rerun / "answers.md").write_text(latest_answer)
            assert rc.verdict(rerun.name) == 0
            rc.start(f"{head}..{changed}")
            new_run = Path("tmp/review-check") / f"range-{head[:7]}-{changed[:7]}-02"
            assert (new_run / "answers-before.md").read_text() == latest_answer
            ask(new_run, "return 3")
            (new_run / "answers.md").write_text(answer)
            assert rc.verdict(new_run.name) == 0
            verdict = (new_run / "03-verdict.md").read_text()
            assert verdict.splitlines()[2].startswith("Verdict: NEEDS DISCUSSION"), verdict
            assert "Answered: util.py `return 2` closes 0 asks" in verdict, verdict
            assert "History:" not in verdict, verdict
            (rerun / "answers.md").unlink()
            (rerun / "answers.md").write_text(latest_answer)
            newer = new_run.stat().st_mtime + 10
            os.utime(rerun, (newer, newer))
            assert rerun.stat().st_mtime > new_run.stat().st_mtime
            later = commit({"util.py": "def helper():\n    return 4\n"}, "after open patch ask")
            folders = set(Path("tmp/review-check").iterdir())
            try:
                rc.start(f"{changed}..{later}")
                raise AssertionError("the descendant run with an open ask must block a new range")
            except SystemExit as e:
                assert new_run.name in str(e), "the guard must choose the descendant run despite the older run's mtime"
            assert set(Path("tmp/review-check").iterdir()) == folders
            (new_run / "answers.md").write_text("- util.py `return 3`: user approved the value in the contract\n")
            assert rc.verdict(new_run.name) == 0
            rc.start(f"{changed}..{later}")
            after_patch = Path("tmp/review-check") / f"range-{changed[:7]}-{later[:7]}-01"
            assert (after_patch / "answers-before.md").read_text() == (new_run / "answers.md").read_text()
            ask(after_patch, "return 4")
            assert rc.verdict(after_patch.name) == 0
            rc.start(f"{changed}..{later}")
            same_head = Path("tmp/review-check") / f"range-{changed[:7]}-{later[:7]}-02"
            ask(same_head, "return 4")
            assert rc.verdict(same_head.name) == 0
            assert "0 fix, 1 ask" in (same_head / "03-verdict.md").read_text()
            Path("util.py").write_text("def helper():\n    return 5\n")
            folders = set(Path("tmp/review-check").iterdir())
            try:
                rc.start("local")
                raise AssertionError("local must refuse an open ask at the same head")
            except SystemExit as e:
                assert same_head.name in str(e) and "answers.md" in str(e), e
            assert set(Path("tmp/review-check").iterdir()) == folders
            same_head_answer = "- util.py `return 4`: user approved the value in the contract\n"
            (same_head / "answers.md").write_text(same_head_answer)
            assert rc.verdict(same_head.name) == 0
            assert "Verdict: APPROVE" in (same_head / "03-verdict.md").read_text()
            assert "0 fix, 1 ask" in (after_patch / "03-verdict.md").read_text(), "the older run remains superseded"
            rc.start("local")
            local = Path("tmp/review-check/local-02")
            assert (local / "answers-before.md").read_text() == same_head_answer
        finally:
            os.chdir(previous)


def test_limits():
    """Only limit: answers on the same whole new line close fix rows; asks still block start."""
    previous = Path.cwd()
    with tempfile.TemporaryDirectory() as d:
        try:
            os.chdir(d)
            sh("git", "init", "-qb", "main")
            sh("git", "config", "user.email", "t@t")
            sh("git", "config", "user.name", "t")
            base = commit({"util.py": "def helper():\n    return 1\n"}, "limit base")
            head = commit({"util.py": "def helper():\n    return 2\n"}, "limit change")
            rc.start(f"{base}..{head}")
            run = Path("tmp/review-check") / f"range-{base[:7]}-{head[:7]}-01"

            def review(run, quote="return 2", kind="fix", *extra):
                checklist = load_checklist(run)
                uid, checks = next(iter(checklist.items()))
                rows(run, f"| {uid} | {', '.join(checks['rules']) or '-'} | x | `{quote}` | n/a | | "
                     "the test code has no case these rules name |",
                     f"| {uid} | - | util.py:2 | `{quote}` | {kind} | helper changes a caller | caller expects 1 |", *extra)

            review(run)
            answer = "- util.py `return 2`: limit: user accepts the caller returning 2\n"
            for prefix in ("limit:", "Limit:", "LIMIT:"):
                (run / "answers.md").write_text(answer.replace("limit:", prefix))
                assert rc.verdict(run.name) == 0
                verdict = (run / "03-verdict.md").read_text()
                assert verdict.splitlines()[2].startswith("Verdict: APPROVE"), verdict
                assert "0 fix, 0 ask, 0 note, 0 answered, 1 limit" in verdict, verdict
                counts = rc.re.search(r"; (\d+) fix, (\d+) ask", verdict.splitlines()[2])
                assert counts and counts.groups() == ("0", "0"), "the start guard must still parse line 3"
                assert "- limit `util.py:2` helper changes a caller (limit: user accepts the caller returning 2)" in verdict, verdict
                assert "Answered: util.py `return 2` closes 1 fixes" in verdict, verdict

            for text in ("limit: ok", "Limit:ok", "LIMIT:", " limit: ok. "):
                (run / "answers.md").write_text(f"- util.py `return 2`: {text}\n")
                assert rc.verdict(run.name) == 1, "the text after limit: must pass the answer gate"
                assert "an answer needs" in (run / "03-verdict.md").read_text()

            for text in ("fix: test_helper fails at " + head[:7], "user accepts the caller returning 2"):
                (run / "answers.md").write_text(f"- util.py `return 2`: {text}\n")
                assert rc.verdict(run.name) == 0
                verdict = (run / "03-verdict.md").read_text()
                assert "Verdict: REQUEST CHANGES" in verdict and "1 fix" in verdict, verdict
                assert "closes 0 asks" in verdict, "a fix row needs a limit: answer"
            for file, line in (("other.py", "return 2"), ("util.py", "return"), ("util.py", "return 1")):
                (run / "answers.md").write_text(f"- {file} `{line}`: limit: user accepts the caller returning 2\n")
                assert rc.verdict(run.name) == 0
                assert "Verdict: REQUEST CHANGES" in (run / "03-verdict.md").read_text()

            (run / "answers.md").write_text(answer)
            review(run, "return 2", "ask")
            assert rc.verdict(run.name) == 0
            verdict = (run / "03-verdict.md").read_text()
            assert "Verdict: NEEDS DISCUSSION" in verdict and "1 ask" in verdict, "limit: never closes an ask"

            review(run, "return 2", "fix",
                   "| u1 | - | util.py:2 | `return 2` | ask | may change another caller | second caller expects 1 |")
            assert rc.verdict(run.name) == 0
            verdict = (run / "03-verdict.md").read_text()
            assert "0 fix, 1 ask, 0 note, 0 answered, 1 limit" in verdict, verdict
            changed = commit({"util.py": "def helper():\n    return 3\n"}, "limit line changed")
            folders = set(Path("tmp/review-check").iterdir())
            try:
                rc.start(f"{head}..{changed}")
                raise AssertionError("0 fix and an open ask must block start even with a limit count")
            except SystemExit as e:
                assert run.name in str(e) and "answers.md" in str(e), e
            assert set(Path("tmp/review-check").iterdir()) == folders
            review(run)
            assert rc.verdict(run.name) == 0
            rc.start(f"{head}..{changed}")
            next_run = Path("tmp/review-check") / f"range-{head[:7]}-{changed[:7]}-01"
            assert (next_run / "answers-before.md").read_text() == answer
            review(next_run, "return 3")
            (next_run / "answers.md").write_text(answer)
            assert rc.verdict(next_run.name) == 0
            verdict = (next_run / "03-verdict.md").read_text()
            assert "Verdict: REQUEST CHANGES" in verdict and "1 fix" in verdict, "a changed line must reopen the fix"
        finally:
            os.chdir(previous)


def test_stale_verdict_guard():
    """start refuses changed verdict inputs until verdict runs again, including answers.md."""
    previous = Path.cwd()
    with tempfile.TemporaryDirectory() as d:
        try:
            os.chdir(d)
            sh("git", "init", "-qb", "main")
            sh("git", "config", "user.email", "t@t")
            sh("git", "config", "user.name", "t")
            base = commit({"util.py": "def helper():\n    return 1\n"}, "stale base")
            head = commit({"util.py": "def helper():\n    return 2\n"}, "stale change")
            rc.start(f"{base}..{head}")
            run = Path("tmp/review-check") / f"range-{base[:7]}-{head[:7]}-01"
            checklist = load_checklist(run)
            uid, checks = next(iter(checklist.items()))
            rows(run, f"| {uid} | {', '.join(checks['rules']) or '-'} | x | `return 2` | n/a | | "
                 "the test code has no case these rules name |",
                 f"| {uid} | - | util.py:2 | `return 2` | ask | may change a caller | caller expects 1 |")
            (run / "answers.md").write_text("- util.py `return 2`: user approved the return value in the contract\n")
            (run / "02-review-refs.md").write_text("Status: done x\nUses:\n")
            (run / "build.json").write_text("{}\n")
            assert rc.verdict(run.name) == 0
            assert "Verdict: APPROVE" in (run / "03-verdict.md").read_text()
            changed = commit({"util.py": "def helper():\n    return 3\n"}, "after stale change")
            folders = set(Path("tmp/review-check").iterdir())
            for filename in ("answers.md", "01-units.md", "02-review.md", "02-review-refs.md", "build.json"):
                file = run / filename
                original = file.read_text()
                file.write_text(original.replace("approved", "accepts") if filename == "answers.md" else original + "\n")
                for target in (f"{head}..{changed}", "local"):
                    try:
                        rc.start(target)
                        raise AssertionError(f"start must refuse a stale verdict after {filename} changes")
                    except SystemExit as e:
                        expected = f"start refused by {run.resolve()}: its verdict is stale ({filename} changed); run `verdict {run.name}` again"
                        assert str(e) == expected, e
                    assert set(Path("tmp/review-check").iterdir()) == folders, "a stale verdict creates no folder"
                assert rc.verdict(run.name) == 0
                assert "Verdict: APPROVE" in (run / "03-verdict.md").read_text()
            rc.start(f"{head}..{changed}")
            assert (Path("tmp/review-check") / f"range-{head[:7]}-{changed[:7]}-01").is_dir()
        finally:
            os.chdir(previous)


def test_merge_guard():
    """Every ancestor review must pass the guard, even when a deeper branch is approved."""
    previous = Path.cwd()
    with tempfile.TemporaryDirectory() as d:
        try:
            os.chdir(d)
            sh("git", "init", "-qb", "main")
            sh("git", "config", "user.email", "t@t")
            sh("git", "config", "user.name", "t")
            base = commit({"util.py": "def helper():\n    return 1\n"}, "merge base")
            sh("git", "switch", "-qc", "side")
            side = commit({"util.py": "def helper():\n    return 2\n"}, "side change")
            rc.start(f"{base}..{side}")
            side_run = Path("tmp/review-check") / f"range-{base[:7]}-{side[:7]}-01"

            def review(run, quote, *extra):
                checklist = load_checklist(run)
                rows(run, *(f"| {uid} | {', '.join(c['rules']) or '-'} | x | `{quote}` | n/a | | "
                            "the test code has no case these rules name |" for uid, c in checklist.items()), *extra)
                assert rc.verdict(run.name) == 0

            review(side_run, "return 2", "| u1 | - | util.py:2 | `return 2` | ask | may change a caller | caller expects 1 |")
            open_verdict = (side_run / "03-verdict.md").read_text()
            assert "0 fix, 1 ask" in open_verdict
            sh("git", "switch", "-q", "main")
            commit({"main.py": "value = 1\n"}, "main first")
            main_head = commit({"main.py": "value = 2\n"}, "main second")
            rc.start(f"{base}..{main_head}")
            main_run = Path("tmp/review-check") / f"range-{base[:7]}-{main_head[:7]}-01"
            review(main_run, "value = 2")
            assert "Verdict: APPROVE" in (main_run / "03-verdict.md").read_text()
            carry = "- main.py `value = 2`: user approved the main value in the contract\n"
            (main_run / "answers.md").write_text(carry)
            sh("git", "merge", "--no-ff", "-qm", "merge side", "side")
            merged = sh("git", "rev-parse", "HEAD")
            folders = set(Path("tmp/review-check").iterdir())
            for prior_verdict in (open_verdict, None, "Status: blocked gate failed\nUses:\nVerdict: INCOMPLETE\n"):
                result = side_run / "03-verdict.md"
                if prior_verdict is None:
                    result.unlink()
                else:
                    result.write_text(prior_verdict)
                for target in (f"{main_head}..{merged}", "local"):
                    try:
                        rc.start(target)
                        raise AssertionError("the open side-branch run must block the merge review")
                    except SystemExit as e:
                        assert side_run.name in str(e), e
                    assert set(Path("tmp/review-check").iterdir()) == folders
            review(side_run, "return 2")
            rc.start(f"{main_head}..{merged}")
            merge_run = Path("tmp/review-check") / f"range-{main_head[:7]}-{merged[:7]}-01"
            assert (merge_run / "answers-before.md").read_text() == carry, "the deepest run still owns carry-forward"
        finally:
            os.chdir(previous)


def test_same_head_carry():
    """Creation time selects carry-forward; equal times use the folder name."""
    previous = Path.cwd()
    with tempfile.TemporaryDirectory() as d:
        try:
            os.chdir(d)
            sh("git", "init", "-qb", "main")
            sh("git", "config", "user.email", "t@t")
            sh("git", "config", "user.name", "t")
            first = commit({"util.py": "def helper():\n    return 1\n"}, "carry base")
            sh("git", "commit", "--allow-empty", "-qm", "second base")
            second = sh("git", "rev-parse", "HEAD")
            head = commit({"util.py": "def helper():\n    return 2\n"}, "carry head")
            older_base, newer_base = sorted((first, second), reverse=True)

            def run_for(base):
                rc.start(f"{base}..{head}")
                run = Path("tmp/review-check") / f"range-{base[:7]}-{head[:7]}-01"
                checklist = load_checklist(run)
                uid, checks = next(iter(checklist.items()))
                rows(run, f"| {uid} | {', '.join(checks['rules']) or '-'} | x | `return 2` | n/a | | "
                     "the test code has no case these rules name |",
                     f"| {uid} | - | util.py:2 | `return 2` | ask | may change a caller | caller expects 1 |")
                assert rc.verdict(run.name) == 0
                return run

            older = run_for(older_base)
            assert "0 fix, 1 ask" in (older / "03-verdict.md").read_text()
            newer = run_for(newer_base)
            newer_answer = "- util.py `return 2`: user approved the value in the newer review\n"
            (newer / "answers.md").write_text(newer_answer)
            assert rc.verdict(newer.name) == 0
            older_answer = "- util.py `return 2`: user approved the value in the older review\n"
            (older / "answers.md").write_text(older_answer)
            assert rc.verdict(older.name) == 0
            os.utime(older / "target.json", (1000, 1000))
            os.utime(newer / "target.json", (2000, 2000))
            os.utime(older, (3000, 3000))
            Path("util.py").write_text("def helper():\n    return 3\n")
            rc.start("local")
            local = Path("tmp/review-check/local-01")
            assert (local / "answers-before.md").read_text() == newer_answer, "carry-forward must use the newer target.json"

            Path("合意.py").write_text("value = '合意'\n", encoding="utf-8")
            sh("git", "config", "core.quotepath", "false")
            write_text = Path.write_text
            encodings = {}

            def ascii_write(path, text, *args, **kwargs):
                encodings[path.name] = kwargs.get("encoding")
                kwargs.setdefault("encoding", "ascii")
                return write_text(path, text, *args, **kwargs)

            with patch.object(Path, "write_text", ascii_write):
                rc.start("local")
            assert encodings["01-units.md"] == encodings["02-review.md"] == "utf-8"
            unicode_run = Path("tmp/review-check/local-02")
            assert "合意.py" in (unicode_run / "01-units.md").read_text(encoding="utf-8")
            assert rc.revision(unicode_run / "02-review.md")

            os.utime(older / "target.json", (2000, 2000))
            glob = Path.glob
            carried = []
            for reverse in (False, True):
                def ordered_glob(path, pattern, *args, **kwargs):
                    found = glob(path, pattern, *args, **kwargs)
                    return iter(sorted(found, reverse=reverse)) if pattern == "*/target.json" else found

                with patch.object(Path, "glob", ordered_glob):
                    rc.start("local")
                tied_run = Path("tmp/review-check") / f"local-{3 + len(carried):02d}"
                carried.append((tied_run / "answers-before.md").read_text(encoding="utf-8"))
            assert carried == [older_answer, older_answer], "equal mtimes must carry from the same folder in either glob order"
        finally:
            os.chdir(previous)


def test_superseded_run():
    """Only the newest run at each ancestor head guards a new range."""
    previous = Path.cwd()
    with tempfile.TemporaryDirectory() as d:
        try:
            os.chdir(d)
            sh("git", "init", "-qb", "main")
            sh("git", "config", "user.email", "t@t")
            sh("git", "config", "user.name", "t")
            base = commit({"util.py": "def helper():\n    return 1\n"}, "superseded base")
            head = commit({"util.py": "def helper():\n    return 2\n"}, "superseded head")
            prefix = f"range-{base[:7]}-{head[:7]}"
            rc.start(f"{base}..{head}")
            older = Path("tmp/review-check") / f"{prefix}-01"
            assert rc.verdict(older.name) == 1
            assert "Verdict: INCOMPLETE" in (older / "03-verdict.md").read_text(encoding="utf-8")
            rc.start(f"{base}..{head}")
            newer = Path("tmp/review-check") / f"{prefix}-02"

            def review(run, ask=False):
                checklist = load_checklist(run)
                extra = ["| u1 | - | util.py:2 | `return 2` | ask | may change a caller | caller expects 1 |"] if ask else []
                rows(run, *(f"| {uid} | {', '.join(c['rules']) or '-'} | x | `return 2` | n/a | | "
                            "the test code has no case these rules name |" for uid, c in checklist.items()), *extra)
                assert rc.verdict(run.name) == 0

            review(newer)
            assert "Verdict: APPROVE" in (newer / "03-verdict.md").read_text(encoding="utf-8")
            os.utime(older / "target.json", (1000, 1000))
            os.utime(newer / "target.json", (2000, 2000))
            changed = commit({"util.py": "def helper():\n    return 3\n"}, "after superseded head")
            rc.start(f"{head}..{changed}")
            next_run = Path("tmp/review-check") / f"range-{head[:7]}-{changed[:7]}-01"
            assert next_run.is_dir(), "a newer APPROVE supersedes the older INCOMPLETE run"

            review(older)
            review(newer, ask=True)
            folders = set(Path("tmp/review-check").iterdir())
            try:
                rc.start(f"{head}..{changed}")
                raise AssertionError("the newer open ask must block despite the older APPROVE")
            except SystemExit as e:
                assert newer.name in str(e), e
            assert set(Path("tmp/review-check").iterdir()) == folders
        finally:
            os.chdir(previous)


def test_run_numbering():
    """Deleting an earlier run leaves the next number above every remaining run."""
    previous = Path.cwd()
    with tempfile.TemporaryDirectory() as d:
        try:
            os.chdir(d)
            sh("git", "init", "-qb", "main")
            sh("git", "config", "user.email", "t@t")
            sh("git", "config", "user.name", "t")
            base = commit({"util.py": "value = 1\n"}, "numbering base")
            head = commit({"util.py": "value = 2\n"}, "numbering head")
            prefix = f"range-{base[:7]}-{head[:7]}"
            for number in range(1, 4):
                rc.start(f"{base}..{head}")
                assert (Path("tmp/review-check") / f"{prefix}-{number:02d}").is_dir()
            shutil.rmtree(Path("tmp/review-check") / f"{prefix}-01")
            rc.start(f"{base}..{head}")
            assert (Path("tmp/review-check") / f"{prefix}-04").is_dir()
        finally:
            os.chdir(previous)


def test_patch_errors():
    script = HERE / "review_check.py"
    for args, message in ((["--patch"], "--patch needs a file"),
                          (["--patch", "missing.patch"], "missing.patch")):
        result = subprocess.run([sys.executable, script, "start", "local", *args],
                                capture_output=True, text=True)
        assert result.returncode == 1, result
        assert message in result.stderr and "Traceback" not in result.stderr, result.stderr
        assert len(result.stderr.splitlines()) == 1, result.stderr


def test_cli_encoding():
    """An ASCII console escapes Unicode rows without changing the verdict or exit code."""
    previous = Path.cwd()
    with tempfile.TemporaryDirectory() as d:
        try:
            os.chdir(d)
            sh("git", "init", "-qb", "main")
            sh("git", "config", "user.email", "t@t")
            sh("git", "config", "user.name", "t")
            base = commit({"util.py": "def helper():\n    return 1\n"}, "console base")
            head = commit({"util.py": "def helper():\n    return '合意'\n"}, "console head")
            target = f"{base}..{head}"
            rc.start(target)
            run = Path("tmp/review-check") / f"range-{base[:7]}-{head[:7]}-01"
            checklist = load_checklist(run)
            rows(run, *(f"| {uid} | {', '.join(c['rules']) or '-'} | x | `return '合意'` | n/a | | "
                        "the test code has no case these rules name |" for uid, c in checklist.items()),
                 "| u1 | - | util.py:2 | `return '合意'` | note | helper → 合意 | caller logs the helper result |")
            script = str(HERE / "review_check.py")
            with TextIOWrapper(BytesIO(), encoding="ascii") as output:
                with redirect_stdout(output), patch.object(sys, "argv", [script, "verdict", run.name]):
                    try:
                        runpy.run_path(script, run_name="__main__")
                        raise AssertionError("the CLI must exit with its verdict result")
                    except SystemExit as e:
                        assert e.code == 0, e
                output.flush()
                assert "helper \\u2192 \\u5408\\u610f" in output.buffer.getvalue().decode("ascii")
            verdict = (run / "03-verdict.md").read_text(encoding="utf-8")
            assert "Verdict: APPROVE" in verdict and "helper → 合意" in verdict
            output = StringIO()
            with redirect_stdout(output), patch.object(sys, "argv", [script, "verdict", run.name]):
                try:
                    runpy.run_path(script, run_name="__main__")
                    raise AssertionError("the CLI must exit with its verdict result")
                except SystemExit as e:
                    assert e.code == 0, e
            assert "helper → 合意" in output.getvalue()

            env = dict(os.environ, LC_ALL="C", PYTHONUTF8="0", PYTHONCOERCECLOCALE="0", PYTHONIOENCODING="ascii")
            result = subprocess.run([sys.executable, script, "start", target], env=env,
                                    capture_output=True, encoding="utf-8", errors="replace")
            assert result.returncode == 0, result.stderr
            next_run = Path("tmp/review-check") / f"range-{base[:7]}-{head[:7]}-02"
            assert "return '合意'" in (next_run / "head/util.py").read_text(encoding="utf-8")
        finally:
            os.chdir(previous)
def test_rust_function_range():
    """universal-ctags gives a Rust fn no end line, so the unit must still span the whole body,
    or a pass row that quotes a body line fails its quote check."""
    with tempfile.TemporaryDirectory() as d:
        src = Path(d) / "notice.rs"
        src.write_text('fn waiting_notice(new: &str) -> Option<String> {\n'
                       '    if new != "waiting" {\n        return None;\n    }\n'
                       '    Some(format!("{} needs you", new))\n}\n\nfn other() {}\n')
        assert ("waiting_notice", 1, 6) in rc.functions(src), rc.functions(src)


def test_catalog():
    """Rule IDs are unique, every regex compiles, and every `Check:` has bad, good, and exception fixtures."""
    catalog = rc.load_rules(sorted((HERE.parent / "references").glob("lens*.md")))
    ids = [r["id"] for r in catalog]
    assert len(ids) == len(set(ids)), sorted(i for i in ids if ids.count(i) > 1)
    for r in catalog:
        assert r["scope"] in (None, "added"), r["id"]
        for c in r["check"]:
            tool, _, name = c.partition(":")
            fixtures = HERE.parent / "fixtures" / tool / name
            assert tool == "oxlint" and all(list(fixtures.glob(f"{k}.*")) for k in ("bad", "good", "exception")), c


def test_fixtures():
    """Each `Check:` rule hits every `// expect` line of bad.* and nothing in good.* or exception.*, with the
    oxlint on PATH (for a repo's own version: PATH=<repo>/node_modules/.bin:$PATH)."""
    if not shutil.which("oxlint"):
        print("fixtures skipped: oxlint is not on PATH")
        return
    root = HERE.parent / "fixtures" / "oxlint"
    for r in rc.load_rules(sorted((HERE.parent / "references").glob("lens*.md"))):
        for c in r["check"]:
            rule = c.partition(":")[2]
            for f in sorted((root / rule).iterdir()):
                out = subprocess.run(["oxlint", "--type-aware", "-A", "all", "-D", rule, "-f", "json", str(f)],
                                     cwd=root, capture_output=True, text=True).stdout
                hits = {d["labels"][0]["span"]["line"] for d in json.loads(out)["diagnostics"]}
                want = {i for i, l in enumerate(f.read_text().splitlines(), 1) if "// expect" in l}
                assert hits == want, (r["id"], f.name, hits, want)


if __name__ == "__main__":
    test_pid_helpers()
    test_build_ignored_hangup()
    test_shards()
    test_rust_function_range()
    test_catalog()
    test_fixtures()
    with tempfile.TemporaryDirectory() as d, tempfile.TemporaryDirectory() as home:
        os.chdir(d)
        os.environ["HOME"] = home  # the repo rules file is read from ~/.review-check
        sh("git", "init", "-qb", "main")
        sh("git", "config", "user.email", "t@t")
        sh("git", "config", "user.name", "t")
        main()
        test_patch_errors()
        test_build_gate()
        test_repo_rules_origin()
        test_seat_gaps()
        test_frozen_build()
        test_cli_encoding()
        test_superseded_run()
        test_answers()
        test_limits()
        test_stale_verdict_guard()
        test_merge_guard()
        test_same_head_carry()
        test_run_numbering()
    print("ok")
