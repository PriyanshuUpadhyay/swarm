"""Run: python3 test_plan_chart.py"""
import re, shutil, subprocess, sys, tempfile
from pathlib import Path
from unittest.mock import patch


SCRIPT = Path(__file__).with_name("plan_chart.py")
KIT = Path(__file__).resolve().parents[3]
STEP_RUN = KIT / "references" / "step_run.py"
sys.path.insert(0, str(SCRIPT.parent))
sys.path.insert(0, str(KIT / "skills" / "implement" / "scripts"))
import implement_run
import block_resolver
import plan_chart

CHOSEN = ["requirements", "local-research", "design-tokens", "build-site", "web-ui-basics", "voice-reply"]


def ledger(chosen=CHOSEN, drop=()):
    """The blocks table: every catalog block but the base one, Chosen when a step uses it."""
    base = re.search(r"^base = \[(.*?)\]", (KIT / "blocks" / "blocks.toml").read_text(encoding="utf-8"), re.M)[1]
    names = sorted(p.parent.name for p in (KIT / "blocks").glob("*/SKILL.md") if f'"{p.parent.name}"' not in base)
    rows = [f"| {n} | {'Chosen' if n in chosen else 'Rejected'} | {'the task needs it' if n in chosen else 'the task has no use for it'} |"
            for n in names if n not in drop]
    return "\n## Blocks considered\n\n| Block | Status | Why |\n|---|---|---|\n" + "\n".join(rows) + "\n"


PLAN = """# Plan: add dark mode to settings

Done when: the settings page has a dark theme.

- the page shows dark colors from `tokens.css`
- the theme follows the system setting

Evidence:
- a screenshot at 375 px and 1280 px
- a `vitest` case for the setting

| File | Needs | Holds | Block | Why |
|---|---|---|---|---|
| 01-frame | none | done-when claims | requirements | the claims say when the work is done |
| 02-read | frame | findings with paths | local-research | find where `theme.ts` sets it |
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


def test_page_is_readable():
    out, folder = chart(PLAN)
    assert out.returncode == 0, out.stderr
    page = (folder / "plan.html").read_text(encoding="utf-8")
    # Backticks become <code> after escaping, and Done when claims are a list.
    assert "<code>theme.ts</code>" in page and "`" not in page.split("</style>")[1], page
    assert "<li>the page shows dark colors from <code>tokens.css</code></li>" in page, page
    assert "<p>Evidence:</p><ul><li>a screenshot at 375 px and 1280 px</li>" in page, page
    assert inline_escapes_html()
    # Stages wrap or stack, never one fixed row that cuts stages off.
    assert "grid-auto-flow:column" not in page and "auto-fit" in page, page
    # The markdown table keeps a backtick in Why.
    assert plan_chart.read_plan(folder / "plan.md")[2][1][4] == "find where `theme.ts` sets it"


def inline_escapes_html():
    return plan_chart.inline("a <b> & `c<d`") == "a &lt;b&gt; &amp; <code>c&lt;d</code>"


def test_defects_stop():
    cases = [
        (PLAN.replace("| build-site, web-ui-basics |", "| build-site, no-such |"), "block no-such, which is not in the kit"),
        (PLAN.replace("| read, tokens |", "| read, lookz |"), "needs 'lookz', which no step gives"),
        (PLAN.replace("| none | done-when", "| reply | done-when"), "cycle: 01-frame needs 'reply'"),
        (PLAN.replace("| read, tokens |", "| re, tokens |"), "step 04-build needs 're', which fits 02-read, 05-reply; name one"),
        (PLAN.replace("| voice-reply |", "| report |"), "the last step 05-reply has no voice- block"),
        (PLAN.replace("| read, tokens |", "| read |"), "steps 03-tokens, 05-reply give output that no step uses"),
        (PLAN.replace("| the owner reads the result |", "|  |"), "step 05-reply needs Holds, Block, and Why"),
        (PLAN.replace("Done when:", "Done:"), "no `Done when:` line"),
        (PLAN.replace("- the page shows", "the page shows").replace("- the theme follows", "the theme follows"), "needs a bullet per claim"),
        (PLAN.replace("Evidence:\n", ""), "needs a bullet per claim"),
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


def test_shared_resolver_import():
    assert hasattr(plan_chart, "resolve_blocks"), "plan_chart must import resolve_blocks"
    assert plan_chart.resolve_blocks is implement_run.resolve_blocks
    assert plan_chart.resolve_blocks.__module__ == "block_resolver"


def test_parent_selection():
    parts = ["when-to-diagram", "html-diagram", "call-tree", "diagram-check"]
    for selected, expected in (("diagram, html-diagram, diagram", ["diagram", *parts]),
                               ("html-diagram, html-diagram", ["diagram", "html-diagram"])):
        chosen = CHOSEN[:-3] + expected + ["voice-reply"]
        text = PLAN.split("\n## Blocks considered")[0].replace("| build-site, web-ui-basics |", f"| {selected} |") + ledger(chosen)
        out, folder = chart(text)
        assert out.returncode == 0, out.stderr
        page = (folder / "plan.html").read_text(encoding="utf-8")
        card = page.split("<h2>04-build</h2>")[1].split("</li>", 1)[0]
        assert card.split("</p>", 1)[0] == '<p>Blocks: ' + ", ".join(f"<code>{b}</code>" for b in expected), card


def test_rejected_parent_stops():
    chosen = CHOSEN[:-3] + ["html-diagram", "voice-reply"]
    text = PLAN.split("\n## Blocks considered")[0].replace("| build-site, web-ui-basics |", "| html-diagram |") + ledger(chosen)
    out, folder = chart(text)
    assert out.returncode == 1 and "block diagram is used by a step but not Chosen" in out.stderr, out
    assert not (folder / "plan.html").exists()


def test_empty_selection_without_catalog():
    folder = Path(tempfile.mkdtemp())
    wanted = {"build": [], "reply": []}
    assert block_resolver.resolve_blocks(None, wanted, "default", blocks_dir=folder, pack=False) == wanted
    packed, judge, _ = block_resolver.resolve_blocks(None, wanted, "default", blocks_dir=folder)
    assert packed == {key: ([], "", []) for key in wanted} and judge == ""


def test_bad_parent_stops_both():
    folder = Path(tempfile.mkdtemp())
    shutil.copytree(KIT / "blocks", folder / "blocks")
    parent = folder / "blocks" / "diagram" / "SKILL.md"
    text = parent.read_text(encoding="utf-8")
    parent.write_text(re.sub(r'^  parts: .*$', '  parts: "missing"', text, flags=re.M), encoding="utf-8")
    rows = [("01-build", "none", "page", ["diagram"], "draw it"),
            ("02-reply", "build", "reply", ["voice-reply"], "show it")]
    with patch.object(plan_chart, "KIT", folder), patch.object(block_resolver, "BLOCKS", folder / "blocks"):
        for check in (lambda: plan_chart.check("Plan", "page", rows),
                      lambda: implement_run.resolve_blocks(folder, {"build": ["diagram"]}, "default")):
            try:
                check()
            except SystemExit as error:
                assert "unknown part missing in parent diagram" in str(error), error
            else:
                raise AssertionError("a parent with a missing part must stop both scripts")


def test_cli_help_and_options():
    for args, code in ((["-h"], 0), (["--help"], 0), (["--other"], 1), (["--help", "extra"], 1), ([], 1)):
        out = subprocess.run([sys.executable, "-I", SCRIPT, *args], capture_output=True, text=True)
        assert out.returncode == code, (args, out)
        assert "plan_chart.py <plan.md>" in out.stdout + out.stderr, out
        assert "Traceback" not in out.stderr, out.stderr


if __name__ == "__main__":
    for name, test in list(globals().items()):
        if name.startswith("test_"):
            test()
    print("ok")
