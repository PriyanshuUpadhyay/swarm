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
            review("the removed line could crash too", quote="return 1", line=2)
            assert rc.verdict(run.name) == 0
            assert "0 fix, 0 ask, 1 note" in (run / "03-verdict.md").read_text(), "a crash note on removed code stays a note"
            # lint checks one seat file as it is written: a quote outside the unit goes back to the seat at once.
            review("the name could be clearer")
            out = StringIO()
            with redirect_stdout(out):
                assert rc.verdict(run.name, "02-review.md") == 0
            assert out.getvalue().strip() == "02-review.md: rows ok", out.getvalue()
            assert not (run / "03-verdict.md").read_text().startswith("Status: blocked"), "lint writes no verdict"
            review("the name could be clearer", quote="return 3")
            before = (run / "03-verdict.md").read_text()
            out = StringIO()
            with redirect_stdout(out):
                assert rc.verdict(run.name, "02-review.md") == 1
            assert "quote not found at util.py:2" in out.getvalue(), out.getvalue()
            assert (run / "03-verdict.md").read_text() == before, "lint leaves the verdict file as it was"
            try:
                rc.verdict(run.name, "02-review-typo.md")
                raise AssertionError("lint of a missing seat file passed")
            except SystemExit as error:
                assert "no such file" in str(error), error
            (run / "02-review-empty.md").write_text("| Unit | Rule |\n")
            out = StringIO()
            with redirect_stdout(out):
                assert rc.verdict(run.name, "02-review-empty.md") == 1
            assert "nothing was checked" in out.getvalue(), out.getvalue()
            (run / "02-review-empty.md").unlink()
            usage = subprocess.run([sys.executable, "-I", str(Path(rc.__file__)), "lint", run.name], capture_output=True, text=True)
            assert usage.returncode == 1 and "Traceback" not in usage.stderr, usage
            usage = subprocess.run([sys.executable, "-I", str(Path(rc.__file__)), "verdict", run.name, "02-review.md"], capture_output=True, text=True)
            assert usage.returncode == 1 and "rows ok" not in usage.stdout and "Traceback" not in usage.stderr, usage
            review("helper crashes on a None argument", quote="def helper(s):", line=1)
            assert rc.verdict(run.name) == 0
            assert "0 fix, 0 ask, 1 note" in (run / "03-verdict.md").read_text(), "a crash on a context line stays a note"
        finally:
            os.chdir(previous)


def test_taste_note():
    """A note on the owner's TASTE- rule at an added line counts as a fix."""
    previous, old_home = Path.cwd(), os.environ.get("HOME")
    with tempfile.TemporaryDirectory() as d:
        try:
            os.environ["HOME"] = d
            (Path(d) / ".review-walk").mkdir()
            (Path(d) / ".review-walk" / "taste.md").write_text(
                "# Taste\n\nFiles: `**/*.py`\n\n- `TASTE-13` Keys kept in sync by hand with another place → one source.\n")
            os.mkdir(Path(d) / "repo")
            os.chdir(Path(d) / "repo")
            sh("git", "init", "-qb", "main")
            sh("git", "config", "user.email", "t@t")
            sh("git", "config", "user.name", "t")
            base = commit({"util.py": "def helper(c):\n    return c\n"}, "taste base")
            head = commit({"util.py": "KEYS = {'path', 'body'}\n\n\ndef helper(c):\n    return c\n"}, "taste change")
            rc.start(f"{base}..{head}")
            run = Path("tmp/review-check") / f"range-{base[:7]}-{head[:7]}-01"
            uid, checks = next(iter(load_checklist(run).items()))
            assert "TASTE-13" in checks["rules"], checks
            rest = [r for r in checks["rules"] if r != "TASTE-13"]
            rows(run, f"| {uid} | {', '.join(rest) or '-'} | x | `def helper(c):` | n/a | | the unit has no case these rules name |",
                 f"| {uid} | TASTE-13 | util.py:1 | `KEYS = {{'path', 'body'}}` | note | KEYS repeats the keys post reads | post reads c['path'] |")
            assert rc.verdict(run.name) == 0
            verdict = (run / "03-verdict.md").read_text()
            assert "1 fix, 0 ask, 0 note" in verdict, verdict
        finally:
            os.chdir(previous)
            os.environ["HOME"] = old_home
