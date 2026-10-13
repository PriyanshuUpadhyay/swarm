#!/usr/bin/env python3
"""Start the step run of a plan only when the newest review round approved plan.md and plan.html as they are now.

  start_plan.py <run>    <run> holds plan.md, plan.html, and the rounds in plan-review/
"""

import sys
from pathlib import Path

KIT = Path(__file__).resolve().parents[3]
sys.path[:0] = [str(KIT / "references"), str(KIT / "blocks" / "comment-round" / "scripts")]  # python3 -I adds neither
import comment_round  # noqa: E402
import step_run  # noqa: E402


def main(argv):
    if len(argv) != 1:
        raise SystemExit(__doc__.strip().splitlines()[2].strip())
    run = Path(argv[0])
    comment_round.check(run / "plan-review", [str(run / "plan.html"), str(run / "plan.md")])
    step_run.start(run, run / "plan.md")


if __name__ == "__main__":
    main(sys.argv[1:])
