# To do

## swarm spawn checks the model and reports what went wrong

`swarm spawn` (and `launch`) should check that the routed model exists for the provider and
account before it opens a pane. When it does not, or when anything else fails at spawn, the
error goes back to the orchestrator during the spawn call, with the potential fixes that might
work (another account, another model, a trust entry, a missing adapter verb). If a suggested fix
does not work, the orchestrator writes to a file that the suggestion failed, so we can later
implement the fixes that worked and drop the ones that did not.

Seen 2026-09-22: two Codex seats exited at launch with only the profile line printed, and the
chair learned nothing until it read the panes by hand.

## Real screen captures for Codex and AGY agent states

The screen check (ADR 0021) reads Codex and AGY panes with patterns taken from their source code,
not from real screens (`tests/fixtures/screens/README.md`). Research the real Codex and AGY
screens for idle, working, approval, and failure, capture them under a private tmux server, and
replace the synthesized fixtures. Also research how to confirm a state for an adapter that has no
`screen` verb; today its hook state turns to unknown after 45 s.

## Main-thread stops with 12 live panes

With 12 streaming stress panes, the Animation Hitches instrument recorded hangs of 0.27 to 1.4 s,
but no hitch over 33 ms. In the one hang traced with Time Profiler, no Swarm thread ran at all, and
the machine load was 5 to 18 on 10 cores. Find out on a quiet machine whether the app or the
system causes them (`SWARM_PANE_STRESS=12`, see ui/README.md).

