# Loop limits, issue identity, and snapshot checks.
from test_review_check_support import *


def test_loop_cap():
    with loop_repo() as (base, head):
        for n in range(3):
            run = loop_round(base, head, "task-a", quote="return 1", kind="fix")
            # This case checks the cap when identity cannot be established.
            (run / "snapshot.json").unlink(missing_ok=True)
            assert json.loads((run / "target.json").read_text())["parent_loop"] == "task-a"
        loop_refused(base, head, "task-a", "cap")
        script = HERE.parents[2] / "skills" / "implement" / "scripts" / "implement_run.py"
        folder = Path("tmp/flow/implement-replan")
        folder.mkdir(parents=True)
        (folder / "loop.json").write_text(json.dumps({"parent_loop": "task-a"}))
        out = subprocess.run([sys.executable, str(script), "review", str(folder), f"{base}..{head}"],
                             capture_output=True, text=True)
        assert out.returncode == 1 and "3 rounds used" in out.stderr, out
        loop_round(base, head, "task-b")
        changed = commit({"value.py": "def value():\n    return 2\n"}, "fix and re-plan")
        loop_refused(head, changed, "task-a", "cap")
        Path("owner.md").write_text("continue: task-a\n")
        loop_round(head, changed, "task-a", "pass", "return 2", "--continue", "owner.md")
        loop_refused(head, changed, "task-a", "cap", "--continue", "owner.md")



def test_loop_default_key():
    with loop_repo() as (base, head):
        run = loop_round(base, head)
        assert json.loads((run / "target.json").read_text())["parent_loop"] == "branch:main"
        sh("git", "checkout", "--detach", "-q", head)
        try:
            rc.start(f"{base}..{head}")
        except SystemExit as error:
            assert "--parent-loop" in str(error) and "detached" in str(error), str(error)
        else:
            raise AssertionError("detached HEAD accepted without a parent key")



def test_loop_answers():
    with loop_repo() as (base, head):
        run = loop_round(base, head, "answers", "ask")
        (run / "answers.md").write_text("- value.py `return 1`: the user approved this value in the task contract\n")
        assert rc.verdict(run.name) == 0
        assert not json.loads((run / "snapshot.json").read_text())["open"]
        assert json.loads((run / "target.json").read_text())["round"] == 1
        assert len(list(Path("tmp/review-check").glob("*/target.json"))) == 1
        loop_round(base, head, "answers", "pass")



def test_loop_snapshots():
    with loop_repo() as (base, head):
        first = loop_round(base, head, "same-issues")
        snapshot = json.loads((first / "snapshot.json").read_text())
        assert snapshot["open"] and snapshot["checks"], snapshot
        changed = commit({"value.py": "def value():\n    # moved\n    # evidence\n    return 1\n"}, "move lines")
        loop_round(base, changed, "same-issues")
        loop_refused(base, changed, "same-issues", "no progress")
        Path("owner.md").write_text("continue: same-issues\n")
        loop_round(base, changed, "same-issues", "pass", "return 1", "--continue", "owner.md")
    with loop_repo() as (base, head):
        loop_round(base, head, "new-issue")
        changed = commit({"value.py": "def value():\n    return 2\n"}, "fix A and expose B")
        loop_round(head, changed, "new-issue", quote="return 2")
        final = loop_round(head, changed, "new-issue", "pass", "return 2")
        assert "Verdict: APPROVE" in (final / "03-verdict.md").read_text()
        folder = Path("tmp/flow/close-on-three")
        folder.mkdir(parents=True)
        (folder / "06-review.md").write_text(f"Status: done x\nUses:\n\nRun: tmp/review-check/{final.name}\n")
        (folder / "07-close.md").write_text("Status: open\nUses: 06-review\n\n## Result\n")
        script = HERE.parents[2] / "skills" / "implement" / "scripts" / "implement_run.py"
        out = subprocess.run([sys.executable, str(script), "take", str(folder), "07-close", "test"], capture_output=True, text=True)
        assert out.returncode == 0, out.stderr
        assert rc.verdict(final.name) == 0, "answer-only verdict must not spend another round"
        assert len([p for p in Path("tmp/review-check").glob("*/target.json")
                    if json.loads(p.read_text())["parent_loop"] == "new-issue"]) == 3



def test_loop_broken_snapshot():
    for broken in ("{",):
        with loop_repo() as (base, head):
            loop_round(base, head, "broken-snapshot")
            second = loop_round(base, head, "broken-snapshot")
            (second / "snapshot.json").write_text(broken)
            third = loop_round(base, head, "broken-snapshot", "pass")
            assert json.loads((third / "target.json").read_text())["round"] == 3
            loop_refused(base, head, "broken-snapshot", "cap")
