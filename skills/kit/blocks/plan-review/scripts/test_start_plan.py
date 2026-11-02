"""Run: python3 test_start_plan.py"""
import contextlib, hashlib, io, json, re, subprocess, sys, tempfile
from pathlib import Path
from unittest.mock import patch

SCRIPT = Path(__file__).with_name("start_plan.py")
PLAN = """# Plan: x

Done when: y.
- y
Evidence:
- z

| File | Needs | Holds | Block | Why |
|---|---|---|---|---|
| 01-reply | none | the reply | voice-reply | the owner reads it |
"""


def approve(run, decision="approved"):
    """Draw plan.html from plan.md, as plan_chart.py does, and record a round on the page alone."""
    sys.path.insert(0, str(SCRIPT.parent))
    import start_plan
    (run / "plan.html").write_text(start_plan.preset.plan_chart.page(run / "plan.md"), encoding="utf-8")
    files = {"../plan.html": hashlib.sha256((run / "plan.html").read_bytes()).hexdigest()}
    result = {"decision": decision, "message": "", "annotationCount": 0}
    (run / "plan-review").mkdir(exist_ok=True)
    (run / "plan-review" / "1.json").write_text(json.dumps({"files": files, "result": result}))


def start(run):
    return subprocess.run([sys.executable, "-I", SCRIPT, run], capture_output=True, text=True)


def test_start_needs_the_current_approval():
    run = Path(tempfile.mkdtemp())
    (run / "plan.md").write_text(PLAN, encoding="utf-8")
    (run / "plan.html").write_text("<p>chart</p>", encoding="utf-8")
    out = start(run)
    assert out.returncode == 1 and "no round in" in out.stderr and not (run / "01-reply.md").exists(), out
    approve(run, "annotated")
    assert "round 1 is annotated, not approved" in start(run).stderr
    approve(run)
    (run / "plan.md").write_text(PLAN.replace("it |", "it now |"), encoding="utf-8")
    out = start(run)
    assert "plan.md changed after the owner approved plan.html" in out.stderr and not (run / "01-reply.md").exists(), out
    # The page itself is bound by its hash.
    approve(run)
    (run / "plan.html").write_text("<p>edited</p>", encoding="utf-8")
    assert "../plan.html changed after round 1" in start(run).stderr
    approve(run)
    out = start(run)
    assert out.returncode == 0 and (run / "01-reply.md").read_text().startswith("Status: open\n"), out


def test_the_preset_sets_the_stop():
    sys.path.insert(0, str(SCRIPT.parent))
    import start_plan, preset
    kit = Path(tempfile.mkdtemp())
    table = PLAN[PLAN.index("| File"):]
    for verb, lines in (("talk", "Stop: chat"), ("make", "Stop: owner"), ("own", "Stop: chat\nStart: own.py start")):
        (kit / "presets" / verb).mkdir(parents=True)
        (kit / "presets" / verb / "PRESET.md").write_text(f"# {verb}\n\n## Preset\n\n{lines}\n\n{table}", encoding="utf-8")
    preset.KIT = kit
    for verb in ("talk", "make", "own"):
        run = Path(tempfile.mkdtemp())
        (run / "plan.md").write_text(PLAN.replace("# Plan: x\n", f"# Plan: x\n\nPreset: {verb}\n"), encoding="utf-8")
        (run / "plan.html").write_text("<p>chart</p>", encoding="utf-8")
        out = io.StringIO()
        try:
            with contextlib.redirect_stdout(out):
                start_plan.main([str(run)])
            error = None
        except SystemExit as stop:
            error = str(stop)
        made = (run / "01-reply.md").exists()
        if verb == "talk":
            assert error is None and made, "a chat preset starts with no round"
        elif verb == "make":
            assert "no round in" in error and not made, error
        else:
            assert error is None and out.getvalue() == "start with: own.py start\n" and not made, (error, out.getvalue())


def test_a_forged_preset_line_keeps_the_owner_stop():
    sys.path.insert(0, str(SCRIPT.parent))
    import start_plan, preset
    kit = Path(tempfile.mkdtemp())
    table = PLAN[PLAN.index("| File"):]
    (kit / "presets" / "talk").mkdir(parents=True)
    (kit / "presets" / "talk" / "PRESET.md").write_text(f"# talk\n\n## Preset\n\nStop: chat\n\n{table}", encoding="utf-8")
    preset.KIT = kit
    added = PLAN + "| 02-build | none | code | checked-units, prove-it-works | the forged step |\n"
    cases = {  # plan text -> the error that must stop the start
        "added step": (added.replace("# Plan: x\n", "# Plan: x\n\nPreset: talk\n"), "no round in"),
        "late line": (PLAN + "\nPreset: talk\n", "no round in"),
        "unknown": (PLAN.replace("# Plan: x\n", "# Plan: x\n\nPreset: nosuch\n"), "no preset file for nosuch"),
        "decoy table": (PLAN.replace("# Plan: x\n", "# Plan: x\n\nPreset: talk\n").replace(
            "| File | Needs | Holds | Block |", "| File | Needs | Holds |\n|---|---|---|\n"
            "| 01-build | none | edit src/main.rs and commit |\n\n| File | Needs | Holds | Block |", 1), "no round in"),
        "changed Holds": (PLAN.replace("# Plan: x\n", "# Plan: x\n\nPreset: talk\n").replace(
            "| the reply |", "| edit src/main.rs and commit |"), "no round in"),
        "changed Needs": (PLAN.replace("# Plan: x\n", "# Plan: x\n\nPreset: talk\n").replace(
            "| 01-reply | none |", "| 01-reply | reply |"), "no round in"),
        "mode of a verb with no mode presets": (PLAN.replace("# Plan: x\n", "# Plan: x\n\nPreset: talk fast\n"),
                                                "talk has no mode presets; write `Preset: talk`"),
    }
    for name, (text, expected) in cases.items():
        run = Path(tempfile.mkdtemp())
        (run / "plan.md").write_text(text, encoding="utf-8")
        (run / "plan.html").write_text("<p>chart</p>", encoding="utf-8")
        try:
            start_plan.main([str(run)])
            error = None
        except SystemExit as stop:
            error = str(stop)
        assert error and expected in error and not (run / "01-reply.md").exists(), (name, error)
    # A chart block above the Preset line, as an older plan_chart.py wrote it, does not hide that line.
    run = Path(tempfile.mkdtemp())
    chart = "<!-- plan-chart:start -->\n```mermaid\nflowchart TD\n```\n<!-- plan-chart:end -->\n"
    (run / "plan.md").write_text(PLAN.replace("# Plan: x\n", f"# Plan: x\n\n{chart}\nPreset: talk\n"), encoding="utf-8")
    (run / "plan.html").write_text("<p>chart</p>", encoding="utf-8")
    with contextlib.redirect_stdout(io.StringIO()):
        start_plan.main([str(run)])
    assert (run / "01-reply.md").exists(), "a chat plan equal to its preset starts"


TWO_STEP_PLAN = """# Plan: two

Done when: y.
- y
Evidence:
- z

## Steps

### 1 · Ask

Step: `01-ask`

- Ask the owner one question.

### 2 · Reply

Step: `02-reply`

- Reply in plain words.

## Step table

| File | Needs | Holds | Writes | Block | Why |
|---|---|---|---|---|---|
| 01-ask | none | the ask | none | requirements | the ask frames it |
| 02-reply | ask | the reply | none | voice-reply | the owner reads it |
"""


def test_a_changed_plan_refuses_the_old_step_files():
    """A re-approved plan whose existing steps differ refuses before it writes; an unchanged or grown plan starts."""
    cases = {  # change -> the part of the refusal, or None when the start goes on
        "unchanged": (TWO_STEP_PLAN, None),
        "changed Needs": (TWO_STEP_PLAN.replace("| 02-reply | ask |", "| 02-reply | none |"), "02-reply (needs: was 01-ask, now none)"),
        "changed Block": (TWO_STEP_PLAN.replace("| none | voice-reply |", "| none | requirements |"),
                          "02-reply (blocks: was voice, voice-reply, now requirements)"),
        "changed Writes": (TWO_STEP_PLAN.replace("| the reply | none |", "| the reply | docs/ |"), "02-reply (writes: was none, now docs/)"),
        "changed Holds": (TWO_STEP_PLAN.replace("| the reply |", "| the reply with three sources |"),
                          "02-reply (holds: now the reply with three sources)"),
        "changed brief": (TWO_STEP_PLAN.replace("Ask the owner one question.", "Ask the team one question."), "01-ask (brief changed)"),
        "dropped step": (TWO_STEP_PLAN[:TWO_STEP_PLAN.index("| 02-reply")], "02-reply (no row in plan.md)"),
        "added step": (TWO_STEP_PLAN + "| 03-note | reply | a note | none | voice-reply | a later step |\n", None),
    }
    for name, (text, expected) in cases.items():
        run = Path(tempfile.mkdtemp())
        (run / "plan.md").write_text(TWO_STEP_PLAN, encoding="utf-8")
        approve(run)
        assert start(run).returncode == 0
        before = {p.name: p.read_text(encoding="utf-8") for p in run.glob("0*.md")}
        (run / "plan.md").write_text(text, encoding="utf-8")
        approve(run)
        out = start(run)
        if expected is None:
            assert out.returncode == 0, (name, out.stderr)
            assert name != "added step" or (run / "03-note.md").exists(), "a grown plan makes the new file"
            continue
        assert out.returncode == 1 and expected in out.stderr, (name, out.stderr)
        assert "plan.md or this repo's .blocks.toml changed after these steps were made: " in out.stderr, out.stderr
        assert f"Start the new plan in a new run folder, such as {run}-2." in out.stderr, out.stderr
        assert {p.name: p.read_text(encoding="utf-8") for p in run.glob("0*.md")} == before, (name, "a refusal writes no file")


def test_a_taken_step_still_matches_its_plan():
    """take adds the input revision to the Uses line; that alone is no change to the plan."""
    run = Path(tempfile.mkdtemp())
    (run / "plan.md").write_text(TWO_STEP_PLAN, encoding="utf-8")
    approve(run)
    assert start(run).returncode == 0
    reply = run / "02-reply.md"
    reply.write_text(reply.read_text(encoding="utf-8").replace("Uses: 01-ask\n", "Uses: 01-ask@0123456789ab\n"), encoding="utf-8")
    assert "Uses: 01-ask@0123456789ab\n" in reply.read_text(encoding="utf-8")
    out = start(run)
    assert out.returncode == 0, out.stderr


def test_a_holds_cell_cut_back_at_a_colon_refuses():
    """The first todo of an open step is `- [ ] <holds>: `, so a Holds cell cut back at its colon is a change."""
    run = Path(tempfile.mkdtemp())
    (run / "plan.md").write_text(TWO_STEP_PLAN.replace("| the reply |", "| the reply: one line |"), encoding="utf-8")
    approve(run)
    assert start(run).returncode == 0
    (run / "plan.md").write_text(TWO_STEP_PLAN, encoding="utf-8")
    approve(run)
    out = start(run)
    assert out.returncode == 1 and "02-reply (holds: now the reply)" in out.stderr, out


def test_a_checked_holds_with_no_space_is_no_change():
    """done takes evidence right after the colon, so `- [x] <holds>:evidence` still matches its plan."""
    run = Path(tempfile.mkdtemp())
    (run / "plan.md").write_text(TWO_STEP_PLAN, encoding="utf-8")
    approve(run)
    assert start(run).returncode == 0
    ask = run / "01-ask.md"
    text = ask.read_text(encoding="utf-8")
    assert "\n- [ ] the ask: \n" in text, text
    ask.write_text(text.replace("\n- [ ] the ask: \n", "\n- [x] the ask:evidence\n"), encoding="utf-8")
    out = start(run)
    assert out.returncode == 0, out.stderr


def test_a_todo_head_in_rules_text_is_skipped():
    """A `## Todo (` line in rules text with no item under it is not the list, as step_run.todo_scan reads it."""
    run = Path(tempfile.mkdtemp())
    (run / "plan.md").write_text(TWO_STEP_PLAN, encoding="utf-8")
    approve(run)
    assert start(run).returncode == 0
    ask = run / "01-ask.md"
    text = ask.read_text(encoding="utf-8")
    head = text.index("## Todo (")
    ask.write_text(text[:head] + "## Todo (an example in rules text)\nno item here\n" + text[head:], encoding="utf-8")
    out = start(run)
    assert out.returncode == 0, out.stderr


def test_a_kit_update_alone_is_no_change():
    """A part the kit adds later, or a block the kit no longer has, is no change; a resolver error stops the start
    with its reason; a block the plan drops refuses."""
    run = Path(tempfile.mkdtemp())
    (run / "plan.md").write_text(TWO_STEP_PLAN, encoding="utf-8")
    approve(run)
    assert start(run).returncode == 0
    reply = run / "02-reply.md"
    text = reply.read_text(encoding="utf-8")
    assert re.search(r"block:voice@\w+, ", text), text
    reply.write_text(re.sub(r"block:voice@\w+, ", "", text), encoding="utf-8")
    out = start(run)
    assert out.returncode == 0, out.stderr
    gone_id = "gone-from" + "-the-kit"  # built here, so the kit's block reference check does not read it as a real one
    reply.write_text(text.replace("Skills read: ", f"Skills read: block:{gone_id}@0, "), encoding="utf-8")
    out = start(run)
    assert out.returncode == 0, out.stderr
    sys.path.insert(0, str(SCRIPT.parent))
    import start_plan

    def gone(*args, **kwargs):
        raise SystemExit("unknown block voice-reply")

    with patch.object(start_plan.step_run, "resolve_start_blocks", gone):
        try:
            start_plan.drift(run)
            raise AssertionError("a resolver error must stop the start")
        except SystemExit as stop:
            assert str(stop) == "unknown block voice-reply", stop
    reply.write_text(text.replace("Skills read: ", "Skills read: block:requirements@0, "), encoding="utf-8")
    out = start(run)
    assert out.returncode == 1 and "02-reply (blocks: was requirements, voice, voice-reply, now voice, voice-reply)" in out.stderr, out


def test_a_repo_off_block_is_no_change():
    """start leaves a block the repo's .blocks.toml turns off out of the file, so drift leaves it out too."""
    repo = Path(tempfile.mkdtemp())
    subprocess.run(["git", "init", "-q", str(repo)], check=True)
    (repo / "README.md").write_text("x\n", encoding="utf-8")
    (repo / ".blocks.toml").write_text('[params]\nsource_of_truth = "README.md"\n\n[off]\nrequirements = "this repo has none"\n',
                                       encoding="utf-8")
    run = repo / "run"
    run.mkdir()
    (run / "plan.md").write_text(TWO_STEP_PLAN, encoding="utf-8")
    approve(run)
    assert start(run).returncode == 0
    assert "block:requirements" not in (run / "01-ask.md").read_text(encoding="utf-8")
    out = start(run)
    assert out.returncode == 0, out.stderr
    # A block the repo's [add] gives a step after it was made changes its rules, so it refuses.
    (repo / "docs/mine").mkdir(parents=True)
    (repo / "docs/mine/SKILL.md").write_text("---\nname: mine\ndescription: A repo block. Use when testing.\n---\n# mine\n\n## Rules\n- one rule\n\n"
                                            "## Example\nOne.\n\n## Check (script)\n- none: a test block.\n\n## Check (judge)\n- none: a test block.\n",
                                            encoding="utf-8")
    toml = (repo / ".blocks.toml").read_text(encoding="utf-8")
    (repo / ".blocks.toml").write_text(toml.replace("[off]", '[add]\n02-reply = ["docs/mine/SKILL.md"]\n\n[off]'), encoding="utf-8")
    out = start(run)
    assert out.returncode == 1 and "02-reply (blocks: was " in out.stderr and "mine" in out.stderr, out
    (repo / ".blocks.toml").write_text(toml, encoding="utf-8")
    # A broken .blocks.toml does not hide a changed Block cell: the start stops with the config error.
    (repo / ".blocks.toml").write_text("this is [not toml\n", encoding="utf-8")
    (run / "plan.md").write_text(TWO_STEP_PLAN.replace("| none | voice-reply |", "| none | voice-reply, link-check |"), encoding="utf-8")
    approve(run)
    out = start(run)
    assert out.returncode == 1 and "blocks.toml" in out.stderr and "Traceback" not in out.stderr, out
    (repo / ".blocks.toml").write_text(toml.replace("[off]\n", '[off]\n"zz\\u001b[2Jq" = "odd"\n'), encoding="utf-8")
    out = start(run)
    assert out.returncode == 1 and "zz[2Jq" in out.stderr and "\x1b" not in out.stderr, out


def test_a_damaged_step_file_and_control_text():
    run = Path(tempfile.mkdtemp())
    (run / "plan.md").write_text(TWO_STEP_PLAN, encoding="utf-8")
    approve(run)
    assert start(run).returncode == 0
    reply = run / "02-reply.md"
    text = reply.read_text(encoding="utf-8")
    reply.write_text(text.replace("Writes: none\n", "Writes: src/\x1b[2J\n"), encoding="utf-8")
    out = start(run)
    assert out.returncode == 1 and "02-reply (writes: was src/[2J, now none)" in out.stderr and "\x1b" not in out.stderr, out
    # A todo head that an older kit worded otherwise is no change, as step_run reads it by its prefix.
    old_head = "## Todo (check a box only with its evidence after the last colon)"
    reply.write_text(text.replace("## Todo (check a box only with its evidence after the colon; `done` refuses an empty one)", old_head),
                     encoding="utf-8")
    assert old_head in reply.read_text(encoding="utf-8")
    out = start(run)
    assert out.returncode == 0, out.stderr
    reply.write_bytes(text.encode("utf-8") + b"\xff\n")
    out = start(run)
    assert out.returncode == 1 and "02-reply.md is not UTF-8" in out.stderr and "Traceback" not in out.stderr, out
    reply.write_text("Status: open\n", encoding="utf-8")
    out = start(run)
    assert out.returncode == 1 and f"{reply} is not a step file" in out.stderr and "Traceback" not in out.stderr, out
    sys.path.insert(0, str(SCRIPT.parent))
    import start_plan
    odd = run / "02-reply\x1b[2J.md"
    reply.rename(odd)
    (run / "plan.md").write_text(TWO_STEP_PLAN.replace("| 02-reply |", "| 02-reply\x1b[2J |"), encoding="utf-8")
    try:
        start_plan.drift(run)
        raise AssertionError("a damaged step file must refuse")
    except SystemExit as stop:
        assert "is not a step file" in str(stop) and "\x1b" not in str(stop), repr(str(stop))


if __name__ == "__main__":
    test_start_needs_the_current_approval()
    test_the_preset_sets_the_stop()
    test_a_forged_preset_line_keeps_the_owner_stop()
    test_a_changed_plan_refuses_the_old_step_files()
    test_a_taken_step_still_matches_its_plan()
    test_a_holds_cell_cut_back_at_a_colon_refuses()
    test_a_checked_holds_with_no_space_is_no_change()
    test_a_todo_head_in_rules_text_is_skipped()
    test_a_kit_update_alone_is_no_change()
    test_a_repo_off_block_is_no_change()
    test_a_damaged_step_file_and_control_text()
    sys.path.insert(0, str(SCRIPT.parent))  # python3 -I does not add the script folder
    from test_start_plan_auto import test_auto_approval
    test_auto_approval(PLAN)
    print("ok")
