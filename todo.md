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
