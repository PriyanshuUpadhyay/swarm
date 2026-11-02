# Build process signals, launch cleanup, and verification locks.
from test_review_check_support import *


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

