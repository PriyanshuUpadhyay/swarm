#!/usr/bin/env python3
"""Check a plan's step table and draw it as one HTML page for the owner to review.

  plan_chart.py <plan.md>    write the chart into plan.md and plan.html beside it, or stop with the first defect and exit 1

`Done when:` is a result line, then a bullet per claim, then `Evidence:` and a bullet per proof.
The table is the step-run table (| File | Needs | Holds |) with three more columns, Writes, Block and Why, so
`references/step_run.py start <run> <plan.md>` runs the same file once the owner approves it. Writes is `none` or the
paths a step may change; two steps that no Needs path orders may not write the same path. A File cell may end in
`(optional)`, the only rows `skip` takes. Only a plan that equals its `Stop: chat` preset may leave Writes out.
A plan with Writes keeps its duty blocks: `requirements` in each first step, `change-review` after each writing step,
and a `voice-` block at the end.
A second table, | Block | Status | Why |, lists each block the steps use as Chosen, and may list a block that no
step uses as Offered. The script checks it against the step table, draws it under the chart, and prints it for
the plan reply.
The Mermaid chart goes into plan.md between the START and END markers, so the approved plan.md
holds the chart the owner saw, and plan.html draws the same text with the pinned Pele build in ../assets.
"""

import html
import re
import sys
from pathlib import Path

KIT = Path(__file__).resolve().parents[3]
sys.path[:0] = [str(KIT / "references"), str(Path(__file__).parent)]  # python3 -I adds neither
import step_run  # noqa: E402
from block_resolver import read_parents, resolve_blocks  # noqa: E402
from plan_rules import check_ledger, check_writes, conflicts  # noqa: E402

ASSETS = Path(__file__).resolve().parents[1] / "assets"
START, END = "<!-- plan-chart:start -->", "<!-- plan-chart:end -->"
HEAD = re.compile(r"\|\s*File\s*\|\s*Needs\s*\|\s*Holds\s*\|(\s*Writes\s*\|)?\s*Blocks?\s*\|\s*Why\s*\|")  # Blocks: step_run.py writes the block rules into each step
OPTIONAL = re.compile(r"\s*\(optional\)$")


def read_plan(path, text=None):
    """(title, done_when, rows) where a row is (file, needs, holds, blocks, why, writes, optional); `text` replaces the
    file's text. writes is None for a table with no Writes column, else a list of paths ([] for `none`)."""
    text = Path(path).read_text(encoding="utf-8") if text is None else text
    title = next((line[2:].strip() for line in text.splitlines() if line.startswith("# ")), "Plan")
    done = next((line.split(":", 1)[1].strip() for line in text.splitlines() if line.startswith("Done when:")), "")
    rows, inside, width = [], False, 5
    for line in text.splitlines():
        if head := HEAD.match(line):
            inside, width = True, 6 if head[1] else 5
        elif inside and line.startswith("|---"):
            continue
        elif inside and line.startswith("|"):
            cells = [c.strip() for c in line.strip().strip("|").split("|")]
            if len(cells) != width:
                raise SystemExit(f"row has {len(cells)} cells, not {width}: {line}")
            writes = step_run.step_scope.write_paths(cells.pop(3)) if width == 6 else None
            name, need, holds, blocks, why = cells  # Holds and Why keep their `code` spans
            optional = bool(OPTIONAL.search(name))
            name, need, blocks = (c.strip("`") for c in (OPTIONAL.sub("", name), need, blocks))
            rows.append((name.removesuffix(".md"), need, holds, [b.strip() for b in blocks.split(",") if b.strip()], why,
                         writes, optional))
        elif inside:
            break
    return title, done, rows


def read_claims(path):
    """([claim], [proof]) from the bullets under `Done when:` and `Evidence:`, up to the step table."""
    claims, proofs, inside, target = [], [], False, None
    for line in Path(path).read_text(encoding="utf-8").splitlines():
        if line.startswith("Done when:"):
            inside, target = True, claims
        elif inside and line[:1] in ("|", "#"):
            break
        elif inside and line.strip().lower() == "evidence:":
            target = proofs
        elif inside and line.startswith("- "):
            target.append(line[2:].strip())
        elif inside and line.strip():
            # A wrapped, nested, or prose line would be lost from plan.html, so the plan must not have one.
            raise SystemExit(f"under `Done when:` each line is a `- ` bullet at the line start, `Evidence:`, or blank: {line}")
    if not claims or not proofs:
        raise SystemExit("`Done when:` needs a bullet per claim, then `Evidence:` and a bullet per proof")
    return claims, proofs


LEDGER_HEAD = re.compile(r"\|\s*Block\s*\|\s*Status\s*\|\s*Why\s*\|")


def read_ledger(path):
    """{block: (status, why)} from the | Block | Status | Why | table."""
    ledger, inside = {}, False
    for line in Path(path).read_text(encoding="utf-8").splitlines():
        if LEDGER_HEAD.match(line):
            inside = True
        elif inside and line.startswith("|---"):
            continue
        elif inside and line.startswith("|"):
            cells = [c.strip() for c in line.strip().strip("|").split("|")]
            if len(cells) != 3:
                raise SystemExit(f"blocks row has {len(cells)} cells, not 3: {line}")
            cells[0], cells[1] = cells[0].strip("`"), cells[1].strip("`")  # Why keeps its `code` spans
            if cells[0] in ledger:
                raise SystemExit(f"block {cells[0]} is listed twice in the blocks table")
            ledger[cells[0]] = (cells[1], cells[2])
        elif inside:
            break
    return ledger


def parents():
    return read_parents({p.parent.name: p for p in sorted((KIT / "blocks").glob("*/SKILL.md"))})


def check(title, done, rows):
    """Each step's needed step files, or SystemExit with the first defect."""
    if not rows:
        raise SystemExit("no | File | Needs | Holds | Block | Why | table")
    if not done:
        raise SystemExit("no `Done when:` line")
    names = [r[0] for r in rows]
    if dupes := sorted({n for n in names if names.count(n) > 1}):
        raise SystemExit(f"step {dupes[0]} appears twice")
    for name, need, holds, blocks, why, *_ in rows:
        if not re.fullmatch(r"\d\d-[a-z0-9-]+", name):
            raise SystemExit(f"step {name} is not NN-name")
        if not holds or not why or not blocks:
            raise SystemExit(f"step {name} needs Holds, Block, and Why")
        for block in blocks:
            if not (KIT / "blocks" / block / "SKILL.md").is_file():
                raise SystemExit(f"step {name} names block {block}, which is not in the kit")
    uses = step_run.check_needs([r[:3] for r in rows])
    selected = resolve_blocks(None, {r[0]: r[3] for r in rows}, "default", blocks_dir=KIT / "blocks", pack=False)
    in_plan = {b for blocks in selected.values() for b in blocks}
    for block in sorted(in_plan):
        if clash := sorted(set(conflicts(block)) & in_plan):
            raise SystemExit(f"blocks {block} and {clash[0]} conflict; keep one")
    stages(uses)  # stops on a loop
    used = {n for need in uses.values() for n in need}
    ends = [n for n in names if n not in used]
    if len(ends) != 1:
        raise SystemExit(f"steps {', '.join(ends)} give output that no step uses; the plan has one end")
    end = next(r for r in rows if r[0] == ends[0])
    if not any(b.startswith("voice-") for b in end[3]):
        raise SystemExit(f"the last step {end[0]} has no voice- block")
    if any(len(r) > 5 and r[5] is not None for r in rows):
        check_writes(rows, uses, selected)
    return uses


def stages(uses):
    """Steps grouped by stage: a step runs one stage after the last step it needs, so a stage runs in parallel."""
    stage = {}
    while len(stage) < len(uses):
        ready = [n for n, need in uses.items() if n not in stage and all(u in stage for u in need)]
        if not ready:
            raise SystemExit(f"loop among steps {', '.join(sorted(set(uses) - set(stage)))}")
        for n in ready:
            stage[n] = 1 + max((stage[u] for u in uses[n]), default=-1)
    return [sorted(n for n in stage if stage[n] == s) for s in range(max(stage.values()) + 1)]


CSS = """
body{font:16px/1.5 system-ui,sans-serif;margin:2rem;color:#1d1d1f;background:#fafafa}
h1{font-size:1.4rem;margin:0 0 .25rem}
.done{margin:0 0 1.5rem;color:#444}
.done ul{margin:.25rem 0 .75rem}
ol.stages{list-style:none;padding:0;margin:0;display:grid;gap:1.5rem}
ol.stages>li:not(:last-child)::after{content:"\\2193";display:block;text-align:center;font-size:1.4rem;color:#888;margin-top:.5rem}
.stage{font-size:13px;text-transform:uppercase;letter-spacing:.04em;color:#666;margin:0 0 .5rem}
ul.steps{list-style:none;padding:0;margin:0;display:grid;grid-template-columns:repeat(auto-fit,minmax(16rem,1fr));gap:.75rem}
.step{background:#fff;border:1px solid #d0d0d5;border-radius:8px;padding:.75rem}
.step h2{font-size:1rem;margin:0 0 .25rem}
.step p{margin:.25rem 0;font-size:14px}
.step code{font-size:13px;background:#eef;padding:0 .25rem;border-radius:3px}
.end{border-color:#6a5acd}
#chart{margin:0;overflow-x:auto}
#chart pre{white-space:pre-wrap;font-size:13px}
.pele{--pele-bg:#fafafa;--pele-font:system-ui,sans-serif}
table{border-collapse:collapse;font-size:14px}
td{padding:.2rem .75rem .2rem 0;vertical-align:top}
"""


def inline(text):
    """Escape text, then show each `code` span as <code>."""
    return re.sub(r"`([^`]+)`", r"<code>\1</code>", html.escape(text))


def render_done(done, claims, proofs):
    items = lambda entries: "<ul>" + "".join(f"<li>{inline(e)}</li>" for e in entries) + "</ul>"
    return f'<div class="done"><p>Done when: {inline(done)}</p>{items(claims)}<p>Evidence:</p>{items(proofs)}</div>'


def short(text, words=12):
    """The first `words` words of a reason, with a cut `code` span closed."""
    cut = text.split()
    if len(cut) <= words:
        return text
    text = " ".join(cut[:words]) + "…"
    return text + "`" if text.count("`") % 2 else text


def ledger_rows(ledger):
    """[("Chosen", [(label, why)]), ("Offered", ...)]: one row per block, each part right under its parent.
    The Offered group shows only when the plan has an Offered row."""
    part_of = {part: parent for parent, parts in parents().items() for part in parts}
    groups = []
    for status in ("Chosen", "Offered"):
        names = {b for b, (s, _) in ledger.items() if s == status}
        # A part sits under its parent only when the parent is in the same group.
        under = {b: part_of[b] for b in names if part_of.get(b) in names}
        order = sorted(names, key=lambda b: (under.get(b, b), b in under, b))
        if order or status == "Chosen":
            groups.append((status, [(f"↳ {b}" if b in under else b, short(ledger[b][1])) for b in order]))
    return groups


def ledger_markdown(ledger):
    out = []
    for status, rows in ledger_rows(ledger):
        out += [f"### {status} ({len(rows)})", "", "| Block | Why |", "|---|---|"]
        out += [f"| {label} | {why.replace('|', '&#124;')} |" for label, why in rows] + [""]
    return "\n".join(out)


def render_ledger(ledger):
    out = ['<section><h2>Blocks considered</h2><p>Say "drop X", "add Y", or "add sub-block Z" and the plan is made again.</p>']
    for status, rows in ledger_rows(ledger):
        items = "".join(f"<tr><td><code>{html.escape(label)}</code></td><td>{inline(why)}</td></tr>" for label, why in rows)
        out.append(f"<h3>{status} ({len(rows)})</h3><table>{items}</table>")
    return "".join(out) + "</section>"


def label(*lines):
    """A quoted Mermaid label: each line escaped as #code; so no text can end the label or the graph."""
    return '"' + "<br/>".join(re.sub(r'[#"<>\[\]{}()|&`%;]', lambda m: f"#{ord(m[0])};", " ".join(t.split())) for t in lines) + '"'


def mermaid(rows, uses):
    """flowchart TD text: a node per step, arrows from Needs, and a subgraph only round a stage of parallel steps."""
    ids = {r[0]: f"s{i}" for i, r in enumerate(rows, 1)}
    by_name = {r[0]: r for r in rows}
    used = {u for need in uses.values() for u in need}
    out = ["flowchart TD"]
    for i, group in enumerate(stages(uses), 1):
        if len(group) > 1:
            out.append(f"  subgraph S{i}[{label(f'{i} · parallel')}]")
        for step in group:
            text = label(step, " · ".join(by_name[step][3]))
            out.append(f"{'    ' if len(group) > 1 else '  '}{ids[step]}{f'([{text}])' if step not in used else f'[{text}]'}")
        if len(group) > 1:
            out.append("  end")
    out += [f"  {ids[u]} --> {ids[n]}" for n in ids for u in uses[n]]
    return "\n".join(out) + "\n"


def put_chart(plan, chart):
    """Write the chart fence between the markers, right under the title; replace the old one."""
    text = plan.read_text(encoding="utf-8")
    block = f"{START}\n```mermaid\n{chart}```\n{END}"
    if START in text and END in text:
        new = text[:text.index(START)] + block + text[text.index(END) + len(END):]
    else:
        lines = text.split("\n")
        at = next((i + 1 for i, line in enumerate(lines) if line.startswith("# ")), 0)
        new = "\n".join(lines[:at] + ["", block] + lines[at:])
    if new != text:
        plan.write_text(new, encoding="utf-8")


def render(title, done, rows, uses, ledger, claims, proofs, chart):
    holds = {r[0]: r[2] for r in rows}
    by_name = {r[0]: r for r in rows}
    selected = resolve_blocks(None, {r[0]: r[3] for r in rows}, "default", blocks_dir=KIT / "blocks", pack=False)
    out = []
    for i, group in enumerate(stages(uses), 1):
        how = "runs alone" if len(group) == 1 else f"{len(group)} steps run in parallel"
        cards = []
        for name in group:
            _, _, gives, _, why, *_ = by_name[name]
            blocks = selected[name]
            gets = "; ".join(f"{inline(holds[u])} from <b>{html.escape(u)}</b>" for u in uses[name]) or "the task"
            end = "" if any(name in v for v in uses.values()) else " end"
            cards.append(
                f'<li class="step{end}"><h2>{html.escape(name)}</h2>'
                f'<p>Blocks: {", ".join(f"<code>{html.escape(b)}</code>" for b in blocks)}</p>'
                f"<p>Gets: {gets}</p>"
                f"<p>Gives: {inline(gives)}</p><p>Why: {inline(why)}</p></li>")
        out.append(f'<li><p class="stage">Stage {i}, {how}</p><ul class="steps">{"".join(cards)}</ul></li>')
    return (f'<!doctype html><html lang="en"><meta charset="utf-8"><title>{html.escape(title)}</title>'
            f"<style>{CSS}</style><h1>{html.escape(title)}</h1>"
            f"{render_done(done, claims, proofs)}"
            f'<div id="chart"><pre>{html.escape(chart)}</pre></div>'
            f"<script>{(ASSETS / 'pele.global.js').read_text(encoding='utf-8')}</script>"
            "<script>const c=document.getElementById('chart');pele.mount(c,c.textContent)</script>"
            f'<figure><ol class="stages">{"".join(out)}</ol>'
            f"<figcaption>Stages run top to bottom. Steps in one stage run in parallel. "
            f"Each step gets the outputs that its Gets line names.</figcaption></figure>{render_ledger(ledger)}</html>\n")


def chat_preset(plan):
    """True when the plan names a `Stop: chat` preset and equals its steps, so it may leave Writes out."""
    sys.path.insert(0, str(KIT / "skills" / "plan" / "scripts"))
    import preset
    file = preset.of_plan(plan)
    return bool(file) and preset.section(file)[0] == "chat" and preset.same_steps(plan, file)


def main(argv):
    usage = __doc__.strip().splitlines()[2].strip()
    if argv in (["-h"], ["--help"]):
        print(usage)
        return
    if len(argv) != 1 or argv[0].startswith("-"):
        raise SystemExit(usage)
    plan = Path(argv[0])
    title, done, rows = read_plan(plan)
    if rows and rows[0][5] is None and not chat_preset(plan):
        raise SystemExit("the step table has no Writes column; add one (`none` for a step that writes only its run folder)")
    uses = check(title, done, rows)
    claims, proofs = read_claims(plan)
    ledger = read_ledger(plan)
    check_ledger(rows, ledger)
    chart = mermaid(rows, uses)
    put_chart(plan, chart)
    page = plan.with_suffix(".html")
    page.write_text(render(title, done, rows, uses, ledger, claims, proofs, chart), encoding="utf-8")
    print(page)
    print(ledger_markdown(ledger))


if __name__ == "__main__":
    main(sys.argv[1:])
