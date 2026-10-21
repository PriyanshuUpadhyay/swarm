# Step runner cases bound to plan.md (council C4 and C6): blocks from the plan, the review seat, and amend.
from test_step_run_support import *


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
    # A CRLF plan keeps its Amend budget.
    plan.write_bytes(plan.read_bytes().replace(b"\n", b"\r\n"))
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
    (repo / ".blocks.toml").write_text('[params]\nsource_of_truth = "README.md"\n\n[off]\nplan-review = "no owner round"\n'
                                        '\n[add]\n02-b = []\n',
                                        encoding="utf-8")
    folder = repo / "run"
    folder.mkdir()
    (folder / "plan.md").write_text("| File | Needs | Holds | Writes | Block | Why |\n|---|---|---|---|---|---|\n"
                                    "| `01-a.md` | none | part a | none | plan-review | it starts the plan |\n"
                                    "| `02-b.md` | a | part b | none |  | it ends |\n", encoding="utf-8")
    out = run("start", folder, folder / "plan.md")
    assert out.returncode == 0, out
    f = folder / "01-a.md"
    assert "block plan-review script" not in f.read_text(encoding="utf-8")
    f.write_text("".join(l.replace("- [ ] ", "- [x] ", 1).rstrip("\n") + " checked\n" if l.startswith("- [ ] ") else l
                         for l in f.read_text(encoding="utf-8").splitlines(keepends=True)), encoding="utf-8")
    assert run("take", folder, "01-a", "x").returncode == 0
    out = run("done", folder, "01-a", "1")
    assert out.returncode == 0 and "names no step" not in out.stderr, out


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
    # A held --before step stops the amend before it writes anything.
    holder = lock_holder(folder, "05-reply")
    try:
        before = plan.read_text(encoding="utf-8")
        out = run("amend", folder, row, "--before", "05-reply")
        assert out.returncode == 1 and "05-reply is locked by a running amend" in out.stderr, out
        assert plan.read_text(encoding="utf-8") == before and not (folder / "06-check.md").exists()
    finally:
        holder.stdin.close()
        holder.wait(timeout=5)
    out = run("amend", folder, row, "--before", "05-reply")
    assert out.returncode == 0 and "Traceback" not in out.stderr, out
    assert (folder / "05-reply.md").read_text(encoding="utf-8").splitlines()[1] == "Uses: 04-build, 06-check"
    # A backticked Needs cell takes the new step word, and a refused amend leaves plan.html as it was.
    ticked = plan.read_text(encoding="utf-8").replace("| `05-reply.md` (optional) | build, check |",
                                                      "| `05-reply.md` (optional) | `build, check` |")
    assert ticked != plan.read_text(encoding="utf-8")
    plan.write_text(ticked, encoding="utf-8")
    # A refused amend puts plan.md back byte for byte, CRLF line ends too.
    crlf = plan.read_bytes().replace(b"\n", b"\r\n")
    plan.write_bytes(crlf)
    out = run("amend", folder, "| 08-bad | nope | x | none | local-research | y |")
    assert out.returncode == 1 and "nope" in out.stderr and plan.read_bytes() == crlf, out
    plan.write_text(ticked, encoding="utf-8")
    # A start that fails after the chart passed leaves plan.html as it was.
    page = plan.with_suffix(".html")
    page.write_text("before\n", encoding="utf-8")
    import step_amend
    with patch.object(step_run, "start", side_effect=SystemExit("boom")):
        try:
            step_amend.amend(folder, "| 08-ok | build | x | none | local-research | y |", "--before", "05-reply")
        except SystemExit as stop:
            assert str(stop) == "boom", stop
    assert page.read_text(encoding="utf-8") == "before\n" and plan.read_text(encoding="utf-8") == ticked
    assert not (folder / "08-ok.md").exists()
    # A failed Uses rewrite of the --before step puts its file back too, and spends no budget.
    reply = (folder / "05-reply.md").read_bytes()
    real = step_run.write_step
    full = lambda file, text: (_ for _ in ()).throw(OSError("disk full")) if file.name == "05-reply.md" else real(file, text)
    with patch.object(step_run, "write_step", full):
        try:
            step_amend.amend(folder, "| 08-ok | build | x | none | local-research | y |", "--before", "05-reply")
        except OSError:
            pass
    assert (folder / "05-reply.md").read_bytes() == reply and plan.read_text(encoding="utf-8") == ticked
    assert "amend 08-ok" not in (folder / "events.log").read_text(encoding="utf-8")
    # A --before step file with no plan row is refused, not a traceback.
    (folder / "99-stray.md").write_text("Status: open\nUses: \n", encoding="utf-8")
    out = run("amend", folder, "| 08-ok | build | x | none | local-research | y |", "--before", "99-stray")
    assert out.returncode == 1 and "99-stray is not a step of plan.md" in out.stderr, out
    (folder / "99-stray.md").unlink()
    # A part block brings its parent into the blocks table, as an approved plan lists it.
    out = run("amend", folder, "| 07-draw | build | a diagram | none | html-diagram | show the flow |", "--before", "05-reply")
    assert out.returncode == 0, out
    assert (folder / "05-reply.md").read_text(encoding="utf-8").splitlines()[1] == "Uses: 04-build, 06-check, 07-draw"
