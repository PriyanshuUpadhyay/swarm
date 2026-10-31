# Shared loop ledgers and linked worktree budgets.
from test_review_check_support import *


def test_loop_readonly_common_dir():
    for existing in (False, True):
        with loop_repo() as (base, head):
            common = Path(rc.git("rev-parse", "--path-format=absolute", "--git-common-dir").strip())
            if existing:
                loop_round(base, head, "readonly", "pass")
            paths = [common, common / "info", common / "info" / "exclude", common / "review-check"]
            paths = [p for p in paths if p.exists()]
            modes = {p: p.stat().st_mode & 0o777 for p in paths}
            try:
                for p in paths:
                    p.chmod(0o555 if p.is_dir() else 0o444)
                for n in range(int(existing), 3):
                    run = loop_round(base, head, "readonly", "pass")
                    assert json.loads((run / "target.json").read_text())["round"] == n + 1
                with redirect_stdout(StringIO()) as out:
                    rc.start(f"{base}..{head}", "--parent-loop", "readonly-note")
                expected = "parent loop: readonly-note (local writes; the git dir is not writable)"
                assert out.getvalue().splitlines().count(expected) == 1, out.getvalue()
                loop_refused(base, head, "readonly", "cap")
                assert rc.verdict(run.name) == 0
            finally:
                for p, mode in modes.items():
                    p.chmod(mode)



def test_loop_start_waits_for_sibling():
    with loop_repo() as (base, head):
        common = Path(rc.git("rev-parse", "--path-format=absolute", "--git-common-dir").strip()) / "review-check"
        common.mkdir()
        linked = Path.cwd() / "tmp" / "sibling"
        sh("git", "worktree", "add", "--detach", str(linked), head)
        with rc.step_run.step_lock(common, "review-check", command="start"):
            contender = subprocess.Popen([sys.executable, str(HERE / "review_check.py"), "start",
                                          f"{base}..{head}", "--parent-loop", "sibling"],
                                         cwd=linked, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
            time.sleep(0.2)
            assert contender.poll() is None, contender.communicate()
        stdout, stderr = contender.communicate(timeout=15)
        assert contender.returncode == 0, (stdout, stderr)
        assert "parent loop: sibling" in stdout, stdout



def test_loop_readonly_shared_budget():
    with loop_repo() as (base, head):
        for _ in range(3):
            loop_round(base, head, "readonly-shared", "pass")
        shared = Path(rc.git("rev-parse", "--path-format=absolute", "--git-common-dir").strip()) / "review-check"
        ledger = next(shared.glob("*.json"))
        original = Path.cwd()
        linked = original / "tmp" / "readonly-sibling"
        sh("git", "worktree", "add", "--detach", str(linked), head)
        shared_mode, ledger_mode = shared.stat().st_mode & 0o777, ledger.stat().st_mode & 0o777
        before = ledger.read_bytes()
        try:
            os.chdir(linked)
            shared.chmod(0o555)
            loop_refused(base, head, "readonly-shared", "cap")
            assert ledger.read_bytes() == before
            for broken in (None, "{"):
                if broken is None:
                    ledger.chmod(0)
                else:
                    ledger.chmod(ledger_mode)
                    ledger.write_text(broken)
                try:
                    rc.start(f"{base}..{head}", "--parent-loop", "readonly-shared")
                except SystemExit as error:
                    assert "cannot read loop ledger" in str(error) and str(ledger) in str(error), error
                else:
                    raise AssertionError("accepted an unreadable or damaged shared ledger")
            assert not list(Path("tmp/review-check").glob("*/target.json"))
        finally:
            ledger.chmod(ledger_mode)
            shared.chmod(shared_mode)
            os.chdir(original)



def test_loop_writable_after_readonly_rounds():
    with loop_repo() as (base, head):
        first = loop_round(base, head, "mixed-mode", "pass")
        shared = Path(rc.git("rev-parse", "--path-format=absolute", "--git-common-dir").strip()) / "review-check"
        ledger = next(shared.glob("*.json"))
        mode = shared.stat().st_mode & 0o777
        runs = [str(first.resolve())]
        try:
            shared.chmod(0o555)
            for n in (2, 3):
                run = loop_round(base, head, "mixed-mode", "pass")
                assert json.loads((run / "target.json").read_text())["round"] == n
                runs.append(str(run.resolve()))
            assert json.loads(ledger.read_text())["used"] == 1
        finally:
            shared.chmod(mode)
        loop_refused(base, head, "mixed-mode", "cap")
        assert json.loads(ledger.read_text()) == {"used": 3, "continued": False, "runs": runs}
        Path("owner.md").write_text("continue: mixed-mode\n")
        try:
            shared.chmod(0o555)
            fourth = loop_round(base, head, "mixed-mode", "pass", "return 1", "--continue", "owner.md")
            runs.append(str(fourth.resolve()))
        finally:
            shared.chmod(mode)
        loop_refused(base, head, "mixed-mode", "cap", "--continue", "owner.md")
        assert json.loads(ledger.read_text()) == {"used": 4, "continued": True, "runs": runs}



def test_loop_broken_ledger():
    for broken in ("{", "[]", "null", "{}", '{"used": 1}',
                   '{"used": "1", "continued": false, "runs": []}',
                   '{"used": 1, "continued": false, "runs": [7]}',
                   '{"used": -1, "continued": false, "runs": []}'):
        with loop_repo() as (base, head):
            loop_round(base, head, "broken-ledger", "pass")
            common = Path(rc.git("rev-parse", "--path-format=absolute", "--git-common-dir").strip()) / "review-check"
            ledger = next(common.glob("*.json"))
            ledger.write_text(broken)
            try:
                rc.start(f"{base}..{head}", "--parent-loop", "broken-ledger")
            except SystemExit as error:
                assert str(ledger) in str(error) and "delete" in str(error), error
            else:
                raise AssertionError(f"accepted damaged ledger: {broken}")
            assert ledger.read_text() == broken



def test_loop_shared_worktree_budget():
    with loop_repo() as (base, head):
        for _ in range(3):
            loop_round(base, head, "shared-task", "pass")
        original = Path.cwd()
        linked = original / "tmp" / "replan"
        sh("git", "worktree", "add", "--detach", str(linked), head)
        try:
            os.chdir(linked)
            loop_refused(base, head, "shared-task", "cap")
            Path("owner.md").write_text("continue: shared-task\n")
            fourth = loop_round(base, head, "shared-task", "pass", "return 1", "--continue", "owner.md")
            assert json.loads((fourth / "target.json").read_text())["round"] == 4
        finally:
            os.chdir(original)
        Path("owner.md").write_text("continue: shared-task\n")
        loop_refused(base, head, "shared-task", "cap", "--continue", "owner.md")



def test_loop_deleted_run_budget():
    with loop_repo() as (base, head):
        for _ in range(3):
            latest = loop_round(base, head, "deleted-run", "pass")
        shutil.rmtree(latest)
        loop_refused(base, head, "deleted-run", "cap")



def test_loop_import_budget_on_refusal():
    with loop_repo() as (base, head):
        for _ in range(3):
            loop_round(base, head, "imported-task", "pass")
        shared = Path(rc.git("rev-parse", "--path-format=absolute", "--git-common-dir").strip()) / "review-check"
        # An old checkout has target.json history, but no shared ledger yet.
        for ledger in shared.glob("*.json"):
            ledger.unlink()
        loop_refused(base, head, "imported-task", "cap")
        original = Path.cwd()
        linked = original / "tmp" / "replan"
        sh("git", "worktree", "add", "--detach", str(linked), head)
        try:
            os.chdir(linked)
            loop_refused(base, head, "imported-task", "cap")
        finally:
            os.chdir(original)



def test_loop_spent_extension_message():
    with loop_repo() as (base, head):
        for _ in range(3):
            loop_round(base, head, "spent-extension", "pass")
        Path("owner.md").write_text("continue: spent-extension\n")
        loop_round(base, head, "spent-extension", "pass", "return 1", "--continue", "owner.md")
        try:
            rc.start(f"{base}..{head}", "--parent-loop", "spent-extension")
        except SystemExit as error:
            assert "extension is used" in str(error) and "open rows" in str(error), error
            assert "the user can write" not in str(error), error
        else:
            raise AssertionError("accepted a fifth round")

