"""Link a step's block rules instead of copying them, and print them in full at take.

start writes `Rules from: <table file>` under `## Rules for this step`, next to the `Skills read:` line that
names each block with the hash of its file. take resolves the step's blocks again from that table, refuses
when any block or hash differs, and prints the Rules and Example text where the copy used to be. In trial 5,
copied rules made the k3 step files 1,752 lines long, and a worker said reading them "took more time than the
research"; the worker still gets the full text, from the one take it runs.
"""

from pathlib import Path

FROM = "Rules from: "


def source_line(skill_md):
    return f"{FROM}{Path(skill_md).resolve()}\n"


def expand(folder, step, text, table, resolve, skills_head):
    """The step text with its linked rules filled in, or the text as it is for a file with copied rules.

    SystemExit when the table is gone, or when a block, its hash, or the block list differs from the Skills read
    line that start wrote, because the worker would follow rules that no longer match its check todos."""
    lines = text.split("\n")
    at = next((i for i, line in enumerate(lines) if line.startswith(FROM)), None)
    if at is None:
        return text
    source = Path(lines[at].removeprefix(FROM))
    if not source.is_file():
        raise SystemExit(f"{step} links its rules from {source}, which is gone; restore it or start a new run folder")
    have = next((l.removeprefix(skills_head).split(", ") for l in lines if l.startswith(skills_head)), [])
    read, rules, _ = resolve(folder, table(source)).get(step, ([], "", []))
    if read != have:
        changed = sorted(set(have) ^ set(read)) or ["the block order"]
        raise SystemExit(f"{step} is not ready: its blocks changed after start ({', '.join(changed)}); "
                         "start the plan in a new run folder")
    return "\n".join(lines[:at + 1] + [rules.rstrip("\n")] + lines[at + 1:])


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
