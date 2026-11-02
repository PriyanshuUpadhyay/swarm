"""Auto approval cases for start_plan.py --auto; test_start_plan.py runs them."""
import subprocess, sys, tempfile
from pathlib import Path

SCRIPT = Path(__file__).with_name("start_plan.py")
WORDS = "Do the changes then run the trials"


def draw(run, plan):
    sys.path.insert(0, str(SCRIPT.parent))
    import start_plan
    (run / "plan.md").write_text(plan, encoding="utf-8")
    (run / "plan.html").write_text(start_plan.preset.plan_chart.page(run / "plan.md"), encoding="utf-8")


def auto(run, source):
    return subprocess.run([sys.executable, "-I", SCRIPT, run, "--auto", source], capture_output=True, text=True)


def test_auto_approval(plan):
    """The owner's words in the file start the plan; words not found, no words line, and an edit after refuse."""
    run = Path(tempfile.mkdtemp())
    note = run / "owner.txt"
    note.write_text(f"swarm: new message\n{WORDS}.\n", encoding="utf-8")
    draw(run, plan)
    out = auto(run, note)
    assert out.returncode == 1 and "no line Auto approval" in out.stderr and not (run / "plan-review").exists(), out
    draw(run, plan.replace("Done when:", f'Auto approval: "{WORDS.lower()}"\n\nDone when:'))
    out = auto(run, note)
    assert out.returncode == 1 and "are not in" in out.stderr and not (run / "plan-review").exists(), out
    signed = plan.replace("Done when:", f'Auto approval: "{WORDS}"\n\nDone when:')
    (run / "plan.md").write_text(signed, encoding="utf-8")
    out = auto(run, note)
    assert out.returncode == 1 and "run plan-chart first" in out.stderr, "a page drawn from an older plan.md refuses"
    draw(run, signed)
    out = auto(run, note)
    assert out.returncode == 0 and "round 1: approved by the owner's words" in out.stdout, out
    assert (run / "01-reply.md").read_text().startswith("Status: open\n"), out
    assert WORDS in (run / "plan-review" / "1.json").read_text()
    (run / "plan.md").write_text(signed.replace("it |", "it now |"), encoding="utf-8")
    out = subprocess.run([sys.executable, "-I", SCRIPT, run], capture_output=True, text=True)
    assert out.returncode == 1 and "plan.md changed after the owner approved plan.html" in out.stderr, out
    assert subprocess.run([sys.executable, "-I", SCRIPT, run, "--bad", note], capture_output=True).returncode != 0
