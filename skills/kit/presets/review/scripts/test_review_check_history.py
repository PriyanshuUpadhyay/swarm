# Review history, stale inputs, ancestor guards, and run numbering.
from test_review_check_support import *


def test_stale_verdict_guard():
    """start refuses changed verdict inputs until verdict runs again, including answers.md."""
    previous = Path.cwd()
    with tempfile.TemporaryDirectory() as d:
        try:
            os.chdir(d)
            sh("git", "init", "-qb", "main")
            sh("git", "config", "user.email", "t@t")
            sh("git", "config", "user.name", "t")
            base = commit({"util.py": "def helper():\n    return 1\n"}, "stale base")
            head = commit({"util.py": "def helper():\n    return 2\n"}, "stale change")
            rc.start(f"{base}..{head}")
            run = Path("tmp/review-check") / f"range-{base[:7]}-{head[:7]}-01"
            checklist = load_checklist(run)
            uid, checks = next(iter(checklist.items()))
            rows(run, f"| {uid} | {', '.join(checks['rules']) or '-'} | x | `return 2` | n/a | | "
                 "the test code has no case these rules name |",
                 f"| {uid} | - | util.py:2 | `return 2` | ask | may change a caller | caller expects 1 |")
            (run / "answers.md").write_text("- util.py `return 2`: user approved the return value in the contract\n")
            (run / "02-review-refs.md").write_text("Status: done x\nUses:\n")
            (run / "build.json").write_text("{}\n")
            assert rc.verdict(run.name) == 0
            assert "Verdict: APPROVE" in (run / "03-verdict.md").read_text()
            changed = commit({"util.py": "def helper():\n    return 3\n"}, "after stale change")
            folders = set(Path("tmp/review-check").iterdir())
            for filename in ("answers.md", "01-units.md", "02-review.md", "02-review-refs.md", "build.json"):
                file = run / filename
                original = file.read_text()
                file.write_text(original.replace("approved", "accepts") if filename == "answers.md" else original + "\n")
                for target in (f"{head}..{changed}", "local"):
                    try:
                        rc.start(target)
                        raise AssertionError(f"start must refuse a stale verdict after {filename} changes")
                    except SystemExit as e:
                        expected = f"start refused by {run.resolve()}: its verdict is stale ({filename} changed); run `verdict {run.name}` again"
                        assert str(e) == expected, e
                    assert set(Path("tmp/review-check").iterdir()) == folders, "a stale verdict creates no folder"
                assert rc.verdict(run.name) == 0
                assert "Verdict: APPROVE" in (run / "03-verdict.md").read_text()
            rc.start(f"{head}..{changed}")
            assert (Path("tmp/review-check") / f"range-{head[:7]}-{changed[:7]}-01").is_dir()
        finally:
            os.chdir(previous)



def test_merge_guard():
    """Every ancestor review must pass the guard, even when a deeper branch is approved."""
    previous = Path.cwd()
    with tempfile.TemporaryDirectory() as d:
        try:
            os.chdir(d)
            sh("git", "init", "-qb", "main")
            sh("git", "config", "user.email", "t@t")
            sh("git", "config", "user.name", "t")
            base = commit({"util.py": "def helper():\n    return 1\n"}, "merge base")
            sh("git", "switch", "-qc", "side")
            side = commit({"util.py": "def helper():\n    return 2\n"}, "side change")
            rc.start(f"{base}..{side}")
            side_run = Path("tmp/review-check") / f"range-{base[:7]}-{side[:7]}-01"

            def review(run, quote, *extra):
                checklist = load_checklist(run)
                rows(run, *(f"| {uid} | {', '.join(c['rules']) or '-'} | x | `{quote}` | n/a | | "
                            "the test code has no case these rules name |" for uid, c in checklist.items()), *extra)
                assert rc.verdict(run.name) == 0

            review(side_run, "return 2", "| u1 | - | util.py:2 | `return 2` | ask | may change a caller | caller expects 1 |")
            open_verdict = (side_run / "03-verdict.md").read_text()
            assert "0 fix, 1 ask" in open_verdict
            sh("git", "switch", "-q", "main")
            commit({"main.py": "value = 1\n"}, "main first")
            main_head = commit({"main.py": "value = 2\n"}, "main second")
            rc.start(f"{base}..{main_head}")
            main_run = Path("tmp/review-check") / f"range-{base[:7]}-{main_head[:7]}-01"
            review(main_run, "value = 2")
            assert "Verdict: APPROVE" in (main_run / "03-verdict.md").read_text()
            carry = "- main.py `value = 2`: user approved the main value in the contract\n"
            (main_run / "answers.md").write_text(carry)
            sh("git", "merge", "--no-ff", "-qm", "merge side", "side")
            merged = sh("git", "rev-parse", "HEAD")
            folders = set(Path("tmp/review-check").iterdir())
            for prior_verdict in (open_verdict, None, "Status: blocked gate failed\nUses:\nVerdict: INCOMPLETE\n"):
                result = side_run / "03-verdict.md"
                if prior_verdict is None:
                    result.unlink()
                else:
                    result.write_text(prior_verdict)
                for target in (f"{main_head}..{merged}", "local"):
                    try:
                        rc.start(target)
                        raise AssertionError("the open side-branch run must block the merge review")
                    except SystemExit as e:
                        assert side_run.name in str(e), e
                    assert set(Path("tmp/review-check").iterdir()) == folders
            review(side_run, "return 2")
            rc.start(f"{main_head}..{merged}")
            merge_run = Path("tmp/review-check") / f"range-{main_head[:7]}-{merged[:7]}-01"
            assert (merge_run / "answers-before.md").read_text() == carry, "the deepest run still owns carry-forward"
        finally:
            os.chdir(previous)



def test_same_head_carry():
    """Creation time selects carry-forward; equal times use the folder name."""
    previous = Path.cwd()
    with tempfile.TemporaryDirectory() as d:
        try:
            os.chdir(d)
            sh("git", "init", "-qb", "main")
            sh("git", "config", "user.email", "t@t")
            sh("git", "config", "user.name", "t")
            first = commit({"util.py": "def helper():\n    return 1\n"}, "carry base")
            sh("git", "commit", "--allow-empty", "-qm", "second base")
            second = sh("git", "rev-parse", "HEAD")
            head = commit({"util.py": "def helper():\n    return 2\n"}, "carry head")
            older_base, newer_base = sorted((first, second), reverse=True)

            def run_for(base):
                rc.start(f"{base}..{head}")
                run = Path("tmp/review-check") / f"range-{base[:7]}-{head[:7]}-01"
                checklist = load_checklist(run)
                uid, checks = next(iter(checklist.items()))
                rows(run, f"| {uid} | {', '.join(checks['rules']) or '-'} | x | `return 2` | n/a | | "
                     "the test code has no case these rules name |",
                     f"| {uid} | - | util.py:2 | `return 2` | ask | may change a caller | caller expects 1 |")
                assert rc.verdict(run.name) == 0
                return run

            older = run_for(older_base)
            assert "0 fix, 1 ask" in (older / "03-verdict.md").read_text()
            newer = run_for(newer_base)
            newer_answer = "- util.py `return 2`: user approved the value in the newer review\n"
            (newer / "answers.md").write_text(newer_answer)
            assert rc.verdict(newer.name) == 0
            older_answer = "- util.py `return 2`: user approved the value in the older review\n"
            (older / "answers.md").write_text(older_answer)
            assert rc.verdict(older.name) == 0
            os.utime(older / "target.json", (1000, 1000))
            os.utime(newer / "target.json", (2000, 2000))
            os.utime(older, (3000, 3000))
            Path("util.py").write_text("def helper():\n    return 3\n")
            rc.start("local")
            local = Path("tmp/review-check/local-01")
            assert (local / "answers-before.md").read_text() == newer_answer, "carry-forward must use the newer target.json"

            Path("合意.py").write_text("value = '合意'\n", encoding="utf-8")
            sh("git", "config", "core.quotepath", "false")
            write_text = Path.write_text
            encodings = {}

            def ascii_write(path, text, *args, **kwargs):
                encodings[path.name] = kwargs.get("encoding")
                kwargs.setdefault("encoding", "ascii")
                return write_text(path, text, *args, **kwargs)

            with patch.object(Path, "write_text", ascii_write):
                rc.start("local")
            assert encodings["01-units.md"] == encodings["02-review.md"] == "utf-8"
            unicode_run = Path("tmp/review-check/local-02")
            assert "合意.py" in (unicode_run / "01-units.md").read_text(encoding="utf-8")
            assert rc.revision(unicode_run / "02-review.md")

            os.utime(older / "target.json", (2000, 2000))
            glob = Path.glob
            carried = []
            for reverse in (False, True):
                def ordered_glob(path, pattern, *args, **kwargs):
                    found = glob(path, pattern, *args, **kwargs)
                    return iter(sorted(found, reverse=reverse)) if pattern == "*/target.json" else found

                with patch.object(Path, "glob", ordered_glob):
                    rc.start("local")
                tied_run = Path("tmp/review-check") / f"local-{3 + len(carried):02d}"
                carried.append((tied_run / "answers-before.md").read_text(encoding="utf-8"))
            assert carried == [older_answer, older_answer], "equal mtimes must carry from the same folder in either glob order"
        finally:
            os.chdir(previous)



def test_superseded_run():
    """Only the newest run at each ancestor head guards a new range."""
    previous = Path.cwd()
    with tempfile.TemporaryDirectory() as d:
        try:
            os.chdir(d)
            sh("git", "init", "-qb", "main")
            sh("git", "config", "user.email", "t@t")
            sh("git", "config", "user.name", "t")
            base = commit({"util.py": "def helper():\n    return 1\n"}, "superseded base")
            head = commit({"util.py": "def helper():\n    return 2\n"}, "superseded head")
            prefix = f"range-{base[:7]}-{head[:7]}"
            rc.start(f"{base}..{head}")
            older = Path("tmp/review-check") / f"{prefix}-01"
            assert rc.verdict(older.name) == 1
            assert "Verdict: INCOMPLETE" in (older / "03-verdict.md").read_text(encoding="utf-8")
            rc.start(f"{base}..{head}")
            newer = Path("tmp/review-check") / f"{prefix}-02"

            def review(run, ask=False):
                checklist = load_checklist(run)
                extra = ["| u1 | - | util.py:2 | `return 2` | ask | may change a caller | caller expects 1 |"] if ask else []
                rows(run, *(f"| {uid} | {', '.join(c['rules']) or '-'} | x | `return 2` | n/a | | "
                            "the test code has no case these rules name |" for uid, c in checklist.items()), *extra)
                assert rc.verdict(run.name) == 0

            review(newer)
            assert "Verdict: APPROVE" in (newer / "03-verdict.md").read_text(encoding="utf-8")
            os.utime(older / "target.json", (1000, 1000))
            os.utime(newer / "target.json", (2000, 2000))
            changed = commit({"util.py": "def helper():\n    return 3\n"}, "after superseded head")
            rc.start(f"{head}..{changed}")
            next_run = Path("tmp/review-check") / f"range-{head[:7]}-{changed[:7]}-01"
            assert next_run.is_dir(), "a newer APPROVE supersedes the older INCOMPLETE run"

            review(older)
            review(newer, ask=True)
            folders = set(Path("tmp/review-check").iterdir())
            try:
                rc.start(f"{head}..{changed}")
                raise AssertionError("the newer open ask must block despite the older APPROVE")
            except SystemExit as e:
                assert newer.name in str(e), e
            assert set(Path("tmp/review-check").iterdir()) == folders
        finally:
            os.chdir(previous)



def test_run_numbering():
    """Deleting an earlier run leaves the next number above every remaining run."""
    previous = Path.cwd()
    with tempfile.TemporaryDirectory() as d:
        try:
            os.chdir(d)
            sh("git", "init", "-qb", "main")
            sh("git", "config", "user.email", "t@t")
            sh("git", "config", "user.name", "t")
            base = commit({"util.py": "value = 1\n"}, "numbering base")
            head = commit({"util.py": "value = 2\n"}, "numbering head")
            prefix = f"range-{base[:7]}-{head[:7]}"
            for number in range(1, 4):
                rc.start(f"{base}..{head}")
                assert (Path("tmp/review-check") / f"{prefix}-{number:02d}").is_dir()
            shutil.rmtree(Path("tmp/review-check") / f"{prefix}-01")
            rc.start(f"{base}..{head}")
            assert (Path("tmp/review-check") / f"{prefix}-04").is_dir()
        finally:
            os.chdir(previous)

