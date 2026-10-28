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
step uses as Offered. The script checks it against the step table, and plan.html draws it as the Skill blocks
section. The script prints only the path of plan.html.
The Mermaid chart goes into plan.md between the START and END markers, in the `<details>` fold that holds the step
table (else under the title), so the approved plan.md holds the chart the owner saw.
plan.html is the whole plan.md drawn as one page, with a Skill blocks section made from the blocks table and each
block's SKILL.md description, and every Mermaid fence drawn with the pinned Pele build in ../assets. The owner
reviews only plan.html. A plan whose `Preset:` line names research must also give a brief under `## Steps` to each
step between the first and the last, two or more seed searches in a `deep-search` step's brief, a `So in this plan:`
line in each `### Block` under `## Building blocks`, and an `Answers:` line over each diagram, of at most 10 nodes.
"""

import re
import sys
from pathlib import Path

KIT = Path(__file__).resolve().parents[3]
sys.path[:0] = [str(KIT / "references"), str(Path(__file__).parent)]  # python3 -I adds neither
import step_run  # noqa: E402
from block_resolver import resolve_blocks  # noqa: E402
from plan_page import inline, parents, render  # noqa: E402,F401 (inline, parents: callers read them here)
from plan_rules import check_layout, check_ledger, check_writes, conflicts  # noqa: E402

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
    """([claim], [proof]) from the bullets under `Done when:` and `Evidence:`, up to a table, heading, or fold."""
    claims, proofs, inside, target = [], [], False, None
    for line in Path(path).read_text(encoding="utf-8").splitlines():
        if line.startswith("Done when:"):
            inside, target = True, claims
        elif inside and line[:1] in ("|", "#", "<"):
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
    """Write the chart fence between the markers: in place of the old one, else at the top of the `<details>` fold
    that holds the step table, else right under the title."""
    text = plan.read_text(encoding="utf-8")
    block = f"{START}\n```mermaid\n{chart}```\n{END}"
    lines = text.split("\n")
    head = next((i for i, line in enumerate(lines) if HEAD.match(line)), None)
    fold = next((i for i in range(head or 0, -1, -1) if lines[i].strip() in ("<details>", "</details>")), None)
    if START in text and END in text:
        new = text[:text.index(START)] + block + text[text.index(END) + len(END):]
    elif head is not None and fold is not None and lines[fold].strip() == "<details>":
        new = "\n".join(lines[:head] + [block, ""] + lines[head:])
    else:
        at = next((i + 1 for i, line in enumerate(lines) if line.startswith("# ")), 0)
        new = "\n".join(lines[:at] + ["", block] + lines[at:])
    if new != text:
        plan.write_text(new, encoding="utf-8")


def page(plan):
    """The plan.html text for plan.md as it is now. start_plan.py compares it with the approved page, so the review
    of plan.html alone also binds plan.md."""
    return render(plan.read_text(encoding="utf-8"), read_plan(plan)[2], read_ledger(plan))


def preset_of(plan):
    """The preset file that the plan's `Preset:` line names, or None."""
    sys.path.insert(0, str(KIT / "skills" / "plan" / "scripts"))
    import preset
    return preset.of_plan(plan)


def chat_preset(plan):
    """True when the plan names a `Stop: chat` preset and equals its steps, so it may leave Writes out."""
    file = preset_of(plan)
    import preset
    return bool(file) and preset.section(file)[0] == "chat" and preset.same_steps(plan, file)


def research(plan):
    """True when the plan starts from the research preset, the only one whose page sections are checked yet."""
    return preset_of(plan) == KIT / "skills" / "research" / "SKILL.md"


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
    read_claims(plan)
    ledger = read_ledger(plan)
    check_ledger(rows, ledger)
    text = plan.read_text(encoding="utf-8")
    if (START in text) != (END in text):
        raise SystemExit(f"plan.md has one of the chart markers {START} and {END}; restore both, or delete both")
    if research(plan):
        check_layout(text.split(START)[0] + text.split(END)[-1] if START in text else text, rows, uses)
    put_chart(plan, mermaid(rows, uses))
    plan.with_suffix(".html").write_text(page(plan), encoding="utf-8")
    print(plan.with_suffix(".html"))


if __name__ == "__main__":
    main(sys.argv[1:])
