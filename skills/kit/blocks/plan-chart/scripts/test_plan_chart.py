"""Run: python3 test_plan_chart.py"""
import re, subprocess, sys, tempfile
from pathlib import Path


SCRIPT = Path(__file__).with_name("plan_chart.py")
KIT = Path(__file__).resolve().parents[3]
STEP_RUN = KIT / "references" / "step_run.py"

CHOSEN = ["requirements", "local-research", "design-tokens", "build-site", "web-ui-basics", "voice-reply"]


def ledger(chosen=CHOSEN, drop=()):
    """The blocks table: every catalog block but the base one, Chosen when a step uses it."""
    base = re.search(r"^base = \[(.*?)\]", (KIT / "blocks" / "blocks.toml").read_text(encoding="utf-8"), re.M)[1]
    names = sorted(p.parent.name for p in (KIT / "blocks").glob("*/SKILL.md") if f'"{p.parent.name}"' not in base)
    rows = [f"| {n} | {'Chosen' if n in chosen else 'Rejected'} | {'the task needs it' if n in chosen else 'the task has no use for it'} |"
            for n in names if n not in drop]
    return "\n## Blocks considered\n\n| Block | Status | Why |\n|---|---|---|\n" + "\n".join(rows) + "\n"


PLAN = """# Plan: add dark mode to settings

Done when: the settings page has a dark theme; evidence: a screenshot at 375 px and 1280 px.

| File | Needs | Holds | Block | Why |
|---|---|---|---|---|
| 01-frame | none | done-when claims | requirements | the claims say when the work is done |
| 02-read | frame | findings with paths | local-research | find where the theme is set |
| 03-tokens | frame | DESIGN.md tokens | design-tokens | the dark colors need tokens |
| 04-build | read, tokens | the page | build-site, web-ui-basics | make the change |
| 05-reply | build | the reply | voice-reply | the owner reads the result |
""" + ledger()


def chart(text):
    folder = Path(tempfile.mkdtemp())
    (folder / "plan.md").write_text(text, encoding="utf-8")
    out = subprocess.run([sys.executable, "-I", SCRIPT, folder / "plan.md"], capture_output=True, text=True)
    return out, folder


def test_good_plan():
    out, folder = chart(PLAN)
    assert out.returncode == 0, out.stderr
    page = (folder / "plan.html").read_text(encoding="utf-8")
    assert "Stage 2, 2 steps run in parallel" in page and page.count('<li class="step') == 5, page
    assert "findings with paths from <b>02-read</b>; DESIGN.md tokens from <b>03-tokens</b>" in page, page
    assert '<li class="step end"><h2>05-reply' in page, page
    # Rejected sub-blocks sit under their alias, and the stdout carries the table for the plan reply.
    assert "<h3>Rejected (" in page and "<code>tech-diagram</code> (sub-blocks)<ul><li><code>when-to-diagram</code>" in page, page
    assert "### Chosen" in out.stdout and "| ↳ html-diagram | the task has no use for it |" in out.stdout, out.stdout
    # The approved plan runs as a step run with no change.
    run = subprocess.run([sys.executable, "-I", STEP_RUN, "start", folder / "run", folder / "plan.md"], capture_output=True, text=True)
    assert run.returncode == 0, run.stderr
    assert (folder / "run" / "04-build.md").read_text(encoding="utf-8").splitlines()[1] == "Uses: 02-read, 03-tokens"


def test_defects_stop():
    cases = [
        (PLAN.replace("| build-site, web-ui-basics |", "| diagram |"), "diagram is a parent; name its parts until the shared resolver lands"),
        (PLAN.replace("| build-site, web-ui-basics |", "| build-site, no-such |"), "block no-such, which is not in the kit"),
        (PLAN.replace("| read, tokens |", "| read, lookz |"), "needs 'lookz', which no step gives"),
        (PLAN.replace("| none | done-when", "| reply | done-when"), "loop among steps"),
        (PLAN.replace("| voice-reply |", "| report |"), "the last step 05-reply has no voice- block"),
        (PLAN.replace("| read, tokens |", "| read |"), "steps 03-tokens, 05-reply give output that no step uses"),
        (PLAN.replace("| the owner reads the result |", "|  |"), "step 05-reply needs Holds, Block, and Why"),
        (PLAN.replace("Done when:", "Done:"), "no `Done when:` line"),
        (PLAN.split("\n## Blocks considered")[0], "no | Block | Status | Why | table"),
        (PLAN.replace("| html-diagram | Rejected |", "| html-diagram | Maybe |"), "block html-diagram needs Status Chosen or Rejected"),
        (PLAN.replace("| html-diagram | Rejected |", "| html-diagram-x | Rejected |"), "names html-diagram-x, which is not in the kit catalog"),
        (PLAN.replace("| html-diagram | Rejected | the task has no use for it |\n", ""), "leaves out html-diagram"),
        (PLAN.replace("| report | Rejected |", "| report | Chosen |"), "block report is Chosen but no step uses it"),
        (PLAN.replace("| web-ui-basics | Chosen |", "| web-ui-basics | Rejected |"), "block web-ui-basics is used by a step but not Chosen"),
    ]
    for text, error in cases:
        out, _ = chart(text)
        assert out.returncode == 1 and error in out.stderr and "Traceback" not in out.stderr, (error, out.stderr)


if __name__ == "__main__":
    for name, test in list(globals().items()):
        if name.startswith("test_"):
            test()
    print("ok")
