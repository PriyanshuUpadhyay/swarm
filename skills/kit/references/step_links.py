"""Link a step's block rules instead of copying them, and print them in full at take.

start writes `Rules from: <table file>` under `## Rules for this step`, next to the `Skills read:` line that
names each block with the hash of its file. take resolves the step's blocks again from that table, refuses
when any block or hash differs, and prints the Rules and Example text where the copy used to be. So the step file
stays short, and the worker still reads the full rules once, at take.
"""

import re
from pathlib import Path

FROM = "Rules from: "
RULES_HEAD = "## Rules for this step"


def source_line(skill_md):
    return f"{FROM}{Path(skill_md).resolve()}\n"


def link_at(lines):
    """The index of the link line, or None. Only the line right under the `## Rules for this step` head (or the
    kit-root line under it) counts, because a plan brief line above it may start with the same words."""
    head = next((i for i, line in enumerate(lines) if line == RULES_HEAD), None)
    if head is None:
        return None
    return next((i for i in range(head + 1, min(head + 3, len(lines))) if lines[i].startswith(FROM)), None)


def rules_for(folder, step, text, table, resolve, skills_head):
    """The linked rules text, or None for a file with copied rules.

    SystemExit when the table is gone, or when a block, its hash, or the block list differs from the Skills read
    line that start wrote, because the worker would follow rules that no longer match its check todos."""
    lines = text.split("\n")
    at = link_at(lines)
    if at is None:
        # start writes the link as an absolute path to a .md table, so such a line with no exact head means the head
        # was edited or lost and the worker would get no rules. A brief or a copied rule ("Rules from: the notes")
        # names no such path, and an implement step's longer head with copied rules never holds a link.
        if RULES_HEAD not in lines and any(re.fullmatch(re.escape(FROM) + r"/.*\.md", l) for l in lines):
            raise SystemExit(f"{step} links its rules but has no `{RULES_HEAD}` line; restore it or start a new run folder")
        return None
    source = Path(lines[at].removeprefix(FROM))
    if not source.is_file():
        raise SystemExit(f"{step} links its rules from {source}, which is gone; restore it or start a new run folder")
    have = next((l.removeprefix(skills_head).split(", ") for l in lines if l.startswith(skills_head)), [])
    read, rules, _ = resolve(folder, table(source)).get(step, ([], "", []))
    if read != have:
        changed = sorted(set(have) ^ set(read)) or ["the block order"]
        raise SystemExit(f"{step} is not ready: its blocks changed after start ({', '.join(changed)}); "
                         "start the plan in a new run folder")
    return rules.rstrip("\n")


def fill(text, rules):
    """The step text with the rules inserted under the link line."""
    lines = text.split("\n")
    at = link_at(lines)
    if rules is None or at is None:
        return text
    return "\n".join(lines[:at + 1] + [rules] + lines[at + 1:])


def expand(folder, step, text, table, resolve, skills_head):
    """The step text with its linked rules filled in, or the text as it is for a file with copied rules."""
    return fill(text, rules_for(folder, step, text, table, resolve, skills_head))


def unique_judges(todos):
    """The todos with each judge question once; a script todo stays, as done needs one per block check."""
    seen, out = set(), []
    for item in todos:
        key = item.split(" judge: ", 1)[1] if item.startswith("block ") and " judge: " in item else None
        if key is not None and key in seen:
            continue
        seen.add(key)
        out.append(item)
    return out
