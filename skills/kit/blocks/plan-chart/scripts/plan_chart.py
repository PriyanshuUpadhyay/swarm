#!/usr/bin/env python3
"""Check a plan's step table and draw it as one HTML page for the owner to review.

  plan_chart.py <plan.md>    write plan.html beside it, or stop with the first defect and exit 1

The table is the step-run table (| File | Needs | Holds |) with two more columns, Block and Why, so
`references/step_run.py start <run> <plan.md>` runs the same file once the owner approves it.
A second table, | Block | Status | Why |, lists every catalog block once as Chosen or Rejected. The script checks it
against the step table, draws it under the chart, and prints it grouped for the plan reply.
"""

import html
import re
import sys
from pathlib import Path

KIT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(KIT / "references"))  # python3 -I does not add it
import step_run  # noqa: E402

HEAD = re.compile(r"\|\s*File\s*\|\s*Needs\s*\|\s*Holds\s*\|\s*Block\s*\|\s*Why\s*\|")


def read_plan(path):
    """(title, done_when, rows) where a row is (file, needs, holds, blocks, why)."""
    text = Path(path).read_text(encoding="utf-8")
    title = next((line[2:].strip() for line in text.splitlines() if line.startswith("# ")), "Plan")
    done = next((line.split(":", 1)[1].strip() for line in text.splitlines() if line.startswith("Done when:")), "")
    rows, inside = [], False
    for line in text.splitlines():
        if HEAD.match(line):
            inside = True
        elif inside and line.startswith("|---"):
            continue
        elif inside and line.startswith("|"):
            cells = [c.strip().strip("`") for c in line.strip().strip("|").split("|")]
            if len(cells) != 5:
                raise SystemExit(f"row has {len(cells)} cells, not 5: {line}")
            name, need, holds, blocks, why = cells
            rows.append((name.removesuffix(".md"), need, holds, [b.strip() for b in blocks.split(",") if b.strip()], why))
        elif inside:
            break
    return title, done, rows


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
            cells = [c.strip().strip("`") for c in line.strip().strip("|").split("|")]
            if len(cells) != 3:
                raise SystemExit(f"blocks row has {len(cells)} cells, not 3: {line}")
            if cells[0] in ledger:
                raise SystemExit(f"block {cells[0]} is listed twice in the blocks table")
            ledger[cells[0]] = (cells[1], cells[2])
        elif inside:
            break
    return ledger


def toml_lists(section):
    """{name: [ids]} for the one-line `name = [...]` entries under [section] ("" is the top) in blocks.toml."""
    text = (KIT / "blocks" / "blocks.toml").read_text(encoding="utf-8")
    body = text.split("\n[", 1)[0] if not section else text.split(f"\n[{section}]", 1)[1].split("\n[", 1)[0]
    return {m[1]: re.findall(r'"([^"]+)"', m[2]) for m in re.finditer(r"^([\w-]+)\s*=\s*\[(.*?)\]", body, re.M)}


def check_ledger(rows, ledger):
    """The ledger names each catalog block once, and Chosen matches the blocks the steps use."""
    base = set(toml_lists("").get("base", []))
    catalog = {p.parent.name for p in (KIT / "blocks").glob("*/SKILL.md")} - base
    if not ledger:
        raise SystemExit("no | Block | Status | Why | table; list every catalog block as Chosen or Rejected")
    if unknown := sorted(set(ledger) - catalog):
        raise SystemExit(f"blocks table names {unknown[0]}, which is not in the kit catalog")
    if missing := sorted(catalog - set(ledger)):
        raise SystemExit(f"blocks table leaves out {missing[0]}; list it as Chosen or Rejected with a reason")
    for block, (status, why) in ledger.items():
        if status not in ("Chosen", "Rejected") or not why:
            raise SystemExit(f"block {block} needs Status Chosen or Rejected, and a Why")
    chosen = {b for b, (status, _) in ledger.items() if status == "Chosen"}
    used = {b for r in rows for b in r[3]} - base
    if chosen != used:
        odd = sorted(chosen ^ used)[0]
        raise SystemExit(f"block {odd} is {'Chosen but no step uses it' if odd in chosen else 'used by a step but not Chosen'}")


def ledger_groups(ledger):
    """[(status, [(label, [members])])]: sub-blocks (blocks.toml [aliases]) go under their alias. A lone block has no members."""
    aliases = toml_lists("aliases")
    groups = []
    for status in ("Chosen", "Rejected"):
        names = [b for b, (s, _) in ledger.items() if s == status]
        entries = []
        for alias, members in aliases.items():
            here = [m for m in members if m in names]
            if here:
                entries.append((alias, here))
        grouped = {m for _, ms in entries for m in ms}
        entries += [(b, []) for b in names if b not in grouped]
        groups.append((status, entries))
    return groups


def ledger_markdown(ledger):
    out = []
    for status, entries in ledger_groups(ledger):
        out += [f"### {status}", "", "| Block | Why |", "|---|---|"]
        for label, members in entries:
            if members:
                out.append(f"| {label} (sub-blocks) | |")
                out += [f"| &nbsp;&nbsp;{m} | {ledger[m][1]} |" for m in members]
            else:
                out.append(f"| {label} | {ledger[label][1]} |")
        out.append("")
    return "\n".join(out)


def conflicts(block):
    text = (KIT / "blocks" / block / "SKILL.md").read_text(encoding="utf-8")
    line = re.search(r"^  conflicts: (.*)$", text, re.M)
    return [c.strip() for c in line[1].strip("\"'").split(",") if c.strip()] if line else []


def check(title, done, rows):
    """Each step's needed step files, or SystemExit with the first defect."""
    if not rows:
        raise SystemExit("no | File | Needs | Holds | Block | Why | table")
    if not done:
        raise SystemExit("no `Done when:` line")
    names = [r[0] for r in rows]
    if dupes := sorted({n for n in names if names.count(n) > 1}):
        raise SystemExit(f"step {dupes[0]} appears twice")
    uses = {}
    for name, need, holds, blocks, why in rows:
        if not re.fullmatch(r"\d\d-[a-z0-9-]+", name):
            raise SystemExit(f"step {name} is not NN-name")
        if not holds or not why or not blocks:
            raise SystemExit(f"step {name} needs Holds, Block, and Why")
        for block in blocks:
            if not (KIT / "blocks" / block / "SKILL.md").is_file():
                raise SystemExit(f"step {name} names block {block}, which is not in the kit")
        # step_run drops a Needs word that matches no step in silence, so each word must match here.
        uses[name] = []
        for word in [w.strip() for w in re.split(r",|\band\b", need) if w.strip() and w.strip() != "none"]:
            if not (found := step_run.need_files(word, names)):
                raise SystemExit(f"step {name} needs {word!r}, which no step gives")
            uses[name] += found
        if name in uses[name]:
            raise SystemExit(f"step {name} needs itself")
    in_plan = {b for r in rows for b in r[3]}
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
ol.stages{list-style:none;padding:0;margin:0;display:grid;grid-auto-flow:column;grid-auto-columns:minmax(14rem,1fr);gap:2.5rem}
ol.stages>li{position:relative}
ol.stages>li:not(:last-child)::after{content:"\\2192";position:absolute;right:-1.9rem;top:2.2rem;font-size:1.4rem;color:#888}
.stage{font-size:13px;text-transform:uppercase;letter-spacing:.04em;color:#666;margin:0 0 .5rem}
ul.steps{list-style:none;padding:0;margin:0;display:grid;gap:.75rem}
.step{background:#fff;border:1px solid #d0d0d5;border-radius:8px;padding:.75rem}
.step h2{font-size:1rem;margin:0 0 .25rem}
.step p{margin:.25rem 0;font-size:14px}
.step code{font-size:13px;background:#eef;padding:0 .25rem;border-radius:3px}
.end{border-color:#6a5acd}
@media (max-width:600px){ol.stages{grid-auto-flow:row}
ol.stages>li:not(:last-child)::after{content:"\\2193";position:static;display:block;text-align:center;margin-top:.5rem}}
"""


def render_ledger(ledger):
    out = ['<section><h2>Blocks considered</h2><p>Say "drop X", "add Y", or "add sub-block Z" and the plan is made again.</p>']
    for status, entries in ledger_groups(ledger):
        items = []
        for label, members in entries:
            if members:
                subs = "".join(f"<li><code>{html.escape(m)}</code>: {html.escape(ledger[m][1])}</li>" for m in members)
                items.append(f"<li><code>{html.escape(label)}</code> (sub-blocks)<ul>{subs}</ul></li>")
            else:
                items.append(f"<li><code>{html.escape(label)}</code>: {html.escape(ledger[label][1])}</li>")
        out.append(f"<h3>{status} ({sum(len(m) or 1 for _, m in entries)})</h3><ul>{''.join(items)}</ul>")
    return "".join(out) + "</section>"


def render(title, done, rows, uses, ledger):
    holds = {r[0]: r[2] for r in rows}
    by_name = {r[0]: r for r in rows}
    out = []
    for i, group in enumerate(stages(uses), 1):
        how = "runs alone" if len(group) == 1 else f"{len(group)} steps run in parallel"
        cards = []
        for name in group:
            _, _, gives, blocks, why = by_name[name]
            gets = "; ".join(f"{html.escape(holds[u])} from <b>{html.escape(u)}</b>" for u in uses[name]) or "the task"
            end = "" if any(name in v for v in uses.values()) else " end"
            cards.append(
                f'<li class="step{end}"><h2>{html.escape(name)}</h2>'
                f'<p>Blocks: {", ".join(f"<code>{html.escape(b)}</code>" for b in blocks)}</p>'
                f"<p>Gets: {gets}</p>"
                f"<p>Gives: {html.escape(gives)}</p><p>Why: {html.escape(why)}</p></li>")
        out.append(f'<li><p class="stage">Stage {i}, {how}</p><ul class="steps">{"".join(cards)}</ul></li>')
    return (f'<!doctype html><html lang="en"><meta charset="utf-8"><title>{html.escape(title)}</title>'
            f"<style>{CSS}</style><h1>{html.escape(title)}</h1>"
            f'<p class="done">Done when: {html.escape(done)}</p>'
            f'<figure><ol class="stages">{"".join(out)}</ol>'
            f"<figcaption>Stages run left to right. Steps in one stage run in parallel. "
            f"Each step gets the outputs that its Gets line names.</figcaption></figure>{render_ledger(ledger)}</html>\n")


def main(argv):
    if len(argv) != 1:
        raise SystemExit(__doc__.strip().splitlines()[2].strip())
    plan = Path(argv[0])
    title, done, rows = read_plan(plan)
    uses = check(title, done, rows)
    ledger = read_ledger(plan)
    check_ledger(rows, ledger)
    page = plan.with_suffix(".html")
    page.write_text(render(title, done, rows, uses, ledger), encoding="utf-8")
    print(page)
    print(ledger_markdown(ledger))


if __name__ == "__main__":
    main(sys.argv[1:])
