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



def test_loop_patch_budget():
    with loop_repo() as (base, head):
        loop_round(base, head, "patch-budget")
        patch_file = Path("partial.patch")
        patch_file.write_text(rc.git("diff", *rc.PLAIN, base, head))
        rc.start(f"{base}..{head}", "--parent-loop", "patch-budget", "--patch", str(patch_file))
        final = loop_round(base, head, "patch-budget", "pass")
        assert json.loads((final / "target.json").read_text())["round"] == 2



def test_loop_uncertain_match():
    with loop_repo() as (base, head), patch.object(rc, "functions", return_value=[]):
        for _ in range(3):
            run = loop_round(base, head, "uncertain")
            assert not json.loads((run / "snapshot.json").read_text())["certain"]
        loop_refused(base, head, "uncertain", "cap")



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
        script = HERE.parents[1] / "implement" / "scripts" / "implement_run.py"
        out = subprocess.run([sys.executable, str(script), "take", str(folder), "07-close", "test"], capture_output=True, text=True)
        assert out.returncode == 0, out.stderr
        assert rc.verdict(final.name) == 0, "answer-only verdict must not spend another round"
        assert len([p for p in Path("tmp/review-check").glob("*/target.json")
                    if json.loads(p.read_text())["parent_loop"] == "new-issue"]) == 3



def test_loop_callers():
    script = HERE.parents[1] / "implement" / "scripts" / "implement_run.py"
    for caller in ("implement", "standalone-deliver"):
        with loop_repo() as (base, head):
            folder = Path("tmp/flow") / caller
            folder.mkdir(parents=True)
            (folder / "loop.json").write_text(json.dumps({"parent_loop": caller}))
            for n in range(1, 4):
                if n > 1:
                    base, head = head, commit({"value.py": f"def value():\n    return {n}\n"}, "fix")
                out = subprocess.run([sys.executable, str(script), "review", str(folder), f"{base}..{head}"],
                                     capture_output=True, text=True)
                assert out.returncode == 0, out.stderr
                run = Path(out.stdout.splitlines()[0])
                checklist = load_checklist(run)
                unit = json.loads((run / "units.json").read_text())[0]
                rows(run, f"| {unit['id']} | {', '.join(checklist[unit['id']]['rules']) or '-'} | value.py:2 | `return {n}` | n/a | | the sample has no such rule case |",
                     f"| {unit['id']} | - | value.py:2 | `return {n}` | fix | issue {n} | caller needs a different value |")
                assert rc.verdict(run.name) == 0
            replan = folder.with_name(caller + "-replan")
            replan.mkdir()
            (replan / "loop.json").write_bytes((folder / "loop.json").read_bytes())
            out = subprocess.run([sys.executable, str(script), "review", str(replan), f"{base}..{head}"],
                                 capture_output=True, text=True)
            assert out.returncode == 1 and "3 rounds used" in out.stderr, out



def test_loop_new_problem_same_evidence():
    with loop_repo() as (base, head):
        loop_round(base, head, "new-problem")
        changed = commit({"value.py": "def value():\n    assert True\n    return 1\n"}, "fix A and expose B")
        second = loop_round(base, changed, "new-problem")
        review = second / "02-review.md"
        review.write_text(review.read_text().replace("value issue", "new issue after the guard"))
        assert rc.verdict(second.name) == 0
        final = loop_round(base, changed, "new-problem", "pass")
        assert "Verdict: APPROVE" in (final / "03-verdict.md").read_text()



def test_loop_broken_snapshot():
    for broken in ("{", "[]", "null", "{}", '{"certain": true}'):
        with loop_repo() as (base, head):
            loop_round(base, head, "broken-snapshot")
            second = loop_round(base, head, "broken-snapshot")
            (second / "snapshot.json").write_text(broken)
            third = loop_round(base, head, "broken-snapshot", "pass")
            assert json.loads((third / "target.json").read_text())["round"] == 3
            loop_refused(base, head, "broken-snapshot", "cap")



def test_loop_snapshot_atomic_without_lock():
    with loop_repo() as (base, head):
        run = loop_round(base, head, "snapshot-lock")
        shared = Path(rc.git("rev-parse", "--path-format=absolute", "--git-common-dir").strip()) / "review-check"
        written = []
        original = rc.step_run.write_step

        def write(file, text):
            if file.name == "snapshot.json":
                with rc.step_run.step_lock(shared, "review-check", command="test"):
                    pass
                written.append(file)
            original(file, text)

        with patch.object(rc.step_run, "write_step", side_effect=write):
            assert rc.verdict(run.name) == 0
        assert written == [run.resolve() / "snapshot.json"], "snapshot write bypassed the atomic writer"
        with rc.step_run.step_lock(shared, "review-check", command="test"), \
                patch.object(rc, "START_LOCK_WAIT", 0.05, create=True):
            started = time.monotonic()
            try:
                rc.start(f"{base}..{head}", "--parent-loop", "snapshot-lock")
            except SystemExit as error:
                assert "locked" in str(error) and "waited" in str(error), error
                assert 0.05 <= time.monotonic() - started < 2
            else:
                raise AssertionError("start bypassed the shared snapshot lock")
            assert rc.verdict(run.name) == 0, "verdict must work while another start owns the lock"

