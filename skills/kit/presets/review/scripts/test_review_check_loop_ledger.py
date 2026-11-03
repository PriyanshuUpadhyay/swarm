# Shared loop ledgers and linked worktree budgets.
from test_review_check_support import *


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
        # An unreadable ledger must refuse, not reset the budget.
        ledger_mode = ledger.stat().st_mode & 0o777
        try:
            ledger.chmod(0)
            rc.start(f"{base}..{head}", "--parent-loop", "mixed-mode")
        except SystemExit as error:
            assert "cannot read loop ledger" in str(error), error
        else:
            raise AssertionError("accepted an unreadable shared ledger")
        finally:
            ledger.chmod(ledger_mode)



def test_loop_broken_ledger():
    for broken in ("{",):
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
    # An old checkout has target.json history, but no shared ledger yet.
    with loop_repo() as (base, head):
        for _ in range(3):
            loop_round(base, head, "imported-task", "pass")
        shared = Path(rc.git("rev-parse", "--path-format=absolute", "--git-common-dir").strip()) / "review-check"
        for ledger in shared.glob("*.json"):
            ledger.unlink()
        loop_refused(base, head, "imported-task", "cap")
