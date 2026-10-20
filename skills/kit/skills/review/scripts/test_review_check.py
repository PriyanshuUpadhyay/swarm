"""Run: python3 test_review_check.py. Builds a throwaway repo and checks that the verdict is earned:
a unit with no row, a rule with no result, a made-up quote, a quote at the wrong line, or a bare "ok"
proof blocks it; a language with no ctags support still gets units; a changed function that others
call gets REF; the build gate owns the IDs the repo's lint config proves, and only for this head."""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from test_review_check_support import *
from test_review_check_loop_snapshots import *
from test_review_check_loop_ledger import *
from test_review_check_catalog import *
from test_review_check_build_process import *
from test_review_check_build_timeout import *
from test_review_check_answers import *
from test_review_check_history import *


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


if __name__ == "__main__":
    test_loop_cap()
    test_loop_default_key()
    test_loop_patch_budget()
    test_loop_uncertain_match()
    test_loop_answers()
    test_loop_snapshots()
    test_loop_callers()
    test_loop_new_problem_same_evidence()
    test_loop_broken_snapshot()
    test_loop_snapshot_atomic_without_lock()
    test_loop_readonly_common_dir()
    test_loop_start_waits_for_sibling()
    test_loop_readonly_shared_budget()
    test_loop_writable_after_readonly_rounds()
    test_loop_broken_ledger()
    test_loop_shared_worktree_budget()
    test_loop_deleted_run_budget()
    test_loop_import_budget_on_refusal()
    test_loop_spent_extension_message()
    test_pid_helpers()
    test_build_ignored_hangup()
    test_shards()
    test_rust_function_range()
    test_catalog()
    test_fixtures()
    # Each legacy case is a separate task on the same default branch key, and some need many starts.
    # Keep their cap override local to this process; the loop tests above use the real cap.
    with patch.object(rc, "ROUND_CAP", 1000), tempfile.TemporaryDirectory() as d, tempfile.TemporaryDirectory() as home:
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
