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
                str(KIT / "skills" / "plan" / "scripts")]  # python3 -I adds none of them
import comment_round  # noqa: E402
import preset  # noqa: E402
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


def main(argv):
    if len(argv) != 1:
        raise SystemExit(__doc__.strip().splitlines()[2].strip())
    run = Path(argv[0])
    if start := approved(run):
        print(f"start with: {start}")
        return
    step_run.start(run, run / "plan.md")


if __name__ == "__main__":
    main(sys.argv[1:])
