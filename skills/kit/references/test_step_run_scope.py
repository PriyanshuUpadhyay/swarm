# Step runner write-scope cases (council C3): done refuses a change outside the Writes of the steps beside it.
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
    assert out.returncode == 1 and "it changed src/b/x.ts, outside the Writes of every step that ran beside it (src/a/" in out.stderr, out
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


def test_scope_parallel_order_names_and_a_lost_base(tmp):
    repo, folder = scope_repo(tmp)
    assert run("take", folder, "01-a", "x").returncode == 0 and run("take", folder, "02-b", "y").returncode == 0
    for path in ("src/a/café.ts", "src/b/b.ts"):
        (repo / path).parent.mkdir(exist_ok=True)
        (repo / path).write_text("x\n", encoding="utf-8")
    # 02-b is done first; 01-a still sees its files as a parallel step's, and git does not quote the é.
    assert run("done", folder, "02-b", "1").returncode == 0
    out = run("done", folder, "01-a", "1")
    assert out.returncode == 0, out
    # A base tree that git pruned fails the check closed.
    f = folder / "03-end.md"
    f.write_text(f.read_text(encoding="utf-8").replace("- [ ] the end: ", "- [x] the end: done"), encoding="utf-8")
    assert run("take", folder, "03-end", "z").returncode == 0
    (repo / "README.md").write_text("new\n", encoding="utf-8")
    subprocess.run(["git", "-C", str(repo), "prune", "--expire=now"], check=True,
                   env={k: v for k, v in os.environ.items() if not k.startswith("GIT_")})
    out = run("done", folder, "03-end", "1")
    assert out.returncode == 1 and "scope check failed" in out.stderr, out


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
    with patch.object(step_scope.subprocess, "run", side_effect=FileNotFoundError("git")):
        assert step_scope.work_tree(folder) is None
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


def chart_plan():
    sys.path.insert(0, str(SCRIPT.parents[1] / "blocks" / "plan-chart" / "scripts"))
    import test_plan_chart
    return test_plan_chart.PLAN


def test_amend_within_the_owner_budget(tmp):
    folder = Path(tmp, "run")
    folder.mkdir()
    plan = folder / "plan.md"
    text = chart_plan().replace("| 02-read |", "| `02-read.md` (optional) |")
    plan.write_text(text.replace("\n## Blocks considered", "\nAmend: 1\n\n## Blocks considered", 1), encoding="utf-8")
    assert run("start", folder, plan).returncode == 0
    # skip needs the plan's (optional) mark.
    out = run("skip", folder, "03-tokens", "not needed")
    assert out.returncode == 1 and "03-tokens is not marked (optional)" in out.stderr, out
    assert run("skip", folder, "02-read", "not needed").returncode == 0
    # A writing step goes back to the owner, and so does a step whose plan check fails.
    out = run("amend", folder, "| 06-docs | build | the docs | `docs/` | voice | the docs |")
    assert out.returncode == 1 and "an amended step writes none" in out.stderr, out
    before = plan.read_text(encoding="utf-8")
    out = run("amend", folder, "| 06-docs | nope | the docs | none | voice | the docs |")
    assert out.returncode == 1 and "amend refused" in out.stderr and "nope" in out.stderr and plan.read_text(encoding="utf-8") == before, out
    out = run("amend", folder, "| 06-check | build | a check | none | local-research | look again |", "--before", "05-reply")
    assert out.returncode == 0 and "06-check: amended (1 of 1 spent)" in out.stdout, out
    assert (folder / "06-check.md").read_text(encoding="utf-8").splitlines()[1:3] == ["Uses: 04-build", "Writes: none"]
    assert (folder / "05-reply.md").read_text(encoding="utf-8").splitlines()[1] == "Uses: 04-build, 06-check"
    out = run("amend", folder, "| 07-more | build | more | none | local-research | again |")
    assert out.returncode == 1 and "no Amend budget left (1 of 1 spent)" in out.stderr, out


def test_plan_blocks_follow_the_repo_off_table(tmp):
    # start leaves out a block that the repo's .blocks.toml turns off, so done must not ask for its todo.
    repo = Path(tmp, "repo")
    repo.mkdir()
    subprocess.run(["git", "-C", str(repo), "init", "-q"], check=True,
                   env={k: v for k, v in os.environ.items() if not k.startswith("GIT_")})
    (repo / ".blocks.toml").write_text('[params]\nsource_of_truth = "README.md"\n\n[off]\nplan-review = "no owner round"\n',
                                        encoding="utf-8")
    folder = repo / "run"
    folder.mkdir()
    (folder / "plan.md").write_text("| File | Needs | Holds | Writes | Block | Why |\n|---|---|---|---|---|---|\n"
                                    "| `01-a.md` | none | part a | none | plan-review | it starts the plan |\n", encoding="utf-8")
    out = run("start", folder, folder / "plan.md")
    assert out.returncode == 0, out
    f = folder / "01-a.md"
    assert "block plan-review script" not in f.read_text(encoding="utf-8")
    f.write_text("".join(l.replace("- [ ] ", "- [x] ", 1).rstrip("\n") + " checked\n" if l.startswith("- [ ] ") else l
                         for l in f.read_text(encoding="utf-8").splitlines(keepends=True)), encoding="utf-8")
    assert run("take", folder, "01-a", "x").returncode == 0
    out = run("done", folder, "01-a", "1")
    assert out.returncode == 0, out


def test_amend_refuses_odd_rows(tmp):
    folder = Path(tmp, "run")
    folder.mkdir()
    plan = folder / "plan.md"
    text = chart_plan().replace("| 05-reply |", "| `05-reply.md` (optional) |")
    plan.write_text(text.replace("\n## Blocks considered", "\nAmend: 3\n\n## Blocks considered", 1), encoding="utf-8")
    assert run("start", folder, plan).returncode == 0
    row = "| 06-check | build | a check | none | local-research | look again |"
    out = run("amend", folder, row + "\nAmend: 99")
    assert out.returncode == 1 and "one table row on one line" in out.stderr, out
    out = run("amend", folder, row.replace("06-check", "extra"), "--before", "05-reply")
    assert out.returncode == 1 and "amended step extra is not NN-name" in out.stderr and "Traceback" not in out.stderr, out
    # The --before step's File cell carries (optional); the plan parser finds it.
    out = run("amend", folder, row, "--before", "05-reply")
    assert out.returncode == 0 and "Traceback" not in out.stderr, out
    assert (folder / "05-reply.md").read_text(encoding="utf-8").splitlines()[1] == "Uses: 04-build, 06-check"
