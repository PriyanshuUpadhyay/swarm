# Step runner test build cases and helpers.
from step_test_support import *


def test_build_start_and_skip():
    with tempfile.TemporaryDirectory() as tmp:
        folder = Path(tmp) / "piece" / "ink"
        skill = SCRIPT.parents[1] / "skills" / "build" / "SKILL.md"
        out = run("start", folder, skill)
        assert out.returncode == 0, out.stderr
        assert step_run.steps(folder) == ["01-intake", "02-look", "03-story", "04-style-frames",
                                          "05-animatic", "06-detail", "07-export"]
        assert step_run.needs(folder, "04-style-frames") == ["02-look", "03-story"]
        original = (folder / "03-story.md").read_text(encoding="utf-8").split("\n", 1)[1]
        for reason in ("", "\n\t"):
            out = run("skip", folder, "03-story", reason)
            assert out.returncode == 1 and "needs a reason" in out.stderr, out
        events = folder / "events.log"
        before = events.read_text(encoding="utf-8") if events.exists() else None
        for args in ((folder, "03-story", "--force"), ("--force", folder, "03-story", "study"),
                     (folder, "03-story", "study", "--force")):
            out = run("skip", *args)
            assert out.returncode == 1 and "skip accepts no --force" in out.stderr, out
            assert (folder / "03-story.md").read_text(encoding="utf-8") == "Status: open\n" + original
            assert (events.read_text(encoding="utf-8") if events.exists() else None) == before
        out = run("skip", folder, "03-story", "study", "route")
        assert out.returncode == 0, out.stderr
        assert (folder / "03-story.md").read_text(encoding="utf-8") == "Status: skipped study route\n" + original
        assert (folder / "events.log").read_text(encoding="utf-8").splitlines()[-1].endswith("\t03-story\tskip study route")
        assert run("skip", folder, "03-story", "again").returncode == 1
        child = lock_holder(folder, "05-animatic")
        try:
            out = run("skip", folder, "05-animatic", "study")
            assert out.returncode == 1 and "locked by a running skip" in out.stderr, out
        finally:
            child.communicate(timeout=5)
        for step in ("02-look", "05-animatic", "06-detail"):
            assert run("skip", folder, step, "study route").returncode == 0
        assert step_run.ready(folder, "04-style-frames")
        assert step_run.ready(folder, "07-export", ["06-detail"])


def test_build_block_todos():
    with tempfile.TemporaryDirectory() as tmp:
        folder = Path(tmp, "piece with spaces", "ink")
        skill = SCRIPT.parents[1] / "skills/build/SKILL.md"
        out = subprocess.run([sys.executable, "-I", SCRIPT, "start", folder, skill], capture_output=True, text=True)
        assert out.returncode == 0, out.stderr
        file = folder / "04-style-frames.md"
        text = file.read_text(encoding="utf-8")
        for expected in ("block look-check script:", "block headless-shot script:",
                         "block look-check judge:", "two or more rounds", "last round have no `miss`",
                         "block visual-qa judge:"):
            assert expected in text, f"missing build check {expected}"
        assert "{{site}}" not in text and "{{shots}}" not in text
        assert "### Rules" in text and "Skills read:" in text
        assert ("Kit blocks (`blocks/<id>/`) and the kit's `references/` are under "
                f"{step_run.kit_path(SCRIPT.parents[1])} (the kit root).") in text
        head, end, _ = step_run.todo_scan(text)
        lines = text[head:end].splitlines()
        # The old single handoff todo cannot close a step whose block checks are still open.
        lines[1] = lines[1].replace("- [ ]", "- [x]") + " handoff evidence"
        file.write_text(text[:head] + "\n".join(lines) + "\n" + text[end:], encoding="utf-8")
        out = run("done", folder, "04-style-frames")
        assert out.returncode == 1 and "look-check" in out.stderr, out
        text = file.read_text(encoding="utf-8")
        file.write_text("\n".join(line.replace("- [ ]", "- [x]") + " evidence" if line.startswith("- [ ]")
                                  else line for line in text.splitlines()) + "\n", encoding="utf-8")
        out = run("done", folder, "04-style-frames")
        assert out.returncode == 0, out.stderr
        before = file.read_bytes()
        assert run("start", folder, skill).returncode == 0
        assert file.read_bytes() == before, "repeat start must preserve completed steps"
        export = (folder / "07-export.md").read_text(encoding="utf-8")
        assert "block headless-shot script:" in export and "block visual-qa judge:" in export


def test_table_blocks():
    with tempfile.TemporaryDirectory() as tmp:
        skill = Path(tmp, "SKILL.md")
        for extra, cell, expected in (("", "", None), (" Block | Why |", " story | reason |", None),
                                      (" Blocks |", " story, look-check |", ["story", "look-check"]),
                                      (" Blocks |", " |", [])):
            skill.write_text(f"| File | Needs | Holds |{extra}\n|---|---|---|\n"
                             f"| `01-ask.md` | none | the question |{cell}\n", encoding="utf-8")
            assert step_run.table(skill) == [("01-ask", "none", "the question", expected)]


def test_build_start_without_tomllib():
    with tempfile.TemporaryDirectory() as tmp:
        folder = Path(tmp, "piece/ink")
        skill = SCRIPT.parents[1] / "skills/build/SKILL.md"
        code = "import runpy, sys; sys.modules['tomllib'] = None; sys.argv = sys.argv[1:]; runpy.run_path(sys.argv[0], run_name='__main__')"
        args = [sys.executable, "-c", code, str(SCRIPT)]
        out = subprocess.run([*args, "start", str(folder), str(skill)], capture_output=True, text=True)
        catalog = SCRIPT.parents[1] / "blocks/blocks.toml"
        assert out.returncode == 1 and out.stderr == (
            f"{catalog}: reading TOML needs Python 3.11 or later (tomllib); "
            "use a newer python3, for example from Homebrew\n"), out
        assert not folder.exists()
        folder.mkdir(parents=True)
        (folder / "01-intake.md").write_text("Status: open\nUses: \n", encoding="utf-8")
        out = subprocess.run([*args, "status", str(folder)], capture_output=True, text=True)
        assert out.returncode == 0 and "01-intake: open" in out.stdout, out


def test_build_piece_repo_paths():
    with tempfile.TemporaryDirectory() as tmp:
        outside = Path(tmp)
        repo, decoy = outside / "repo", outside / "decoy"
        env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
        for root in (repo, decoy):
            subprocess.run(["git", "init", "-q", str(root)], env=env, check=True)
        piece = repo / "art-b"
        skill = SCRIPT.parents[1] / "skills/build/SKILL.md"
        texts = []
        hook = {"GIT_DIR": str(decoy / ".git"), "GIT_WORK_TREE": str(decoy),
                "GIT_INDEX_FILE": str(decoy / ".git/index")}
        for index, (cwd, extra) in enumerate(((repo, {}), (outside, {}), (outside, hook))):
            folder = piece / f"ink-{index}"
            out = subprocess.run([sys.executable, SCRIPT, "start", folder, skill], cwd=cwd,
                                 env={**env, **extra}, capture_output=True, text=True)
            assert out.returncode == 0, out.stderr
            text = (folder / "04-style-frames.md").read_text(encoding="utf-8")
            assert "file art-b/shots/*.png" in text, text
            texts.append(text)
        assert len(set(texts)) == 1, "piece bindings must not depend on cwd or caller Git variables"
        assert not (decoy / "art-b").exists()


def test_build_repo_additions():
    with tempfile.TemporaryDirectory() as tmp:
        repo = Path(tmp)
        env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
        subprocess.run(["git", "init", "-q", str(repo)], env=env, check=True)
        (repo / "docs").mkdir()
        (repo / "docs/rule.md").write_text("## Rules\n- IMPLEMENT ONLY\n", encoding="utf-8")
        (repo / "docs/visual.md").write_text("## Rules\n- BUILD ONLY\n## Check (script)\n- none: text only\n## Check (judge)\n- Is it visual?\n", encoding="utf-8")
        (repo / ".blocks.toml").write_text(
            '[params]\nsource_of_truth = "docs/rule.md"\n[add]\nbuild = ["docs/rule.md"]\n04-style-frames = ["docs/visual.md"]\n', encoding="utf-8")
        folder = repo / "art-b/ink"
        skill = SCRIPT.parents[1] / "skills/build/SKILL.md"
        out = subprocess.run([sys.executable, SCRIPT, "start", folder, skill],
                             cwd=repo, env=env, capture_output=True, text=True)
        assert out.returncode == 0, out.stderr
        text = (folder / "04-style-frames.md").read_text(encoding="utf-8")
        assert "BUILD ONLY" in text and "IMPLEMENT ONLY" not in text, text
        assert "[add] build in" in out.stderr and "names no step of this skill; skipped" in out.stderr, out.stderr


def test_block_command():
    for script in (SCRIPT, SCRIPT.parents[1] / "skills/implement/scripts/implement_run.py"):
        with tempfile.TemporaryDirectory() as tmp:
            folder = Path(tmp)
            step = "01-ask" if script == SCRIPT else "01-frame"
            file = folder / f"{step}.md"
            body = "Uses: upstream@123\n\n## Result\nkept\n"
            original = "Status: active builder\n" + body
            file.write_text(original, encoding="utf-8")
            call = lambda *args: subprocess.run([sys.executable, script, "block", *map(str, args)],
                                                capture_output=True, text=True)
            for args in ((), (folder,), (folder, step), (folder, step, " "),
                         (folder, step, "--force", "no seats"), ("--force", folder, step, "no seats")):
                out = call(*args)
                assert out.returncode == 1 and "Traceback" not in out.stderr, out
                assert file.read_text(encoding="utf-8") == original
                assert not (folder / "events.log").exists()
            for missing in ("06-reviw", "06-review"):
                out = call(folder, missing, "needs review seats")
                assert out.returncode == 1 and "Traceback" not in out.stderr, out
                assert missing in out.stderr
                assert not (folder / f"{missing}.lock").exists()
                assert not (folder / f"{missing}.md").exists()
                assert not (folder / "events.log").exists()
                assert file.read_text(encoding="utf-8") == original
            with step_run.step_lock(folder, step):
                out = call(folder, step, "missing seat")
                assert out.returncode == 1 and "locked by a running block" in out.stderr, out
            out = call(folder, step, "\x1bneeds\nreview\tseats\x7f")
            assert out.returncode == 0, out.stderr
            assert file.read_text(encoding="utf-8") == "Status: blocked needs review seats\n" + body
            events = (folder / "events.log").read_text(encoding="utf-8")
            assert len(events.splitlines()) == 1 and events.endswith(f"\t{step}\tblock needs review seats\n"), events
            for state in ("done 123", "skipped route"):
                file.write_text(f"Status: {state}\n" + body, encoding="utf-8")
                out = call(folder, step, "no seats")
                assert out.returncode == 1 and "only an open or active step" in out.stderr, out
                assert file.read_text(encoding="utf-8") == f"Status: {state}\n" + body
                assert (folder / "events.log").read_text(encoding="utf-8") == events
            file.write_text("Status: open\n" + body, encoding="utf-8")
            assert call(folder, step, "missing input").returncode == 0
            assert file.read_text(encoding="utf-8") == "Status: blocked missing input\n" + body


def test_start_refuses_bad_needs():
    head = "| File | Needs | Holds |\n|---|---|---|\n"
    cases = {  # table rows -> text that the refusal must hold
        "unknown": ("| `01-ask.md` | none | q |\n| `02-look.md` | nothing | x |\n", ["02-look", "'nothing'"]),
        "ambiguous": ("| `01-ask.md` | none | q |\n| `02-look-local.md` | ask | x |\n"
                      "| `03-look-web.md` | ask | y |\n| `04-report.md` | look | z |\n", ["04-report", "'look'"]),
        "cycle": ("| `01-ask.md` | report | q |\n| `02-look.md` | ask | x |\n| `03-report.md` | look | y |\n",
                  ["cycle", "01-ask needs 'report'"]),
    }
    for case, (rows, words) in cases.items():
        with tempfile.TemporaryDirectory() as tmp:
            skill = Path(tmp) / "SKILL.md"
            skill.write_text(head + rows, encoding="utf-8")
            out = run("start", Path(tmp) / "run", skill)
            assert out.returncode == 1 and "Traceback" not in out.stderr, (case, out)
            assert all(w in out.stderr for w in words), (case, out.stderr)
            assert not (Path(tmp) / "run").exists() or not step_run.steps(Path(tmp) / "run"), case
    with tempfile.TemporaryDirectory() as tmp:  # an exact word names its step even when another step starts with it
        skill = Path(tmp) / "SKILL.md"
        skill.write_text(head + "| `01-review.md` | none | a |\n| `02-review-fix.md` | review | b |\n"
                         "| `03-reply.md` | review | c |\n", encoding="utf-8")
        out = run("start", Path(tmp) / "run", skill)
        assert out.returncode == 0, out.stderr
        assert (Path(tmp) / "run" / "03-reply.md").read_text(encoding="utf-8").splitlines()[1] == "Uses: 01-review"
    for skill in ("council", "build", "research", "web-search", "implement"):
        with tempfile.TemporaryDirectory() as tmp:
            out = run("start", Path(tmp) / "run", SCRIPT.parents[1] / "skills" / skill / "SKILL.md")
            assert out.returncode == 0, (skill, out.stderr)
