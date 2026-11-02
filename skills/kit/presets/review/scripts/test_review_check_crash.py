# A note that names a crash on new code counts as a fix (kit-right-size council 2026-11-02, A2).
from test_review_check_support import *


def test_crash_note():
    """A crash note on a new line gives REQUEST CHANGES; a limit: line closes it; an old-code note stays a note."""
    previous = Path.cwd()
    with tempfile.TemporaryDirectory() as d:
        try:
            os.chdir(d)
            sh("git", "init", "-qb", "main")
            sh("git", "config", "user.email", "t@t")
            sh("git", "config", "user.name", "t")
            base = commit({"util.py": "def helper(s):\n    return 1\n"}, "crash base")
            head = commit({"util.py": "def helper(s):\n    return int(s[:2])\n"}, "crash change")
            rc.start(f"{base}..{head}")
            run = Path("tmp/review-check") / f"range-{base[:7]}-{head[:7]}-01"

            def review(problem, quote="return int(s[:2])", line=2):
                uid, checks = next(iter(load_checklist(run).items()))
                rows(run, f"| {uid} | {', '.join(checks['rules']) or '-'} | x | `{quote}` | n/a | | "
                     "the test code has no case these rules name |",
                     f"| {uid} | - | util.py:{line} | `{quote}` | note | {problem} | stored value \"March\" |")

            review("potential issue, not confirmed: int() crashes with a ValueError traceback")
            assert rc.verdict(run.name) == 0
            verdict = (run / "03-verdict.md").read_text()
            assert verdict.splitlines()[2].startswith("Verdict: REQUEST CHANGES"), verdict
            assert "1 fix, 0 ask, 0 note" in verdict, verdict
            (run / "answers.md").write_text("- util.py `return int(s[:2])`: limit: user accepts the crash on a bad stored date\n")
            assert rc.verdict(run.name) == 0
            verdict = (run / "03-verdict.md").read_text()
            assert verdict.splitlines()[2].startswith("Verdict: APPROVE") and "0 fix, 0 ask, 0 note, 0 answered, 1 limit" in verdict, verdict
            (run / "answers.md").unlink()
            for words in ("the script exits 1 on an empty file", "a panic on a nil map", "an uncaught KeyError"):
                review(words)
                assert rc.verdict(run.name) == 0
                assert "1 fix, 0 ask, 0 note" in (run / "03-verdict.md").read_text(), words
            review("the name could be clearer")
            assert rc.verdict(run.name) == 0
            assert "0 fix, 0 ask, 1 note" in (run / "03-verdict.md").read_text()
            review("the removed line could crash too", quote="return 1", line=2)
            assert rc.verdict(run.name) == 0
            assert "0 fix, 0 ask, 1 note" in (run / "03-verdict.md").read_text(), "a crash note on removed code stays a note"
        finally:
            os.chdir(previous)
