"""Run: python3 test_plan_page.py"""
import re, subprocess, sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from test_plan_chart import KIT, STEP_RUN, block_resolver, chart, plan_chart  # noqa: E402
import step_brief  # noqa: E402


def research_plan():
    """The research example in blocks/planner/SKILL.md, with its blocks table filled in."""
    text = (KIT / "blocks" / "planner" / "SKILL.md").read_text(encoding="utf-8").split("````\n")[1]
    rows = plan_chart.read_plan(None, text)[2]
    selected = block_resolver.resolve_blocks(None, {r[0]: r[3] for r in rows}, "default", blocks_dir=KIT / "blocks", pack=False)
    chosen = "".join(f"| {b} | Chosen | the run needs it |\n" for b in sorted({b for v in selected.values() for b in v}))
    return re.sub(r"\| requirements \| Chosen .*\n.*\n", chosen, text)


def test_research_plan_page():
    out, folder = chart(research_plan())
    assert out.returncode == 0, out.stderr
    md = (folder / "plan.md").read_text(encoding="utf-8")
    assert md.index("<details>") < md.index(plan_chart.START) < md.index("| File |"), "the chart sits in the fold"
    page = (folder / "plan.html").read_text(encoding="utf-8")
    for part in ('<div class="k">Question 1</div>', '<section class="part block">', '<span class="tag known">Known</span>',
                 '<p class="so"><b>So in this plan:</b>', '<span class="stepid"><code>03-web</code></span>',
                 '<div class="chips"><code>Twilio verified caller ID non-Twilio number</code><code>Twilio caller ID per subaccount</code></div>',
                 '<summary>How it runs (for the agent)</summary>', '<div class="chart">flowchart TD'):
        assert part in page, part
    # The approved brief goes into its step file, word for word, without its Step line.
    run = subprocess.run([sys.executable, "-I", STEP_RUN, "start", folder / "run", folder / "plan.md"], capture_output=True, text=True)
    assert run.returncode == 0, run.stderr
    web = (folder / "run" / "03-web.md").read_text(encoding="utf-8")
    assert "## Brief (approved in plan.md)\n### 2 · Check Twilio's rules\n\n- Look in Twilio's" in web, web
    assert "  - `Twilio caller ID per subaccount`\n" in web and "Step: `03-web`" not in web, web
    assert web.index("## Brief") < web.index("## Todo"), web
    assert "## Brief" not in (folder / "run" / "01-question.md").read_text(encoding="utf-8")


def test_research_plan_gaps_stop():
    plan = research_plan()
    big = "*Answers: how?*\n\n```mermaid\nflowchart LR\n" + "".join(f"  n{i}[x] --> n{i + 1}[y]\n" for i in range(10)) + "```\n"
    cases = [
        (plan.replace("Step: `02-local`\n", ""),
         'step 02-local has no brief under "## Steps" (add "### N · <plain name>" with "Step: 02-local")'),
        (plan.replace("  - `Twilio caller ID per subaccount`\n", ""), 'step 03-web needs "- Seed searches:" with two or more'),
        (plan.replace("So in this plan: step 1", "Then step 1"),
         'building block "Block 1 · Caller ID is one field on the outbound leg" has no "So in this plan:" line'),
        (plan.replace("\n## Steps\n", "\n```mermaid\nflowchart LR\n  a[x] --> b[y]\n```\n\n## Steps\n"),
         "the diagram that starts `flowchart LR` has no Answers: line over it"),
        (plan.replace("\n## Steps\n", f"\n{big}\n## Steps\n"), "the diagram that starts `flowchart LR` has 11 nodes"),
    ]
    for text, error in cases:
        out, _ = chart(text)
        assert out.returncode == 1 and error in out.stderr and "Traceback" not in out.stderr, (error, out.stderr)
        # Only a research plan is held to these sections yet.
        out, _ = chart(text.replace("Preset: research\n", ""))
        assert out.returncode == 0, (error, out.stderr)


def test_briefs_skip_fences():
    text = "## Steps\n\n### 1 · a\n\nStep: `02-a`\n\n```text\n## Steps\n### not a step\n```\n- Result: x\n\n## Not doing\n"
    assert step_brief.briefs(text) == {"02-a": "### 1 · a\n\n```text\n## Steps\n### not a step\n```\n- Result: x\n"}


if __name__ == "__main__":
    for name, test in list(globals().items()):
        if name.startswith("test_"):
            test()
    print("ok")
