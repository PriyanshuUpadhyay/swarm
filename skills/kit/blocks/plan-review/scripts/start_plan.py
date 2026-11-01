#!/usr/bin/env python3
"""Start the step run of a plan only when its stop rule holds.

  start_plan.py <run>    <run> holds plan.md, plan.html, and the rounds in plan-review/

A plan whose `Preset:` line names a `Stop: chat` preset, and whose File, Needs, Holds, and Block rows equal
that preset's (in the table that the start reads), starts with no round. Any other plan, also one that adds
a step, a block, or a decoy table, needs the newest review round to have approved plan.html as it is now, and plan.html to carry the hash of plan.md as
it is now, so an edit to plan.md after the round stops the start. A preset with a `Start:`
line runs its own step run, so this script makes no step files for it and prints that command.
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
    """Each existing step file whose needs, writes, blocks, or brief differ from plan.md, and each one with no row.

    step_run.start skips a file that exists, so a re-approved plan would otherwise run the old file in silence.
    The check sits here, not in start, because step_amend calls start on purpose to add one file. Block ids are
    compared without their hash, and a part that the kit adds to a block later is no change, so a kit update
    alone does not refuse: only a plan block missing from the file, or a file block the plan no longer gives."""
    run = Path(run)
    if not (have := step_run.steps(run)):
        return []
    rows = step_run.table(run / "plan.md")
    uses = step_run.check_needs([r[:3] for r in rows])
    brief = step_brief.sections(run / "plan.md")
    blocks = step_run.plan_blocks(run, rows)
    found = [f"{name} (no row in plan.md)" for name in have if name not in uses]
    for name, _, _, chosen, writes, optional in rows:
        if name not in have:
            continue
        lines = (run / f"{name}.md").read_text(encoding="utf-8").split("\n")
        was = [u.split("@")[0] for u in lines[1].removeprefix("Uses: ").split(", ") if u]
        if was != uses[name]:
            found.append(f"{name} (needs: was {', '.join(was) or 'none'}, now {', '.join(uses[name]) or 'none'})")
        scope = "" if writes is None else f"Writes: {', '.join(writes) or 'none'}\n" + ("Optional: yes\n" if optional else "")
        blank = lines.index("", 2)
        head, rest = lines[2:blank], "\n".join(lines[blank + 1:])
        if "".join(f"{x}\n" for x in head) != scope:
            now = scope.strip().replace("\n", "; ").removeprefix("Writes: ") or "none"
            found.append(f"{name} (writes: was {'; '.join(head).removeprefix('Writes: ') or 'none'}, now {now})")
        elif not any(rest.startswith(brief.get(name, "") + tail) for tail in ("Skills read: ", step_run.TODO_HEAD)):
            found.append(f"{name} (brief changed)")
        ids = lambda read: [r.split("@")[0].removeprefix("block:") for r in read if r.startswith("block:")]
        old = ids(next((x.removeprefix("Skills read: ").split(", ") for x in lines if x.startswith("Skills read: ")), []))
        new = ids(blocks.get(name, ([],))[0])
        if any(b not in old for b in chosen or []) or any(b not in new for b in old):
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
        raise SystemExit(f"plan.md changed after these steps were made: {'; '.join(changed)}. "
                         f"Start the new plan in a new run folder, such as {run}-2, and name {run} in its first lines.")
    step_run.start(run, run / "plan.md")


if __name__ == "__main__":
    main(sys.argv[1:])
