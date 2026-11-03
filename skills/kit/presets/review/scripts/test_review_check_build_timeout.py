# Build timeouts, pushed CI results, and frozen build checks.
from test_review_check_support import *


def test_build_timeout(run, verify):
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


def check_verify_recreated(run, verify, head):
    shutil.rmtree(verify)
    rc.build(run.name, "--run")
    assert sh("git", "-C", str(verify), "rev-parse", "HEAD") == head
    assert json.loads((run / "build.json").read_text())["conclusion"] == "success"


def check_verify_reset(run, verify):
    (verify / "value.txt").write_text("dirty verify tree\n")
    (verify / "output.txt").write_text("untracked output\n")
    rc.build(run.name, "--run")
    assert (verify / "value.txt").read_text() == "later\n" and not (verify / "output.txt").exists()
    assert json.loads((run / "build.json").read_text())["conclusion"] == "success"


def check_verify_lock_refuses(run, verify):
    original = (run / "build.json").read_text()
    with rc.step_run.step_lock(verify.parent, "verify", command="build --run"):
        try:
            rc.build(run.name, "--run")
            raise AssertionError("build must refuse a held verify lock")
        except SystemExit:
            pass
    assert (run / "build.json").read_text() == original


def test_pushed_build_without_verify(run, verify, head):
    verify.mkdir()
    gh_result = {"check_runs": [{"id": 1, "status": "completed", "conclusion": "success",
                                 "html_url": "https://example.invalid/check/1"}]}
    real_run = subprocess.run

    def stub_gh(args, **kwargs):
        if args[0] == "gh":
            return subprocess.CompletedProcess(args, 0, json.dumps(gh_result), "")
        return real_run(args, **kwargs)

    with patch.object(rc.subprocess, "run", stub_gh):
        rc.build(run.name)
    result = json.loads((run / "build.json").read_text())
    assert result["source"] == "check-run" and result["head"] == head
    assert result["conclusion"] == "success" and result["tail"] == ["https://example.invalid/check/1"]
    assert verify.is_dir() and not (verify.parent / "verify.lock").exists()



def test_pushed_build_missing_gh(run):
    original = (run / "build.json").read_text(encoding="utf-8")
    real_run = subprocess.run

    def missing_gh(args, **kwargs):
        if args[0] == "gh":
            raise FileNotFoundError("gh")
        return real_run(args, **kwargs)

    with patch.object(rc.subprocess, "run", missing_gh):
        try:
            rc.build(run.name)
            raise AssertionError("a missing gh must refuse build proof")
        except SystemExit:
            pass
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
            check_verify_recreated(run, verify, head)
            check_verify_lock_refuses(run, verify)
            test_build_timeout(run, verify)
            target = json.loads((run / "target.json").read_text())
            target["head"] = moved
            target["ci"]["cmd"] = "test $(cat value.txt) = later"
            (run / "target.json").write_text(json.dumps(target))
            rc.build(run.name, "--run")
            assert sh("git", "-C", str(verify), "rev-parse", "HEAD") == moved, "verify did not move to the next head"
            check_verify_reset(run, verify)
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

