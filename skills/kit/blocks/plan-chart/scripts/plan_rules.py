"""The block and Writes rules of a plan: the blocks table, `conflicts`, write collisions, and the duty blocks.
plan_chart.py runs them; see its docstring for the plan format."""

import re
import sys
from pathlib import Path, PurePosixPath

KIT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(KIT / "references"))  # python3 -I does not add it
from block_resolver import read_toml, resolve_blocks  # noqa: E402
from step_brief import briefs, fences  # noqa: E402


def blocks_toml():
    return read_toml(KIT / "blocks" / "blocks.toml")


def is_duty(block):
    """A block that every plan with Writes keeps, as plan_chart.check requires: `requirements` first,
    `change-review` after a writing step, and a `voice-` part at the end."""
    return block in ("requirements", "change-review") or block.startswith("voice-")


def kit_catalog():
    """The block ids a plan or a preset may name: each kit block folder, less the base blocks that load in every step."""
    return {p.parent.name for p in (KIT / "blocks").glob("*/SKILL.md")} - set(blocks_toml().get("base", []))


def check_ledger(rows, ledger):
    """The table lists exactly the blocks the steps use, each Chosen with a Why. A block that no step uses may
    have an Offered row with a Why, so the owner can ask to add it."""
    base = set(blocks_toml().get("base", []))
    catalog = kit_catalog()
    if not ledger:
        raise SystemExit("no | Block | Status | Why | table; list each block the steps use as Chosen")
    if unknown := sorted(set(ledger) - catalog):
        raise SystemExit(f"blocks table names {unknown[0]}, which is not in the kit catalog")
    for block, (status, why) in ledger.items():
        if status not in ("Chosen", "Offered") or not why:
            raise SystemExit(f"block {block} needs Status Chosen or Offered and a Why; the table lists the blocks "
                             "the steps use, and Offered ones that no step uses")
    chosen = {b for b, (status, _) in ledger.items() if status == "Chosen"}
    selected = resolve_blocks(None, {r[0]: r[3] for r in rows}, "default", blocks_dir=KIT / "blocks", pack=False)
    used = {b for blocks in selected.values() for b in blocks} - base
    if chosen != used:
        odd = sorted(chosen ^ used)[0]
        raise SystemExit(f"block {odd} is {'Chosen but no step uses it' if odd in chosen else 'used by a step but not Chosen'}")


def conflicts(block):
    text = (KIT / "blocks" / block / "SKILL.md").read_text(encoding="utf-8")
    line = re.search(r"^  conflicts: (.*)$", text[3:].partition("\n---\n")[0], re.M)  # the frontmatter only
    return [c.strip() for c in line[1].strip("\"'").split(",") if c.strip()] if line else []


def before(uses):
    """{step: every step it reaches back to through Needs}."""
    seen = {}

    def walk(name):
        if name not in seen:
            seen[name] = set()
            for u in uses[name]:
                seen[name] |= {u} | walk(u)
        return seen[name]

    for name in uses:
        walk(name)
    return seen


def overlaps(a, b):
    """True when one path is the other or a folder that holds it, compared by path parts."""
    a, b = PurePosixPath(a).parts, PurePosixPath(b).parts
    return a[:len(b)] == b or b[:len(a)] == a


def check_writes(rows, uses, selected):
    """The Writes rules and the duty blocks of a plan with a Writes column, or SystemExit with the first defect."""
    writes = {r[0]: r[5] or [] for r in rows}
    earlier = before(uses)
    names = [r[0] for r in rows]
    for i, a in enumerate(names):
        for b in names[i + 1:]:
            if a in earlier[b] or b in earlier[a]:
                continue
            if clash := next((p for p in writes[a] for q in writes[b] if overlaps(p, q)), None):
                raise SystemExit(f"steps {a} and {b} both write {clash}; order them with Needs")
    for name in names:
        if not uses[name] and "requirements" not in selected[name]:
            raise SystemExit(f"the first step {name} has no requirements block")
        if writes[name] and not any(name in earlier[n] and "change-review" in selected[n] for n in names):
            raise SystemExit(f"step {name} writes {writes[name][0]}, and no later step has a change-review block")


def check_layout(text, rows, uses):
    """SystemExit with the first gap in a research plan's page sections: briefs, seed searches, blocks, diagrams.
    `text` is plan.md without the step chart that plan_chart.py writes."""
    found = briefs(text)
    if extra := sorted(set(found) - {row[0] for row in rows}):
        raise SystemExit(f"the brief with Step: {extra[0]} names no step of the table; fix its Step line")
    used = {u for need in uses.values() for u in need}
    loads = resolve_blocks(None, {r[0]: r[3] for r in rows}, "default", blocks_dir=KIT / "blocks", pack=False)
    for name, need, *_ in rows:
        if need == "none" or name not in used:
            continue  # the frame and the reply have no work to brief
        if name not in found:
            raise SystemExit(f'step {name} has no brief under "## Steps" (add "### N · <plain name>" with "Step: {name}")')
        if "deep-search" in loads[name]:  # also through a parent such as `sources`
            seeds = re.search(r"^- Seed searches[^\n]*\n((?:  - .*\n)*)", found[name], re.M)
            if not seeds or seeds[1].count("  - ") < 2:
                raise SystemExit(f'step {name} needs "- Seed searches:" with two or more "  - " lines in its brief')
    part = text.split("\n## Building blocks\n", 1)[1].split("\n## ", 1)[0] if "\n## Building blocks\n" in text else ""
    for block in re.split(r"^(?=### )", part, flags=re.M)[1:]:
        if block.startswith("### Block") and "\nSo in this plan:" not in block:
            raise SystemExit(f'building block "{block.splitlines()[0][4:]}" has no "So in this plan:" line')
    lines = text.split("\n")
    for lang, i, fence in fences(text):
        if lang != "mermaid":
            continue  # a fence inside another fence is quoted text, as the page shows it
        first = (fence.splitlines() or ["(empty)"])[0]
        if not any("Answers:" in l for l in lines[max(0, i - 3):i]):
            raise SystemExit(f"the diagram that starts `{first}` has no Answers: line over it")
        if (count := nodes(fence)) > 10:
            raise SystemExit(f"the diagram that starts `{first}` has {count} nodes; split it to 10 or fewer")


def nodes(fence):
    """The node count of a Mermaid diagram: participants, or ids defined with a label."""
    # ponytail: counts ids given a label only; a node that is never given a label is missed.
    fence = re.sub(r"^\s*%%.*$", "", fence, flags=re.M)  # a comment line
    if re.search(r"^\s*sequenceDiagram\b", fence, re.M):
        return len(re.findall(r"^\s*(?:participant|actor)\s", fence, re.M))
    if re.search(r"^\s*classDiagram\b", fence, re.M):
        return len(set(re.findall(r"^\s*class\s+([\w`]+)", fence, re.M)))
    text = re.sub(r'"[^"]*"', '""', re.sub(r"^\s*(?:subgraph|style|classDef|class|click|linkStyle) .*$", "", fence, flags=re.M))
    text = re.sub(r"\|[^|\n]*\||(?<![-=.])(?:--|==|-\.)\s[^\n]*?(?=-->|==>|\.->|--[ox]|---|===|\.-)", "", text)  # edge labels
    # Each match takes its whole label, so a word inside a label is never read as an id.
    found = re.findall(r"(?<![\w-])([A-Za-z_]\w*)\s*(?:\[[^\]]*\]|\([^)]*\)+|\{[^}]*\}+)", text)
    return len(set(found))
