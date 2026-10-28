"""The step briefs of an approved plan.md, which step_run.py start copies into each step file."""

import re
from pathlib import Path


def briefs(text):
    """{step: brief} from the `### N · name` sections under `## Steps` that hold a `Step: NN-name` line. A brief is its
    heading and lines up to the next `### ` or `## ` heading outside a fence, without the Step line."""
    found, inside, fence, head, body = {}, False, False, None, []

    def close():
        if head is not None and (step := next((l for l in body if re.fullmatch(r"Step: `?\d\d-[a-z0-9-]+`?", l)), None)):
            lines = [head] + [l for l in body if l != step]
            found[step[6:].strip("`")] = re.sub(r"\n{3,}", "\n\n", "\n".join(lines)).strip() + "\n"

    for line in text.splitlines():
        if line.startswith("```"):
            fence = not fence
        if not fence and line.startswith(("## ", "### ")):
            close()
            head, body = None, []
            if line.startswith("## "):
                inside = line.strip() == "## Steps"
            elif inside:
                head = line
            continue
        if head is not None:
            body.append(line)
    close()
    return found


def sections(plan):
    """{step: the brief section of its step file}: the brief the owner approved in plan.md, so the
    worker does the work they saw."""
    return {name: f"## Brief (approved in plan.md)\n{text}\n" for name, text in briefs(Path(plan).read_text(encoding="utf-8")).items()}
