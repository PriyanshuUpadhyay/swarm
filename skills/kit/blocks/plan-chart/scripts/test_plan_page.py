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


def refused(call):
    try:
        call()
    except SystemExit as stop:
        return str(stop)
    return None


def test_briefs_skip_fences():
    text = "## Steps\n\n### 1 · a\n\nStep: `02-a`\n\n```text\n### not a step\n```\n- Result: x\n\n## Not doing\n"
    assert step_brief.briefs(text) == {"02-a": "### 1 · a\n\n```text\n### not a step\n```\n- Result: x\n"}
    # A `## ` line would end the step file's own sections, and two briefs for one step would lose one.
    assert "starts with `## `" in refused(lambda: step_brief.briefs(text.replace("### not a step", "## Todo (x)")))
    assert "hidden" not in step_brief.briefs(text.replace("- Result: x", "<!-- hidden -->\n- Result: x"))["02-a"]
    assert "  <!-- shown -->" in step_brief.briefs(text.replace("- Result: x", "  <!-- shown -->\n- Result: x"))["02-a"]
    assert "<!-- kept -->" in step_brief.briefs(text.replace("### not a step", "<!-- kept -->"))["02-a"]
    assert "Skills read: " in refused(lambda: step_brief.briefs(text.replace("- Result: x", "Skills read: x")))
    nested = text.replace("```text\n### not a step\n```", "````md\n```\n### not a step\n```\n````")
    assert list(step_brief.briefs(nested)) == ["02-a"] and "### not a step" in step_brief.briefs(nested)["02-a"]
    assert "two briefs" in refused(lambda: step_brief.briefs(text.replace("## Not doing", "### 2 · b\n\nStep: `02-a`\n\n## Not doing")))


def test_diagram_checks():
    import plan_rules
    labels = "flowchart TD\n" + "".join(f'  n{i}["Read (fast) [x]"] --> m{i}(Write {{disk}})\n' for i in range(5))
    assert plan_rules.nodes(labels) == 10, plan_rules.nodes(labels)
    edges = 'flowchart LR\n  a[x] -->|calls (once)| b[y]\n  b -- reads (cache) --> c[z]\n  c -. waits (x) .-> a\n'
    assert plan_rules.nodes(edges) == 3, plan_rules.nodes(edges)
    for link in ("== go (now) ==>", "-- see (x) --o", "-- see (x) --x", "-- see (x) ---", "-. wait (x) .->"):
        assert plan_rules.nodes(f"flowchart LR\n  a[x] {link} b[y]\n") == 2, link
    for chain in ("a[x] -.-> b[y] -.-> c[z]", "a[x] --- b[y] --> c[z]", "a[x] -- go --> b[y] -- on --> c[z]"):
        assert plan_rules.nodes(f"flowchart LR\n  {chain}\n") == 3, chain
    big = "```mermaid\nflowchart LR\n" + "".join(f"  n{i}[x] --> n{i + 1}[y]\n" for i in range(11)) + "```\n"
    # Each fence is checked, not only the last, and its Answers line must sit right over it.
    text = f"*Answers: a*\n{big}\n*Answers: b*\n```mermaid\nflowchart LR\n  a[x]\n```\n"
    assert "has 12 nodes" in refused(lambda: plan_rules.check_layout(text, [], {}))
    text = "*Answers: a*\n```mermaid\nflowchart LR\n  a[x]\n```\n\n\n\n\n```mermaid\nflowchart LR\n  b[x]\n```\n"
    assert "no Answers: line" in refused(lambda: plan_rules.check_layout(text, [], {}))
    rows = [("03-web", "local", "x", ["sources"], "y")]
    brief = "## Steps\n\n### 1 · web\n\nStep: `03-web`\n\n- Result: x\n"
    assert "Seed searches" in refused(lambda: plan_rules.check_layout(brief, rows, {"04-report": ["03-web"]}))
    spaced = "``` mermaid\nflowchart LR\n" + "".join(f"  n{i}[x]\n" for i in range(11)) + "```\n"
    assert "no Answers: line" in refused(lambda: plan_rules.check_layout(spaced, [], {}))
    four = "````mermaid\nflowchart LR\n" + "".join(f"  n{i}[x]\n" for i in range(11)) + "````\n"
    assert "no Answers: line" in refused(lambda: plan_rules.check_layout(four, [], {}))
    assert "`(empty)` has no Answers" in refused(lambda: plan_rules.check_layout("```mermaid\n```\n", [], {}))


def test_page_guards():
    import plan_page
    assert "uses only plain tags" in refused(lambda: plan_page.convert("```html\n<script>x</script>\n```"))
    assert "uses only plain tags" in refused(lambda: plan_page.convert('```html\n<div onclick="x">a</div>\n```'))
    for bad in ("<style>.part{display:none}</style>", "<svg/onload=alert(1)>", '<a href="&#106;avascript:x">a</a>',
                '<div style="background:url(x)">a</div>', "<img src=x onerror=y>", '<div title="a>b" onclick="x">c</div>', '<div style="background:u\\72l(x)">a</div>',
                '<div style="position: fixed;inset:0">a</div>'):
        assert "uses only plain tags" in refused(lambda: plan_page.convert(f"```html\n{bad}\n```")), bad
    assert ".flow>li:not(:last-child)::after" in plan_page.CSS  # the html-diagram flow, since a fence has no <style>
    assert plan_page.convert('```html\n<div class="box">a</div>\n```') == '<div class="figure"><div class="box">a</div></div>'
    assert "<table>" in plan_page.convert("|---|---|")
    chips = plan_page.dress(plan_page.convert("## Steps\n\n- Seed searches:\n  - one\n  - `two`\n"))
    assert '<div class="chips"><code>one</code><code>two</code></div>' in chips, chips
    import plan_rules
    assert "names no step" in refused(lambda: plan_rules.check_layout("## Steps\n\n### 1 · a\n\nStep: `09-x`\n", [], {}))
    assert "<td>a | b</td>" in plan_page.convert("| x | y |\n|---|---|\n| a \\| b | c |")
    page = plan_page.dress('<h2 id="building-blocks">B</h2><h3>x</h3><p>To check in step 12, look.</p>')
    assert "To check · step 12</span> look." in page, page
    # A blocks table row that names no kit block draws, so start_plan.py can give its own refusal.
    assert "(not in the kit)" in plan_page.skill_blocks([], {"no-such": ("Offered", "x")})


def test_amend_adds_deep_search_outside_research():
    text = research_plan().replace("Preset: research\n", "").replace("\n| Block | Status", "\nAmend: 1\n\n| Block | Status")
    out, folder = chart(text)
    assert out.returncode == 0, out.stderr
    import test_plan_chart
    test_plan_chart.approve(folder)
    start = [sys.executable, "-I", KIT / "blocks" / "plan-review" / "scripts" / "start_plan.py", folder]
    assert subprocess.run(start, capture_output=True, text=True).returncode == 0
    row = "| 06-more | local | more search | none | deep-search | a gap |"
    out = subprocess.run([sys.executable, "-I", STEP_RUN, "amend", folder, row, "--before", "04-report"], capture_output=True, text=True)
    assert out.returncode == 0, out.stderr


def test_chart_markers_in_order():
    swapped = research_plan().replace("<details>", "<!-- plan-chart:end -->\n\n<!-- plan-chart:start -->\n\n<details>")
    out, _ = chart(swapped)
    assert "once each and in that order" in out.stderr, out.stderr


def test_kit_update_keeps_the_approval_and_amend_adds_a_brief():
    out, folder = chart(research_plan())
    assert out.returncode == 0, out.stderr
    import test_plan_chart
    test_plan_chart.approve(folder)
    page = folder / "plan.html"
    page.write_text(page.read_text(encoding="utf-8").replace("The kit has", "The kit now has"), encoding="utf-8")
    test_plan_chart.approve(folder)  # the owner approved a page drawn by another kit version
    start = [sys.executable, "-I", KIT / "blocks" / "plan-review" / "scripts" / "start_plan.py", folder]
    assert subprocess.run(start, capture_output=True, text=True).returncode == 0
    plan = folder / "plan.md"
    plan.write_text(plan.read_text(encoding="utf-8").replace("\n| Block | Status", "\nAmend: 2\n\n| Block | Status"), encoding="utf-8")
    row = "| 06-probe | local | one more check | none | local-research | a gap in step 1 |"
    out = subprocess.run([sys.executable, "-I", STEP_RUN, "amend", folder, row, "--before", "04-report"], capture_output=True, text=True)
    assert out.returncode == 0, out.stderr
    assert "## Brief (approved in plan.md)\n### Amended · 06-probe" in (folder / "06-probe.md").read_text(encoding="utf-8")
    row = "| 07-more | local | more search | none | sources | a gap |"
    out = subprocess.run([sys.executable, "-I", STEP_RUN, "amend", folder, row, "--before", "04-report"], capture_output=True, text=True)
    assert "cannot add deep-search to a research plan" in out.stderr, out.stderr
    # A plan.md with CRLF line ends keeps its approval: the page and the start hash the same text.
    plan.write_bytes(plan.read_bytes().replace(b"\n", b"\r\n"))
    assert subprocess.run([sys.executable, "-I", plan_chart.__file__, plan], capture_output=True).returncode == 0
    test_plan_chart.approve(folder)
    result = subprocess.run(start, capture_output=True, text=True)
    assert result.returncode == 0, result.stderr


if __name__ == "__main__":
    for name, test in list(globals().items()):
        if name.startswith("test_"):
            test()
    print("ok")
