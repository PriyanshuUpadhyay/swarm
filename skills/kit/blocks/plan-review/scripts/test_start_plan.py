"""Run: python3 test_start_plan.py"""
import contextlib, hashlib, io, json, subprocess, sys, tempfile
from pathlib import Path

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
        (kit / "skills" / verb).mkdir(parents=True)
        (kit / "skills" / verb / "SKILL.md").write_text(f"# {verb}\n\n## Preset\n\n{lines}\n\n{table}", encoding="utf-8")
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
    (kit / "skills" / "talk").mkdir(parents=True)
    (kit / "skills" / "talk" / "SKILL.md").write_text(f"# talk\n\n## Preset\n\nStop: chat\n\n{table}", encoding="utf-8")
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


if __name__ == "__main__":
    test_start_needs_the_current_approval()
    test_the_preset_sets_the_stop()
    test_a_forged_preset_line_keeps_the_owner_stop()
    print("ok")
