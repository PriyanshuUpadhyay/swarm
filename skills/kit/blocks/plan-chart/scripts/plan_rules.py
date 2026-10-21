"""The block and Writes rules of a plan: the blocks table, `conflicts`, write collisions, and the duty blocks.
plan_chart.py runs them; see its docstring for the plan format."""

import re
import sys
from pathlib import Path, PurePosixPath

KIT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(KIT / "references"))  # python3 -I does not add it
from block_resolver import read_toml, resolve_blocks  # noqa: E402


def blocks_toml():
    return read_toml(KIT / "blocks" / "blocks.toml")


def check_ledger(rows, ledger):
    """The table lists exactly the blocks the steps use, each Chosen with a Why."""
    base = set(blocks_toml().get("base", []))
    catalog = {p.parent.name for p in (KIT / "blocks").glob("*/SKILL.md")} - base
    if not ledger:
        raise SystemExit("no | Block | Status | Why | table; list each block the steps use as Chosen")
    if unknown := sorted(set(ledger) - catalog):
        raise SystemExit(f"blocks table names {unknown[0]}, which is not in the kit catalog")
    for block, (status, why) in ledger.items():
        if status != "Chosen" or not why:
            raise SystemExit(f"block {block} needs Status Chosen and a Why; the table lists only the blocks the steps use")
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
