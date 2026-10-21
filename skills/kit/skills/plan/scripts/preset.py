#!/usr/bin/env python3
"""Check a verb's preset and print each step with the blocks it resolves to.

  preset.py <verb> [<mode>]    check skills/<verb>/SKILL.md, or skills/<verb>/references/<mode>.md, and print its steps
  preset.py --json             print every block and preset as JSON, with each step's stage and resolved blocks
  preset.py --check            check a draft SKILL.md or `## Preset` section on stdin; print {"ok", "preset" or "defect"}

A preset is a `## Preset` section with one `Stop: owner` or `Stop: chat` line, an optional
`Start: <command>` line, and one table in the plan step-table shape (| File | Needs | Holds | Block | Why |).
"""

import json
import os
import re
import sys
from pathlib import Path

KIT = Path(__file__).resolve().parents[3]
sys.path[:0] = [str(KIT / "references"), str(KIT / "blocks" / "plan-chart" / "scripts")]  # python3 -I adds neither
import plan_chart  # noqa: E402
import step_run  # noqa: E402
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


def section(file, text=None):
    """(stop, start, rows) from the file's `## Preset` section; `text` replaces the file's text."""
    text = file.read_text(encoding="utf-8") if text is None else text
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
    """True when every row that the start writes is the preset's own row, so the preset's stop holds for the plan.

    step_run.start makes the steps from the first | File | Needs | Holds | table, so that table must equal the
    preset's File, Needs, and Holds, and the plan's step table must also equal its blocks. Why stays free.
    """
    theirs = section(file)[2]
    written = [r[:3] for r in step_run.table(plan)]
    drawn = [(r[0], r[1], r[2], r[3]) for r in plan_chart.read_plan(plan)[2]]
    return (written == [(n, need, holds.strip("`")) for n, need, holds, *_ in theirs]
            and drawn == [r[:4] for r in theirs])


def check(file):
    """Each step with its resolved blocks, or SystemExit with the first defect."""
    stop, start, rows = section(file)
    plan_chart.check(file.name, "preset", rows)
    selected = resolve_blocks(None, {r[0]: r[3] for r in rows}, "default", blocks_dir=KIT / "blocks", pack=False)
    return stop, start, [(name, need, selected[name]) for name, need, *_ in rows]


def facts(file, verb, mode=None, text=None):
    """One preset as the Skills page draws it: each step's cells as written, its uses, stage, and blocks."""
    stop, start, rows = section(file, text)
    uses = plan_chart.check(file.name, "preset", rows)
    stage = {n: i for i, names in enumerate(plan_chart.stages(uses)) for n in names}
    selected = resolve_blocks(None, {r[0]: r[3] for r in rows}, "default", blocks_dir=KIT / "blocks", pack=False)
    body = (file.read_text(encoding="utf-8") if text is None else text).split("\n## Preset\n", 1)[1]
    lines = body.split("\n## ", 1)[0].splitlines()
    head = next(i for i, line in enumerate(lines) if plan_chart.HEAD.match(line))
    raw = []
    for line in lines[head + 1:]:  # the lines that read_plan takes as rows
        if line.startswith("|---") or plan_chart.HEAD.match(line):
            continue
        if not line.startswith("|"):
            break
        raw.append([c.strip() for c in line.strip().strip("|").split("|")])
    # `cells` is each row as written, so a copy of a preset keeps its exact table text.
    steps = [{"file": name, "needs": need, "holds": holds, "why": why, "cells": cells, "blocks": blocks,
              "resolved": selected[name], "uses": uses[name], "stage": stage[name]}
             for (name, need, holds, blocks, why, *_), cells in zip(rows, raw, strict=True)]
    return {"preset": f"{verb} {mode}" if mode else verb, "verb": verb, "mode": mode, "stop": stop, "start": start,
            "steps": steps}


def meta(path):
    """{key: value} of a block's front matter: description, and kind, needs, gives under metadata."""
    text = path.read_text(encoding="utf-8")
    head = text[3:].partition("\n---\n")[0] if text.startswith("---\n") else ""
    found = {}
    for key, value in re.findall(r"^(description|  kind|  needs|  gives): (.*)$", head, re.M):
        value = value.strip()
        found[key.strip()] = json.loads(value) if value.startswith('"') else value
    return found


def kit_json():
    """Every block with the presets that use it, and every preset with its step facts."""
    files = {p.parent.name: p for p in sorted((KIT / "blocks").glob("*/SKILL.md"))}
    parents = plan_chart.parents()
    parent_of = {part: parent for parent, parts in parents.items() for part in parts}
    presets = []
    for file in sorted((KIT / "skills").glob("*/SKILL.md")) + sorted((KIT / "skills").glob("*/references/*.md")):
        if "\n## Preset\n" in file.read_text(encoding="utf-8"):
            verb, mode = (file.parent.name, None) if file.name == "SKILL.md" else (file.parents[1].name, file.stem)
            presets.append(facts(file, verb, mode))
    used = {}
    for p in presets:
        for step in p["steps"]:
            for block in step["resolved"]:
                used.setdefault(block, []).append({"preset": p["preset"], "step": step["file"],
                                                   "named": block in step["blocks"]})
    blocks = [{"id": name, "kind": m.get("kind", ""), "description": m.get("description", ""), "needs": m.get("needs", ""),
               "gives": m.get("gives", ""), "parent": parent_of.get(name), "parts": parents.get(name, []),
               "used_by": used.get(name, [])}
              for name, m in ((name, meta(path)) for name, path in files.items())]
    version = json.loads((KIT / "plugin.json").read_text(encoding="utf-8")).get("version", "")
    return {"schema": "swarm.kit/1", "version": version, "blocks": blocks, "presets": presets}


def front_matter_defect(text):
    """The first defect in a new skill's `name` and `description`, or None for a draft with no front matter."""
    if not text.startswith("---\n"):
        return None
    found = dict(re.findall(r"^(name|description): ?(.*)$", text[4:].partition("\n---\n")[0], re.M))
    if not re.fullmatch(r"[a-z0-9-]+", found.get("name", "")):
        return "name needs lowercase letters, digits, and dashes"
    written = found.get("description", "").strip()
    quoted = written[:1] in ("'", '"')
    try:
        description = json.loads(written) if written.startswith('"') else written
    except ValueError:
        return "description is not a valid quoted string"
    if written.startswith("'"):
        if len(written) < 2 or not written.endswith("'") or "'" in written[1:-1].replace("''", ""):
            return "description is not a valid quoted string"
        description = written[1:-1].replace("''", "'")
    if not description.strip():
        return "description needs a `Use when` line"
    if not quoted and (": " in description or " #" in description or description[0] in "!&*[]{}|>'%@`,?-:#\""):
        return "description has YAML punctuation, so it needs double quotes"
    return None


def draft(text):
    """{"ok": true, "preset"} for a draft that passes every preset check, else {"ok": false, "defect"}."""
    if defect := front_matter_defect(text):
        return {"ok": False, "defect": defect}
    text = text if "\n## Preset\n" in text else "\n" + text
    try:
        return {"ok": True, "preset": facts(Path("draft"), "draft", text=text)}
    except SystemExit as stop:
        return {"ok": False, "defect": str(stop.code)}


def main(argv):
    if argv in (["--json"], ["--check"]):
        # Only --json and --check stop here; an import of this module (start_plan.py, plan_chart.py) runs on older Python.
        if sys.version_info < (3, 11):
            raise SystemExit("swarm kit: needs Python 3.11 or newer")
        if argv == ["--check"] and sys.stdin.isatty():
            raise SystemExit("preset.py --check reads a draft on stdin; pipe one in")
        try:
            print(json.dumps(kit_json() if argv == ["--json"] else draft(sys.stdin.buffer.read().decode("utf-8"))))
            sys.stdout.flush()
        except BrokenPipeError:
            fd = os.open(os.devnull, os.O_WRONLY)
            # Python flushes stdout at exit; devnull prevents another error from the closed pipe.
            os.dup2(fd, sys.stdout.fileno())
            os.close(fd)
        return None
    if not 1 <= len(argv) <= 2 or argv[0].startswith("-"):
        raise SystemExit(__doc__.strip().splitlines()[2].strip())
    stop, start, steps = check(path(*argv))
    print(f"Stop: {stop}" + (f"\nStart: {start}" if start else ""))
    for name, need, blocks in steps:
        print(f"{name} | needs {need} | {', '.join(blocks)}")


if __name__ == "__main__":
    main(sys.argv[1:])
