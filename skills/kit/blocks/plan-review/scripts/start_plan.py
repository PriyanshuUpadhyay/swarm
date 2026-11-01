#!/usr/bin/env python3
"""Start the step run of a plan only when its stop rule holds.

  start_plan.py <run>    <run> holds plan.md, plan.html, and the rounds in plan-review/

A plan whose `Preset:` line names a `Stop: chat` preset, and whose File, Needs, Holds, and Block rows equal
that preset's (in the table that the start reads), starts with no round. Any other plan, also one that adds
a step, a block, or a decoy table, needs the newest review round to have approved plan.html as it is now, and plan.html to carry the hash of plan.md as
it is now, so an edit to plan.md after the round stops the start. A preset with a `Start:`
line runs its own step run, so this script makes no step files for it and prints that command.

A run whose step files exist starts again only when each file still matches plan.md (see drift): the same
needs, holds, Writes and Optional lines, brief, and blocks, and no file without a row. A plan that adds a step
only makes the new file. Any other change refuses and asks for a new run folder that names this one.
"""

import hashlib
import re
import sys
from pathlib import Path

KIT = Path(__file__).resolve().parents[3]
sys.path[:0] = [str(KIT / "references"), str(KIT / "blocks" / "comment-round" / "scripts"),
                str(KIT / "skills" / "swarm" / "scripts")]  # python3 -I adds none of them
import comment_round  # noqa: E402
import preset  # noqa: E402
import step_brief  # noqa: E402
import step_run  # noqa: E402


def approved(run):
    """The preset's Start command or None, after the stop rule holds; else SystemExit."""
    run = Path(run)
    file = preset.of_plan(run / "plan.md")
    stop, start, _ = preset.section(file) if file else ("owner", None, [])
    if stop == "owner" or not preset.same_steps(run / "plan.md", file):
        comment_round.check(run / "plan-review", [str(run / "plan.html")])
        # The page carries the hash of the plan.md it was drawn from, so a kit update that redraws it differently
        # does not count as an edit, and an edit to plan.md after the round does.
        drawn = re.search(r'<meta name="plan-md-sha256" content="(\w+)">', (run / "plan.html").read_text(encoding="utf-8"))
        if not drawn or drawn[1] != hashlib.sha256((run / "plan.md").read_text(encoding="utf-8").encode("utf-8")).hexdigest():
            raise SystemExit("plan.md changed after the owner approved plan.html; run plan-chart and start a new round")
    return start


def drift(run):
    """Each existing step file whose needs, holds, writes, blocks, or brief differ from plan.md, and each one with no row.

    step_run.start skips a file that exists, so a re-approved plan would otherwise run the old file in silence.
    The check sits here, not in start, because step_amend calls start on purpose to add one file. Block ids are
    compared without their hash, as start resolves them for this repo, so a repo `[off]` block is off here too.
    A kit update alone does not refuse: a part that the kit adds, and a file block that the kit no longer has,
    are no change. Only a plan block or a repo `[add]` block missing from the file, or a file block that the kit
    still has and the plan no longer gives, refuses."""
    run = Path(run)
    if not (have := step_run.steps(run)):
        return []
    rows = step_run.table(run / "plan.md")
    uses = step_run.check_needs([r[:3] for r in rows])
    brief = step_brief.sections(run / "plan.md")
    # A resolver error (a block the kit no longer has, a broken .blocks.toml, Python before 3.11) stops the start
    # with its own reason, because without the resolve a changed or dropped block cannot be seen.
    try:
        new_ids = {name: [r.split("@")[0].removeprefix("block:") for r in read if r.startswith("block:")]
                   for name, (read, _, _) in step_run.resolve_start_blocks(run, rows).items()}
    except SystemExit as error:  # its reasons can quote keys from a cloned .blocks.toml
        raise SystemExit(step_run.plain(str(error.code))) from None
    found = [f"{name} (no row in plan.md)" for name in have if name not in uses]
    for name, _, holds, chosen, writes, optional in rows:
        if name not in have:
            continue
        lines = (run / f"{name}.md").read_text(encoding="utf-8").split("\n")
        if len(lines) < 3 or "" not in lines[2:]:
            raise SystemExit(step_run.plain(f"{run / name}.md is not a step file (no Uses: line and blank line below it); "
                                            "restore it or start a new run folder"))
        if (was := step_run.needs(run, name)) != uses[name]:
            found.append(f"{name} (needs: was {', '.join(was) or 'none'}, now {', '.join(uses[name]) or 'none'})")
        blank = lines.index("", 2)
        head, rest = lines[2:blank], "\n".join(lines[blank + 1:])
        if "".join(f"{x}\n" for x in head) != (scope := step_run.scope_lines(writes, optional)):
            now = scope.strip().replace("\n", "; ").removeprefix("Writes: ") or "none"
            found.append(f"{name} (writes: was {'; '.join(head).removeprefix('Writes: ') or 'none'}, now {now})")
        elif not any(rest.startswith(brief.get(name, "") + tail) for tail in (step_run.SKILLS_HEAD, step_run.TODO_HEAD)):
            found.append(f"{name} (brief changed)")
        todo = lines.index(step_run.TODO_HEAD) if step_run.TODO_HEAD in lines else -1
        first = lines[todo + 1] if 0 <= todo < len(lines) - 1 else ""
        if not first.startswith((f"- [ ] {holds}:", f"- [x] {holds}:")):  # start writes Holds as the first todo
            found.append(f"{name} (holds: now {holds})")
        read = next((x.removeprefix(step_run.SKILLS_HEAD).split(", ") for x in lines if x.startswith(step_run.SKILLS_HEAD)), [])
        old = [r.split("@")[0].removeprefix("block:") for r in read if r.startswith("block:")]
        new = list(new_ids.get(name, []))
        # A new id that is a plan block, or no kit block (so a repo `[add]` gave it), refuses; a new kit part does not.
        if any(b not in old and (b in (chosen or []) or not (KIT / "blocks" / b / "SKILL.md").is_file()) for b in new) or \
                any(b not in new and (KIT / "blocks" / b / "SKILL.md").is_file() for b in old):
            found.append(f"{name} (blocks: was {', '.join(old) or 'none'}, now {', '.join(new) or 'none'})")
    return found


def main(argv):
    if len(argv) != 1:
        raise SystemExit(__doc__.strip().splitlines()[2].strip())
    run = Path(argv[0])
    if start := approved(run):
        print(f"start with: {start}")
        return
    if changed := drift(run):
        # A repo's .blocks.toml can turn a block on or off too, which changes a step's rules as a plan edit does.
        raise SystemExit(f"plan.md or this repo's .blocks.toml changed after these steps were made: {step_run.plain('; '.join(changed))}. "
                         f"Start the new plan in a new run folder, such as {run}-2, and name {run} in its first lines.")
    step_run.start(run, run / "plan.md")


if __name__ == "__main__":
    main(sys.argv[1:])
