# Step runner write-scope cases (council C3): done refuses a change outside every active step's Writes.
from test_step_run_support import *
import step_scope  # noqa: E402

PLAN = """| File | Needs | Holds | Writes |
|---|---|---|---|
| `01-a.md` | none | part a | `src/a/` |
| `02-b.md` | none | part b | `src/b/` |
| `03-end.md` | a, b | the end | none |
"""


def scope_repo(tmp):
    """A git repo with a plan run in tmp/run (ignored), both writing steps ready, and their todos checked."""
    repo = Path(tmp, "repo")
    (repo / "src").mkdir(parents=True)
    git = lambda *a: subprocess.run(["git", "-C", str(repo), *a], capture_output=True, text=True, check=True,
                                    env={k: v for k, v in os.environ.items() if not k.startswith("GIT_")})
    git("init", "-q")
    (repo / ".git" / "info" / "exclude").write_text("/tmp/\n", encoding="utf-8")
    (repo / "src" / "keep.ts").write_text("1\n", encoding="utf-8")
    git("add", "-A")
    git("-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "one")
    (repo / "plan.md").write_text(PLAN, encoding="utf-8")
    folder = repo / "tmp" / "run"
    assert run("start", folder, repo / "plan.md").returncode == 0
    for step in ("01-a", "02-b"):
        f = folder / f"{step}.md"
        f.write_text(f.read_text(encoding="utf-8").replace("- [ ] part a: ", "- [x] part a: done")
                     .replace("- [ ] part b: ", "- [x] part b: done"), encoding="utf-8")
    return repo, folder


def test_done_refuses_a_write_outside_scope(tmp):
    repo, folder = scope_repo(tmp)
    assert (folder / "01-a.md").read_text(encoding="utf-8").splitlines()[2] == "Writes: src/a/"
    (repo / "src" / "b").mkdir()
    (repo / "src" / "b" / "before.ts").write_text("made before the take\n", encoding="utf-8")
    assert run("take", folder, "01-a", "x").returncode == 0
    assert "\t01-a\tbase " in (folder / "events.log").read_text(encoding="utf-8")
    (repo / "src" / "a").mkdir()
    (repo / "src" / "a" / "y.ts").write_text("in scope\n", encoding="utf-8")
    (repo / "src" / "b" / "x.ts").write_text("out of scope\n", encoding="utf-8")
    out = run("done", folder, "01-a", "1")
    assert out.returncode == 1 and "it changed src/b/x.ts, outside every active step's Writes (src/a/" in out.stderr, out
    assert (folder / "01-a.md").read_text(encoding="utf-8").startswith("Status: active x #1")
    # While 02-b is active, its Writes count too: a parallel step's files cannot be told apart.
    assert run("take", folder, "02-b", "y").returncode == 0
    assert run("done", folder, "01-a", "1").returncode == 0
    (repo / "README.md").write_text("no step writes this\n", encoding="utf-8")
    out = run("done", folder, "02-b", "1")
    assert out.returncode == 1 and "it changed README.md" in out.stderr, out
    (repo / "README.md").unlink()
    (repo / "src" / "keep.ts").write_text("2\n", encoding="utf-8")
    out = run("done", folder, "02-b", "1")
    assert out.returncode == 1 and "it changed src/keep.ts" in out.stderr, out
    (repo / "src" / "keep.ts").write_text("1\n", encoding="utf-8")
    assert run("done", folder, "02-b", "1").returncode == 0


def test_scope_needs_git_and_a_writes_line(tmp):
    folder = Path(tmp, "run")
    Path(tmp, "plan.md").write_text(PLAN, encoding="utf-8")
    assert run("start", folder, Path(tmp, "plan.md")).returncode == 0
    f = folder / "01-a.md"
    f.write_text(f.read_text(encoding="utf-8").replace("- [ ] part a: ", "- [x] part a: done"), encoding="utf-8")
    with patch.object(step_scope, "work_tree", lambda folder: None), redirect_stdout(StringIO()) as out:
        step_run.take(folder, "01-a", "x")
        step_run.done(folder, "01-a", "1")
    assert "not run: scope: no git work tree" in out.getvalue(), out.getvalue()
    assert (folder / "01-a.md").read_text(encoding="utf-8").startswith("Status: done ")
    assert step_scope.writes("Status: open\nUses: \n\n## Todo\n") is None
    assert step_scope.writes("Status: open\nUses: \nWrites: none\n\nx") == []



def test_done_takes_blocks_from_the_plan(tmp):
    # The run folder holds plan.md, as a plan run does. Deleting `Skills read:` and the check todo skips no check.
    folder = Path(tmp, "run")
    folder.mkdir()
    (folder / "plan.md").write_text("| File | Needs | Holds | Writes | Block | Why |\n|---|---|---|---|---|---|\n"
                                    "| `01-a.md` | none | part a | none | plan-review | it starts the plan |\n", encoding="utf-8")
    assert run("start", folder, folder / "plan.md").returncode == 0
    f = folder / "01-a.md"
    text = f.read_text(encoding="utf-8")
    assert "Skills read: " in text and "- [ ] block plan-review script: " in text, text
    lines = [l for l in text.splitlines() if not l.startswith(("Skills read: ", "- [ ] block plan-review script: "))]
    f.write_text("\n".join(l.replace("- [ ] ", "- [x] ", 1) + ("ok" if l.startswith("- [ ] ") else "") for l in lines) + "\n",
                 encoding="utf-8")
    assert run("take", folder, "01-a", "x").returncode == 0
    out = run("done", folder, "01-a", "1")
    assert out.returncode == 1 and "a blocks/plan-review script check has no todo" in out.stderr, out


def test_writer_does_not_take_its_review(tmp):
    folder = Path(tmp, "run")
    folder.mkdir()
    (folder / "plan.md").write_text("| File | Needs | Holds | Writes | Block | Why |\n|---|---|---|---|---|---|\n"
                                    "| `01-a.md` | none | part a | `src/` |  | it writes |\n"
                                    "| `02-rev.md` | a | the verdict | none | change-review | new seats judge it |\n",
                                    encoding="utf-8")
    assert run("start", folder, folder / "plan.md").returncode == 0
    f = folder / "01-a.md"
    f.write_text(f.read_text(encoding="utf-8").replace("- [ ] part a: ", "- [x] part a: done"), encoding="utf-8")
    assert run("take", folder, "01-a", "writer").returncode == 0
    assert run("done", folder, "01-a", "1").returncode == 0
    out = run("take", folder, "02-rev", "writer")
    assert out.returncode == 1 and "02-rev covers 01-a, which writer took; a new seat takes the review" in out.stderr, out
    assert (folder / "02-rev.md").read_text(encoding="utf-8").startswith("Status: open")
    assert run("take", folder, "02-rev", "review-seats").returncode == 0
