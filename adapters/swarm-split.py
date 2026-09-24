#!/usr/bin/env python3

"""Split one new swarm child pane straight into the main-grid shape.

The orchestrator keeps the left half and the children stack in the right column, so the
layout never needs a rebuild afterwards. A rebuild has to park panes in a scratch tab,
because Herdr refuses a pane move inside one tab, and that tab blinks in the tab bar.

The swarm herdr adapter calls this for its `spawn` verb and reads the pane id from stdout.
"""

import json
import os
import subprocess
import sys

FORWARDED = ("SWARM_SESSION_ID", "SWARM_AGENT_ID", "SWARM_HOME", "SWARM_ADAPTER")
# Display-only labels: the sidebar rows render `$tree` and `$seat` per pane and only the
# spawner knows which panes are children and which seat each one holds. `$seat` is the swarm
# agent id, which a Codex or AGY pane never shows, because its title stays the launch command
# line. `SWARM_PANE_TREE` picks another marker; an empty value turns that tag off.
TOKEN_SOURCE = "swarm-split"
TREE_MARKER = os.environ.get("SWARM_PANE_TREE", "↳")


def herdr(*args):
    result = subprocess.run(
        ["herdr", *args], capture_output=True, text=True, check=False
    )
    if result.returncode != 0:
        raise RuntimeError(
            result.stderr.strip() or result.stdout.strip() or f"herdr {args[0]} failed"
        )
    return json.loads(result.stdout) if result.stdout.strip() else None


def layout_of(pane):
    return herdr("pane", "layout", "--pane", pane)["result"]["layout"]


def children_of(layout, orchestrator):
    """Panes right of the orchestrator, top to bottom."""
    left = next(
        pane for pane in layout["panes"] if pane["pane_id"] == orchestrator
    )["rect"]["x"]
    return sorted(
        (pane for pane in layout["panes"] if pane["rect"]["x"] > left),
        key=lambda pane: pane["rect"]["y"],
    )


def equalise_steps(children):
    """One (pane id, ratio delta) per split, top to bottom, so every child ends the same
    height. A split's ratio is its top pane's share of that split's own rect, which is
    scale free, so moving one split leaves the ratios below it alone and every delta can
    come from a single layout read.
    """
    heights = [pane["rect"]["height"] for pane in children]
    steps = []
    for index in range(len(children) - 1):
        share = heights[index] / sum(heights[index:])
        steps.append((index, 1 / (len(children) - index) - share))
    return steps


def equalise(children):
    for index, delta in equalise_steps(children):
        if abs(delta) < 0.01:
            continue
        # Each direction addresses the border on that side of the named pane, and the
        # named pane always grows: `down` on the pane above the split, `up` on the one
        # below it.
        pane, direction = (
            (children[index], "down") if delta > 0 else (children[index + 1], "up")
        )
        herdr(
            "pane", "resize", "--pane", pane["pane_id"],
            "--direction", direction, "--amount", f"{abs(delta):.4f}",
        )


def tag_options():
    """`--token` options for the child pane. Herdr cuts a value at 80 characters and clears a
    key whose value is empty, so an absent value is left out instead of published blank.
    """
    tokens = {"tree": TREE_MARKER, "seat": os.environ.get("SWARM_AGENT_ID", "")}
    options = []
    for name, value in tokens.items():
        if value:
            options += ["--token", f"{name}={value[:80]}"]
    return options


def tag(pane):
    """Best-effort: a missing token only costs the sidebar its label."""
    options = tag_options()
    if not options:
        return
    try:
        # The parser needs the pane id before the options; the reverse order is rejected.
        herdr("pane", "report-metadata", pane, "--source", TOKEN_SOURCE, *options)
    except (OSError, RuntimeError, ValueError) as error:
        print(f"swarm-split: pane tags skipped: {error}", file=sys.stderr)


def spawn(orchestrator, cwd):
    children = children_of(layout_of(orchestrator), orchestrator)
    target, direction = (
        (children[-1]["pane_id"], "down") if children else (orchestrator, "right")
    )
    forwarded = []
    for name in FORWARDED:
        forwarded += ["--env", f"{name}={os.environ.get(name, '')}"]
    pane = herdr(
        "pane", "split", target, "--cwd", cwd, "--direction", direction,
        "--ratio", "0.5", "--no-focus", *forwarded,
    )["result"]["pane"]["pane_id"]
    tag(pane)
    equalise(children_of(layout_of(orchestrator), orchestrator))
    return pane


def self_check():
    pane = lambda name, height: {"pane_id": name, "rect": {"height": height}}
    assert equalise_steps([pane("a", 40)]) == []
    two = equalise_steps([pane("a", 40), pane("b", 40)])
    assert [index for index, _ in two] == [0] and abs(two[0][1]) < 1e-9
    three = equalise_steps([pane("a", 40), pane("b", 20), pane("c", 20)])
    assert abs(three[0][1] - (1 / 3 - 0.5)) < 1e-9, three
    assert abs(three[1][1]) < 1e-9, three
    grown = equalise_steps([pane("a", 20), pane("b", 30), pane("c", 30)])
    assert abs(grown[0][1] - (1 / 3 - 0.25)) < 1e-9, grown
    os.environ["SWARM_AGENT_ID"] = "ws-claude-run1"
    assert tag_options() == [
        "--token", f"tree={TREE_MARKER}", "--token", "seat=ws-claude-run1"
    ], tag_options()
    os.environ["SWARM_AGENT_ID"] = "s" * 100
    assert tag_options()[-1] == "seat=" + "s" * 80, tag_options()
    del os.environ["SWARM_AGENT_ID"]
    assert tag_options() == ["--token", f"tree={TREE_MARKER}"], tag_options()
    print("swarm-split: self-check ok")


def main():
    if "--self-check" in sys.argv[1:]:
        self_check()
        return 0
    orchestrator = os.environ.get("HERDR_PANE_ID")
    if not orchestrator:
        print("swarm-split: HERDR_PANE_ID is not set", file=sys.stderr)
        return 2
    try:
        print(spawn(orchestrator, os.getcwd()))
    except (OSError, RuntimeError, ValueError, KeyError, StopIteration) as error:
        print(f"swarm-split: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
