"""The step briefs of an approved plan.md, which step_run.py start copies into each step file."""

import re
from pathlib import Path


def briefs(text):
    """{step: brief} from the `### N · name` sections under `## Steps` that hold a `Step: NN-name` line. A brief is its
    heading and lines up to the next `### ` or `## ` heading outside a fence, without the Step line."""
    found, inside, fence, head, body = {}, False, "", None, []

    def close():
        if head is not None and (step := next((l for l in body if re.fullmatch(r"Step: `?\d\d-[a-z0-9-]+`?", l)), None)):
            lines = [head] + [l for l in body if l != step]
            name = step[6:].strip("`")
            if name in found:
                raise SystemExit(f"two briefs under ## Steps have the line Step: {name}; keep one")
            if any(l.startswith(("## ", "Skills read: ")) for l in body):
                # The step file reads these lines as its own: `## ` as a section such as the todo list, and the first
                # `Skills read: ` line as the blocks the worker read.
                raise SystemExit(f"the brief of {name} has a line that starts with `## ` or `Skills read: `; indent it")
            found[name] = re.sub(r"\n{3,}", "\n\n", "\n".join(lines)).strip() + "\n"

    for line in text.splitlines():
        # A fence ends only at a line of at least as many backticks, so a ```` fence may hold a ``` one.
        if m := re.match(r"`{3,}", line):
            fence = m[0] if not fence else "" if line.strip() == "`" * len(line.strip()) and len(m[0]) >= len(fence) else fence
        if not fence and line.startswith(("## ", "### ")):
            close()
            head, body = None, []
            if line.startswith("## "):
                inside = line.strip() == "## Steps"
            elif inside:
                head = line
            continue
        if head is not None and (fence or not line.startswith("<!--")):
            body.append(line)  # the page does not show a `<!--` line outside a fence, so the worker does not get it
    close()
    return found


def sections(plan):
    """{step: the brief section of its step file}: the brief the owner approved in plan.md, so the
    worker does the work they saw."""
    return {name: f"## Brief (approved in plan.md)\n{text}\n" for name, text in briefs(Path(plan).read_text(encoding="utf-8")).items()}
