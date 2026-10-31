# Build timeouts, pushed CI results, and frozen build checks.
from test_review_check_support import *
from test_review_check_build_process import test_missing_verify_directory, test_verify_tree_cleanup, test_verify_build_lock, test_pid_helpers, test_build_child_signal_mask, test_build_interrupt, test_build_ignored_hangup, test_build_interrupt_ignored_hangup, test_build_launch_deferred_signal, test_build_launch_interrupt, test_build_after_launch_interrupt


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

