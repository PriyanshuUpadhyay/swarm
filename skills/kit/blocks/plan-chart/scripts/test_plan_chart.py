"""Run: python3 test_plan_chart.py"""
import hashlib, html, json, re, shutil, subprocess, sys, tempfile
from pathlib import Path
from unittest.mock import patch


SCRIPT = Path(__file__).with_name("plan_chart.py")
KIT = Path(__file__).resolve().parents[3]
STEP_RUN = KIT / "references" / "step_run.py"
START_PLAN = KIT / "blocks" / "plan-review" / "scripts" / "start_plan.py"
sys.path.insert(0, str(SCRIPT.parent))
sys.path.insert(0, str(KIT / "skills" / "implement" / "scripts"))
import implement_run
import block_resolver
import plan_chart

CHOSEN = ["requirements", "local-research", "web-ui", "design-tokens", "voice", "build-site", "web-ui-basics", "change-review",
          "voice-reply"]


def ledger(chosen=CHOSEN):
    """The blocks table: one row for each Chosen block."""
    rows = [f"| {n} | Chosen | the task needs it |" for n in chosen]
    return "\n## Blocks considered\n\n| Block | Status | Why |\n|---|---|---|\n" + "\n".join(rows) + "\n"


PLAN = """# Plan: add dark mode to settings

Done when: the settings page has a dark theme.

- the page shows dark colors from `tokens.css`
- the theme follows the system setting

Evidence:
- a screenshot at 375 px and 1280 px
- a `vitest` case for the setting

| File | Needs | Holds | Writes | Block | Why |
|---|---|---|---|---|---|
| 01-frame | none | done-when claims | none | requirements | the claims say when the work is done |
| 02-read | frame | findings with paths | none | local-research | find where `theme.ts` sets it |
| 03-tokens | frame | DESIGN.md tokens | `DESIGN.md` | design-tokens | the dark colors need tokens |
| 04-build | read, tokens | the page | `web/src/`, `DESIGN.md` | build-site, web-ui-basics | make the change |
| 05-reply | build | the reply | none | change-review, voice-reply | the owner reads the result |
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
    # The page is the whole plan: the step table, and a Skill blocks section in place of the blocks table, with
    # one row per Chosen block, each part right under its parent, its SKILL.md description, and its steps.
    assert "<td>findings with paths</td>" in page and ">Blocks considered<" not in page, page
    used = page.split('id="used-in-this-plan-9"')[1].split("</table>")[0]
    assert used.index("<code>voice</code>") < used.index("<code>voice-reply</code>"), used[:400]
    assert "<td>Write the final answer to the owner" in used and "<td>05-reply</td>" in used, used
    assert "Offered, left off" not in page and "Other blocks in the kit (" in page, page
    assert out.stdout == f"{folder / 'plan.html'}\n", out.stdout
    # The approved plan runs as a step run with no change, and each step file carries its Writes.
    run = subprocess.run([sys.executable, "-I", STEP_RUN, "start", folder / "run", folder / "plan.md"], capture_output=True, text=True)
    assert run.returncode == 0, run.stderr
    lines = (folder / "run" / "04-build.md").read_text(encoding="utf-8").splitlines()
    assert lines[1:3] == ["Uses: 02-read, 03-tokens", "Writes: web/src/, DESIGN.md"], lines[:4]
    assert (folder / "run" / "02-read.md").read_text(encoding="utf-8").splitlines()[2] == "Writes: none"


def test_an_offered_block_that_no_step_loads_is_listed_apart():
    out, folder = chart(PLAN + "| html-diagram | Offered | a drawn flow may help the owner later |\n")
    assert out.returncode == 0, out.stderr
    page = (folder / "plan.html").read_text(encoding="utf-8")
    offered = page.split('id="offered-left-off-1"')[1].split("</table>")[0]
    assert "<td><code>html-diagram</code></td>" in offered and "a drawn flow may help the owner later" in offered, offered
    assert "<code>html-diagram</code>" not in page.split("Other blocks in the kit (")[1], "an offered block is not in Other"


def test_page_is_readable():
    out, folder = chart(PLAN)
    assert out.returncode == 0, out.stderr
    page = (folder / "plan.html").read_text(encoding="utf-8")
    # Backticks become <code> after escaping, and Done when claims are a list.
    assert "<code>theme.ts</code>" in page and "`" not in re.sub(r"<script>.*?</script>", "", page.split("</style>")[1], flags=re.S), page[:400]
    assert "<li>the page shows dark colors from <code>tokens.css</code></li>" in page, page
    assert "<p>Evidence:</p>\n<ul><li>a screenshot at 375 px and 1280 px</li>" in page, page
    assert inline_escapes_html()
    # Stages wrap or stack, never one fixed row that cuts stages off.
    assert "grid-auto-flow:column" not in page and "auto-fit" in page, page
    # The markdown table keeps a backtick in Why.
    assert plan_chart.read_plan(folder / "plan.md")[2][1][4] == "find where `theme.ts` sets it"


def test_mermaid_chart():
    out, folder = chart(PLAN)
    assert out.returncode == 0, out.stderr
    md = (folder / "plan.md").read_text(encoding="utf-8")
    # The fence sits right under the title, between the markers.
    assert md.startswith("# Plan: add dark mode to settings\n\n<!-- plan-chart:start -->\n```mermaid\nflowchart TD\n"), md[:200]
    chart_text = md.split("```mermaid\n")[1].split("```")[0]
    # Only a stage of 2 or more steps gets a subgraph; the chart holds the flow and no dropped block.
    assert chart_text.split("  s1 --> s2")[0] == "\n".join([
        "flowchart TD", '  s1["01-frame<br/>requirements"]', '  subgraph S2["2 · parallel"]',
        '    s2["02-read<br/>local-research"]', '    s3["03-tokens<br/>design-tokens"]', "  end",
        '  s4["04-build<br/>build-site · web-ui-basics"]', '  s5(["05-reply<br/>change-review · voice-reply"])', ""]), chart_text
    assert "  s2 --> s4\n  s3 --> s4\n  s4 --> s5\n" in chart_text and "build-canvas" not in chart_text, chart_text
    # The fence ends at the end marker, with no line between.
    assert md.split("```\n", 1)[1].startswith("<!-- plan-chart:end -->"), md[:900]
    # plan.html draws the same text with the pinned Pele build, offline.
    page = (folder / "plan.html").read_text(encoding="utf-8")
    assert html.escape(chart_text.rstrip("\n")) in page and "/*! Pele 1.6.0 | MIT License" in page and "<script src" not in page
    rendered = pele_render(chart_text)
    assert rendered is None or rendered.startswith("<svg"), rendered


def pele_render(text):
    """The SVG that the pinned Pele makes from text, or None when node is not installed."""
    if not shutil.which("node"):
        return None
    js = ("const fs=require('fs');new Function(fs.readFileSync(process.argv[1],'utf8')+';globalThis.pele=pele')();"
          "process.stdout.write(pele.render(fs.readFileSync(0,'utf8')).svg)")
    out = subprocess.run(["node", "-e", js, KIT / "blocks" / "plan-chart" / "assets" / "pele.global.js"],
                         input=text, capture_output=True, text=True)
    assert out.returncode == 0, out.stderr[-500:]
    return out.stdout


def test_hostile_label():
    hostile = 'a"] --> evil[x | <b>#35; `c` (d) {e} %% f; & g\nh'
    text = plan_chart.label(hostile, "two")
    assert text == '"a#34;#93; --#62; evil#91;x #124; #60;b#62;#35;35#59; #96;c#96; #40;d#41; #123;e#125; #37;#37; f#59; #38; g h<br/>two"', text
    rendered = pele_render(f"flowchart TD\n  s1[{text}]\n  s2[{plan_chart.label('b')}]\n  s1 --> s2\n")
    assert rendered is None or "a&quot;] --&gt; evil[x" in rendered, rendered[-2000:]


def test_chart_replace_is_idempotent():
    out, folder = chart(PLAN)
    assert out.returncode == 0, out.stderr
    plan = folder / "plan.md"
    first = plan.read_bytes()
    assert subprocess.run([sys.executable, "-I", SCRIPT, plan], capture_output=True).returncode == 0
    assert plan.read_bytes() == first
    plan.write_text(first.decode().replace("| read, tokens |", "| read |").replace("| 05-reply | build |", "| 05-reply | build, tokens |").replace("| `web/src/`, `DESIGN.md` |", "| `web/src/` |"), encoding="utf-8")
    assert subprocess.run([sys.executable, "-I", SCRIPT, plan], capture_output=True).returncode == 0
    text = plan.read_text(encoding="utf-8")
    assert text.count("plan-chart:start") == 1 and text.count("```mermaid") == 1 and "  s3 --> s5\n" in text and "  s3 --> s4" not in text, text[:900]


def approve(run):
    files = {"../plan.html": hashlib.sha256((run / "plan.html").read_bytes()).hexdigest()}
    (run / "plan-review").mkdir(exist_ok=True)
    result = {"decision": "approved", "message": "", "annotationCount": 0}
    (run / "plan-review" / "1.json").write_text(json.dumps({"files": files, "result": result}))


def test_start_refuses_after_chart_change():
    out, run = chart(PLAN)
    assert out.returncode == 0, out.stderr
    approve(run)
    plan = run / "plan.md"
    approved = plan.read_text(encoding="utf-8")
    plan.write_text(approved.replace("s2 --> s4", "s2 --> s5"), encoding="utf-8")  # only the chart changed
    start = lambda: subprocess.run([sys.executable, "-I", START_PLAN, run], capture_output=True, text=True)
    refused = start()
    assert refused.returncode == 1 and "plan.md changed after the owner approved plan.html" in refused.stderr, refused
    assert subprocess.run([sys.executable, "-I", SCRIPT, plan], capture_output=True).returncode == 0
    assert plan.read_text(encoding="utf-8") == approved
    assert start().returncode == 0 and (run / "05-reply.md").is_file()


def inline_escapes_html():
    return plan_chart.inline("a <b> & `c<d`") == "a &lt;b&gt; &amp; <code>c&lt;d</code>"


def test_defects_stop():
    cases = [
        (PLAN.replace("| build-site, web-ui-basics |", "| build-site, no-such |"), "block no-such, which is not in the kit"),
        (PLAN.replace("| read, tokens |", "| read, lookz |"), "needs 'lookz', which no step gives"),
        (PLAN.replace("| none | done-when", "| reply | done-when"), "cycle: 01-frame needs 'reply'"),
        (PLAN.replace("| read, tokens |", "| re, tokens |"), "step 04-build needs 're', which fits 02-read, 05-reply; name one"),
        (PLAN.replace("| change-review, voice-reply |", "| change-review, report |"), "the last step 05-reply has no voice- block"),
        (PLAN.replace("| read, tokens |", "| read |"), "steps 03-tokens, 05-reply give output that no step uses"),
        (PLAN.replace("| the owner reads the result |", "|  |"), "step 05-reply needs Holds, Block, and Why"),
        (PLAN.replace("Done when:", "Done:"), "no `Done when:` line"),
        (PLAN.replace("- the page shows dark colors from `tokens.css`\n", "").replace("- the theme follows the system setting\n", ""), "needs a bullet per claim"),
        (PLAN.replace("- the theme follows", "  wrapped text\n- the theme follows"), "each line is a `- ` bullet"),
        (PLAN.replace("- the theme follows", "  - nested\n- the theme follows"), "each line is a `- ` bullet"),
        (PLAN.replace("Evidence:\n", ""), "needs a bullet per claim"),
        (PLAN.split("\n## Blocks considered")[0], "no | Block | Status | Why | table"),
        (PLAN.replace("| voice | Chosen |", "| voice | Maybe |"), "block voice needs Status Chosen or Offered and a Why"),
        (PLAN + "| html-diagram-x | Chosen | no |\n", "names html-diagram-x, which is not in the kit catalog"),
        (PLAN + "| report | Chosen | x |\n", "block report is Chosen but no step uses it"),
        (PLAN + "| report | Offered |  |\n", "block report needs Status Chosen or Offered and a Why"),
        (PLAN.replace("| web-ui-basics | Chosen |", "| web-ui-basics | Offered |"), "block web-ui-basics is used by a step but not Chosen"),
        (PLAN.replace("| web-ui-basics | Chosen |", "| web-ui-basics | Rejected |"), "block web-ui-basics needs Status Chosen or Offered and a Why"),
        # C4: no census; the table lists only the blocks the steps use.
        (PLAN + "| html-diagram | Rejected | no use |\n", "block html-diagram needs Status Chosen or Offered and a Why"),
        # C4: the duty blocks of a plan with Writes.
        (PLAN.replace("| none | requirements |", "| none | local-research |").replace("| requirements | Chosen | the task needs it |\n", ""),
         "the first step 01-frame has no requirements block"),
        (PLAN.replace("| change-review, voice-reply |", "| voice-reply |").replace("| change-review | Chosen | the task needs it |\n", ""),
         "step 03-tokens writes DESIGN.md, and no later step has a change-review block"),
        # C3: a plan needs Writes, and unordered steps may not write the same path.
        (re.sub(r"^(\|[^|]*\|[^|]*\|[^|]*)\|[^|]*(\|[^|]*\|[^|]*\|)$", r"\1\2", PLAN, flags=re.M),
         "the step table has no Writes column"),
        (PLAN.replace("| none | local-research |", "| `DESIGN.md/` | local-research |"),
         "steps 02-read and 03-tokens both write DESIGN.md/; order them with Needs"),
    ]
    for text, error in cases:
        out, _ = chart(text)
        assert out.returncode == 1 and error in out.stderr and "Traceback" not in out.stderr, (error, out.stderr)


def test_writes_overlap_by_path_parts():
    # 02-read and 03-tokens run in parallel: `DESIGN` is not a folder of `DESIGN.md`, and an ordered pair may share.
    out, _ = chart(PLAN.replace("| none | local-research |", "| `DESIGN` | local-research |"))
    assert out.returncode == 0, out.stderr
    import plan_rules
    assert plan_rules.overlaps("src/a/", "src/a/b.ts") and not plan_rules.overlaps("src/a", "src/ab")
    out, _ = chart(PLAN.replace("| `DESIGN.md` | design-tokens |", "| none, `DESIGN.md` | design-tokens |"))
    assert out.returncode == 1 and "Writes `none` stands alone" in out.stderr, out.stderr
    parallel = PLAN.replace("| none | local-research |", "| `src/a/b/` | local-research |").replace("| `DESIGN.md` | design-tokens |", "| `src/a/` | design-tokens |")
    out, _ = chart(parallel)
    assert out.returncode == 1 and "steps 02-read and 03-tokens both write src/a/b/; order them with Needs" in out.stderr, out.stderr
    ordered = PLAN.replace("| `DESIGN.md` | design-tokens |", "| `src/a/` | design-tokens |").replace("| `web/src/`, `DESIGN.md` |", "| `src/a/b/` |")
    out, _ = chart(ordered)
    assert out.returncode == 0, out.stderr
    assert plan_chart.read_plan(None, PLAN.replace("| 02-read |", "| `02-read.md` (optional) |"))[2][1][5:] == ([], True)


def test_shared_resolver_import():
    assert hasattr(plan_chart, "resolve_blocks"), "plan_chart must import resolve_blocks"
    assert plan_chart.resolve_blocks is implement_run.resolve_blocks
    assert plan_chart.resolve_blocks.__module__ == "block_resolver"


def test_parent_selection():
    parts = ["when-to-diagram", "html-diagram", "call-tree", "diagram-check"]
    for selected, expected in (("diagram, html-diagram, diagram", ["diagram", *parts]),
                               ("html-diagram, html-diagram", ["diagram", "html-diagram"])):
        chosen = CHOSEN[:-4] + expected + ["change-review", "voice-reply"]
        text = PLAN.split("\n## Blocks considered")[0].replace("| build-site, web-ui-basics |", f"| {selected} |") + ledger(chosen)
        out, folder = chart(text)
        assert out.returncode == 0, out.stderr
        page = (folder / "plan.html").read_text(encoding="utf-8")
        used = page.split('id="used-in-this-plan-')[1].split("</table>")[0]
        for block in expected:
            row = used.split(f"<td><code>{block}</code></td>")[1].split("</tr>")[0]
            assert row.endswith("<td>04-build</td>"), (block, row)


def test_rejected_parent_stops():
    chosen = CHOSEN[:-4] + ["html-diagram", "change-review", "voice-reply"]
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
