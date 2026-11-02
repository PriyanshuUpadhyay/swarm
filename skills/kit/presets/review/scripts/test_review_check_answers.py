# Whole-line answers and review carry-forward checks.
from test_review_check_support import *


def test_answers():
    """Only a whole new line at the ask's location lets an answer close it."""
    previous = Path.cwd()
    with tempfile.TemporaryDirectory() as d:
        try:
            os.chdir(d)
            sh("git", "init", "-qb", "main")
            sh("git", "config", "user.email", "t@t")
            sh("git", "config", "user.name", "t")
            base = commit({"util.py": "def helper():\n    return 1\n"}, "answer base")
            source_with_separator = 'x = f("`: ", 1)'
            head = commit({"util.py": f"def helper():\n    return 2\n    {source_with_separator}\n"}, "answer change")
            rc.start(f"{base}..{head}")
            run = Path("tmp/review-check") / f"range-{base[:7]}-{head[:7]}-01"
            assert json.loads((run / "target.json").read_text())["patch"] is False

            def ask(run, quote, line=2):
                checklist = load_checklist(run)
                uid, checks = next(iter(checklist.items()))
                rows(run, f"| {uid} | {', '.join(checks['rules']) or '-'} | x | `{quote}` | n/a | | "
                     "the test code has no case these rules name |",
                     f"| {uid} | - | util.py:{line} | `{quote}` | ask | may change a caller | caller expects 1 |")

            ask(run, "return 2")
            answer = "- util.py `return 2`: user approved the return value in the contract\n"
            (run / "answers.md").write_text(answer)
            assert rc.verdict(run.name) == 0
            verdict = (run / "03-verdict.md").read_text()
            assert verdict.splitlines()[2].startswith("Verdict: APPROVE"), verdict
            assert "0 fix, 0 ask, 0 note, 1 answered" in verdict, verdict
            assert "Answered: util.py `return 2` closes 1 asks" in verdict, verdict
            assert "- answered `util.py:2` may change a caller (answer: user approved" in verdict, verdict
            assert f"answers@{rc.revision(run / 'answers.md')}" in verdict.splitlines()[1], verdict
            approved_uses = verdict.splitlines()[1]

            review = run / "02-review.md"
            escaped_problem = "\x1b[31mmay change a caller"
            review.write_text(review.read_text().replace("may change a caller", escaped_problem))
            output = StringIO()
            with redirect_stdout(output):
                assert rc.verdict(run.name) == 0
            assert "\x1b" not in output.getvalue(), "terminal output must remove the ESC byte from a row"
            assert "[31mmay change a caller" in output.getvalue()
            assert escaped_problem in (run / "03-verdict.md").read_text(), "the verdict file must keep the exact row text"
            ask(run, "return 2")
            control_problem = "\x7fmay\x85 change a caller"
            review.write_text(review.read_text().replace("may change a caller", control_problem))
            output = StringIO()
            with redirect_stdout(output):
                assert rc.verdict(run.name) == 0, "C1 controls inside a row must not split the row"
            assert "\x7f" not in output.getvalue() and "\x85" not in output.getvalue(), "terminal output must remove DEL and C1 controls"
            assert "may change a caller" in output.getvalue()
            assert control_problem in (run / "03-verdict.md").read_text(), "the verdict file must keep DEL and C1 controls"
            ask(run, "return 2")

            (run / "answers.md").write_text("- util.py `return 2`: user approved `helper`: returns 2\n")
            assert rc.verdict(run.name) == 0
            verdict = (run / "03-verdict.md").read_text()
            assert verdict.splitlines()[2].startswith("Verdict: APPROVE"), "answer code followed by a colon must close the ask"
            assert "Answered: util.py `return 2` closes 1 asks" in verdict, verdict

            ask(run, source_with_separator, 3)
            source_answer = f"- util.py `{source_with_separator}`: user approved the separator in the contract\n"
            (run / "answers.md").write_text(source_answer)
            assert rc.verdict(run.name) == 0
            verdict = (run / "03-verdict.md").read_text()
            assert "0 fix, 0 ask, 0 note, 1 answered" in verdict, "a source separator must not leave the ask open"
            assert verdict.count("Answered:") == 1, "alternatives belong to one answer line"
            assert f"Answered: util.py `{source_with_separator}` closes 1 asks" in verdict, verdict
            (run / "answers.md").unlink()
            (run / "answers-before.md").write_text(source_answer)
            assert rc.verdict(run.name) == 0
            verdict = (run / "03-verdict.md").read_text()
            assert verdict.count("History:") == 1 and f"History: util.py `{source_with_separator}`: user approved" in verdict, verdict
            (run / "answers-before.md").unlink()
            ask(run, "return 2")
            (run / "answers.md").write_text("- util.py `return 2`: user approved `helper`: ok\n")
            assert rc.verdict(run.name) == 0, "only the matched answer text gets the GENERIC check"
            verdict = (run / "03-verdict.md").read_text()
            assert "answer: user approved `helper`: ok" in verdict, verdict
            (run / "answers.md").write_text("- other.py `return 2`: user approved `helper`: ok\n")
            assert rc.verdict(run.name) == 1, "an unmatched line checks its shortest answer text"
            verdict = (run / "03-verdict.md").read_text()
            assert "Answered: other.py `return 2` closes 0 asks" in verdict, "unmatched lines display the first quote"

            unicode_answer = "- util.py `return 2`: user approved the value — 合意\n"
            for separator in ("\x85", "\u2028", "\u2029", "\x0b", "\x0c", "\x1c", "\x1d", "\x1e"):
                text = f"user approved{separator}the value"
                (run / "answers.md").write_text(f"- util.py `return 2`: {text}\n", encoding="utf-8")
                assert rc.answers(run / "answers.md") == [[("util.py", "return 2", text)]], repr(separator)
            (run / "answers.md").write_text(unicode_answer, encoding="utf-8")
            read_text = Path.read_text

            def ascii_default(path, *args, **kwargs):
                kwargs.setdefault("encoding", "ascii")
                return read_text(path, *args, **kwargs)

            write_text = Path.write_text
            written_encodings = {}

            def ascii_write(path, text, *args, **kwargs):
                written_encodings[path.name] = kwargs.get("encoding")
                kwargs.setdefault("encoding", "ascii")
                return write_text(path, text, *args, **kwargs)

            with patch.object(Path, "read_text", ascii_default):
                assert rc.answers(run / "answers.md") == [[("util.py", "return 2", "user approved the value — 合意")]]
                assert rc.revision(run / "answers.md") == rc.hashlib.sha1(unicode_answer.encode("utf-8")).hexdigest()[:12]
            originals = {}
            for filename in ("01-units.md", "diff.patch", "02-review.md", "units.json", "checklist.json", "target.json"):
                file = run / filename
                originals[filename] = file.read_text(encoding="utf-8")
                text = originals[filename]
                if filename.endswith(".json"):
                    value = json.loads(text)
                    if filename == "units.json":
                        value[0]["symbol"] += " — 合意"
                    elif filename == "checklist.json":
                        next(iter(value.values()))["note"] = "合意"
                    else:
                        value["note"] = "合意"
                    text = json.dumps(value, ensure_ascii=False)
                else:
                    text += "# 合意\n"
                file.write_text(text, encoding="utf-8")
            original_source = Path("util.py").read_text()
            Path("util.py").write_text("def helper():\n    return 3\n")
            with patch.object(Path, "read_text", ascii_default), patch.object(Path, "write_text", ascii_write):
                assert rc.verdict(run.name) == 0
                assert "合意" in (run / "03-verdict.md").read_text(encoding="utf-8")
                rc.start("local")
            assert written_encodings["03-verdict.md"] == written_encodings["target.json"] == "utf-8"
            Path("util.py").write_text(original_source)
            for filename, text in originals.items():
                (run / filename).write_text(text, encoding="utf-8")

            for text in ("ok", "fix: ok", "Fix: ok", "FIX: ok", "fix:ok", "fix:", "FIX:   ", " Fix: ok. "):
                (run / "answers.md").write_text(f"- util.py `return 2`: {text}\n")
                assert rc.verdict(run.name) == 1, "a bare ok is not an answer, even with fix:"
                verdict = (run / "03-verdict.md").read_text()
                assert "an answer needs" in verdict and "INCOMPLETE" in verdict, verdict
                assert approved_uses != verdict.splitlines()[1], "an edit to the first answer line changes Uses"

            for text in ("- util.py `return`: user approved this value\n",
                         "- other.py `return 2`: user approved this value\n"):
                (run / "answers.md").write_text(text)
                assert rc.verdict(run.name) == 0
                verdict = (run / "03-verdict.md").read_text()
                assert "NEEDS DISCUSSION" in verdict and "closes 0 asks" in verdict, verdict
            ask(run, "return 1")
            (run / "answers.md").write_text("- util.py `return 1`: user approved the old value\n")
            assert rc.verdict(run.name) == 0
            verdict = (run / "03-verdict.md").read_text()
            assert "NEEDS DISCUSSION" in verdict and "closes 0 asks" in verdict, "removed lines cannot close asks"
            ask(run, "return 2")
            with (run / "02-review.md").open("a") as f:
                f.write("| u1 | - | util.py:2 | `return 2` | ask | another caller | second caller expects 1 |\n")
            (run / "answers.md").write_text(answer)
            assert rc.verdict(run.name) == 0
            verdict = (run / "03-verdict.md").read_text()
            assert "2 answered" in verdict and "closes 2 asks" in verdict, verdict
            ask(run, "return 2")
            (run / "answers.md").write_text("- util.py `return 2`: fix: test_helper fails at " + head[:7] + "\n")
            assert rc.verdict(run.name) == 0
            verdict = (run / "03-verdict.md").read_text()
            assert verdict.splitlines()[2].startswith("Verdict: REQUEST CHANGES"), verdict
            assert f"- fix `util.py:2` fix: test_helper fails at {head[:7]} (proof: caller expects 1)" in verdict, verdict
            for text in (f"Fix:test_helper fails at {head[:7]}", f"FIX: test_helper fails at {head[:7]}"):
                (run / "answers.md").write_text(answer + f"- util.py `return 2`: {text}\n")
                assert rc.verdict(run.name) == 0
                verdict = (run / "03-verdict.md").read_text()
                assert verdict.splitlines()[2].startswith("Verdict: REQUEST CHANGES"), verdict
                assert f"- fix `util.py:2` {text} (proof: caller expects 1)" in verdict, verdict

            (run / "answers.md").write_text(answer)
            assert rc.verdict(run.name) == 0
            rc.start(f"{base}..{head}")
            rerun = Path("tmp/review-check") / f"range-{base[:7]}-{head[:7]}-02"
            assert (rerun / "answers-before.md").read_text() == answer
            ask(rerun, "return 2")
            assert rc.verdict(rerun.name) == 0
            verdict = (rerun / "03-verdict.md").read_text()
            assert verdict.splitlines()[2].startswith("Verdict: NEEDS DISCUSSION"), verdict
            assert "History: util.py `return 2`: user approved" in verdict, verdict
            changed = commit({"util.py": "def helper():\n    return 3\n"}, "answer line changed")
            target_file = rerun / "target.json"
            metadata = json.loads(target_file.read_text())
            metadata["target"] += " — 合意"
            target_file.write_text(json.dumps(metadata, ensure_ascii=False), encoding="utf-8")
            (rerun / "03-verdict.md").write_text((rerun / "03-verdict.md").read_text() + "— 合意\n", encoding="utf-8")
            with patch.object(Path, "read_text", ascii_default):
                try:
                    rc.start(f"{head}..{changed}")
                    raise AssertionError("UTF-8 predecessor files must still block an open ask")
                except SystemExit as e:
                    assert rerun.name in str(e), e
            Path("util.py").write_text("def helper():\n    return 4\n")
            folders = set(Path("tmp/review-check").iterdir())
            result = rerun / "03-verdict.md"
            open_verdict = result.read_text()
            for prior_verdict in (None, "Status: blocked gate failed\nUses:\nVerdict: INCOMPLETE\n", open_verdict):
                if prior_verdict is None:
                    result.unlink()
                else:
                    result.write_text(prior_verdict)
                for target in (f"{head}..{changed}", "local"):
                    try:
                        rc.start(target)
                        raise AssertionError("a new range above an unfinished review must be refused")
                    except SystemExit as e:
                        assert rerun.name in str(e) and "answers.md" in str(e) and f"verdict {rerun.name}" in str(e), e
                    assert set(Path("tmp/review-check").iterdir()) == folders, "a refused start creates no folder"
            patch_file = Path("tmp/part.patch")
            patch_file.write_text(sh("git", "diff", *rc.PLAIN, head, changed) + "\n")
            rc.start(f"{head}..{changed}", "--patch", str(patch_file))
            partial = Path("tmp/review-check") / f"range-{head[:7]}-{changed[:7]}-01"
            assert json.loads((partial / "target.json").read_text())["patch"] is True
            assert not (partial / "answers-before.md").exists(), "--patch bypasses guard and carry-forward"
            ask(partial, "return 3")
            assert rc.verdict(partial.name) == 0
            newer = partial.stat().st_mtime + 10
            os.utime(partial, (newer, newer))
            latest_answer = "- util.py `return 2`: user approved the value in an earlier answer\n"
            (rerun / "answers.md").write_text(latest_answer)
            assert rc.verdict(rerun.name) == 0
            rc.start(f"{head}..{changed}")
            new_run = Path("tmp/review-check") / f"range-{head[:7]}-{changed[:7]}-02"
            assert (new_run / "answers-before.md").read_text() == latest_answer
            ask(new_run, "return 3")
            (new_run / "answers.md").write_text(answer)
            assert rc.verdict(new_run.name) == 0
            verdict = (new_run / "03-verdict.md").read_text()
            assert verdict.splitlines()[2].startswith("Verdict: NEEDS DISCUSSION"), verdict
            assert "Answered: util.py `return 2` closes 0 asks" in verdict, verdict
            assert "History:" not in verdict, verdict
            (rerun / "answers.md").unlink()
            (rerun / "answers.md").write_text(latest_answer)
            newer = new_run.stat().st_mtime + 10
            os.utime(rerun, (newer, newer))
            assert rerun.stat().st_mtime > new_run.stat().st_mtime
            later = commit({"util.py": "def helper():\n    return 4\n"}, "after open patch ask")
            folders = set(Path("tmp/review-check").iterdir())
            try:
                rc.start(f"{changed}..{later}")
                raise AssertionError("the descendant run with an open ask must block a new range")
            except SystemExit as e:
                assert new_run.name in str(e), "the guard must choose the descendant run despite the older run's mtime"
            assert set(Path("tmp/review-check").iterdir()) == folders
            (new_run / "answers.md").write_text("- util.py `return 3`: user approved the value in the contract\n")
            assert rc.verdict(new_run.name) == 0
            rc.start(f"{changed}..{later}")
            after_patch = Path("tmp/review-check") / f"range-{changed[:7]}-{later[:7]}-01"
            assert (after_patch / "answers-before.md").read_text() == (new_run / "answers.md").read_text()
            ask(after_patch, "return 4")
            assert rc.verdict(after_patch.name) == 0
            rc.start(f"{changed}..{later}")
            same_head = Path("tmp/review-check") / f"range-{changed[:7]}-{later[:7]}-02"
            ask(same_head, "return 4")
            assert rc.verdict(same_head.name) == 0
            assert "0 fix, 1 ask" in (same_head / "03-verdict.md").read_text()
            Path("util.py").write_text("def helper():\n    return 5\n")
            folders = set(Path("tmp/review-check").iterdir())
            try:
                rc.start("local")
                raise AssertionError("local must refuse an open ask at the same head")
            except SystemExit as e:
                assert same_head.name in str(e) and "answers.md" in str(e), e
            assert set(Path("tmp/review-check").iterdir()) == folders
            same_head_answer = "- util.py `return 4`: user approved the value in the contract\n"
            (same_head / "answers.md").write_text(same_head_answer)
            assert rc.verdict(same_head.name) == 0
            assert "Verdict: APPROVE" in (same_head / "03-verdict.md").read_text()
            assert "0 fix, 1 ask" in (after_patch / "03-verdict.md").read_text(), "the older run remains superseded"
            rc.start("local")
            local = Path("tmp/review-check/local-02")
            assert (local / "answers-before.md").read_text() == same_head_answer
        finally:
            os.chdir(previous)

