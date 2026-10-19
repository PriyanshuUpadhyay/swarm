#!/usr/bin/env python3
"""Check a verb's preset and print each step with the blocks it resolves to.

  preset.py <verb> [<mode>]    check skills/<verb>/SKILL.md, or skills/<verb>/references/<mode>.md, and print its steps

A preset is a `## Preset` section with one `Stop: owner` or `Stop: chat` line, an optional
`Start: <command>` line, and one table in the plan step-table shape (| File | Needs | Holds | Block | Why |).
"""

import re
import sys
from pathlib import Path

KIT = Path(__file__).resolve().parents[3]
sys.path[:0] = [str(KIT / "references"), str(KIT / "blocks" / "plan-chart" / "scripts")]  # python3 -I adds neither
import plan_chart  # noqa: E402
from block_resolver import resolve_blocks  # noqa: E402


def path(verb, mode=None):
    if not re.fullmatch(r"[a-z0-9-]+", verb + (mode or "")):
        raise SystemExit(f"no preset file for {verb} {mode or ''}".strip())
    skill = KIT / "skills" / verb / "SKILL.md"
    if mode and skill.is_file() and "\n## Preset\n" in skill.read_text(encoding="utf-8"):
        raise SystemExit(f"{verb} has no mode presets; write `Preset: {verb}`")
    file = skill.parent / "references" / f"{mode}.md" if mode else skill
    if not file.is_file():
        raise SystemExit(f"no preset file for {verb} {mode or ''}".strip())
    return file


def section(file):
    """(stop, start, rows) from the file's `## Preset` section."""
    text = file.read_text(encoding="utf-8")
    if "\n## Preset\n" not in text:
        raise SystemExit(f"{file} has no ## Preset section")
    body = text.split("\n## Preset\n", 1)[1].split("\n## ", 1)[0]
    stop = re.search(r"^Stop: (owner|chat)$", body, re.M)
    if not stop:
        raise SystemExit(f"{file} preset needs one line `Stop: owner` or `Stop: chat`")
    start = re.search(r"^Start: (.+)$", body, re.M)
    rows = plan_chart.read_plan(file, body)[2]
    return stop[1], start[1] if start else None, rows


def of_plan(plan):
    """The preset file that the plan's `Preset:` line names, or None.

    Only the first line after the title counts, past blank lines and the chart that plan_chart.py writes
    there, so a `Preset:` line anywhere else in the plan names nothing.
    """
    lines = iter(Path(plan).read_text(encoding="utf-8").splitlines())
    if not any(l.startswith("# ") for l in lines):
        return None
    line = next(lines, "")
    while not line.strip() or line == plan_chart.START:
        if line == plan_chart.START:
            while line != plan_chart.END:
                line = next(lines, plan_chart.END)
        line = next(lines, None)
        if line is None:
            return None
    if not line.startswith("Preset: "):
        return None
    words = line.split()[1:]
    if not 1 <= len(words) <= 2:
        raise SystemExit(f"`{line}` needs a verb and at most one mode")
    return path(*words)


def same_steps(plan, file):
    """True when the plan's steps and blocks are the preset's, so the preset's stop holds for the plan."""
    ours = [(r[0], r[3]) for r in plan_chart.read_plan(plan)[2]]
    return ours == [(r[0], r[3]) for r in section(file)[2]]


def check(file):
    """Each step with its resolved blocks, or SystemExit with the first defect."""
    stop, start, rows = section(file)
    plan_chart.check(file.name, "preset", rows)
    selected = resolve_blocks(None, {r[0]: r[3] for r in rows}, "default", blocks_dir=KIT / "blocks", pack=False)
    return stop, start, [(name, need, selected[name]) for name, need, *_ in rows]


def main(argv):
    if not 1 <= len(argv) <= 2 or argv[0].startswith("-"):
        raise SystemExit(__doc__.strip().splitlines()[2].strip())
    stop, start, steps = check(path(*argv))
    print(f"Stop: {stop}" + (f"\nStart: {start}" if start else ""))
    for name, need, blocks in steps:
        print(f"{name} | needs {need} | {', '.join(blocks)}")


if __name__ == "__main__":
    main(sys.argv[1:])
