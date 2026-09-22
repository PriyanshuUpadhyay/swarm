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

## swarm send right after spawn fails once with a foreign key error

Seen 2026-09-22: `swarm send <seat> ask` a second after `swarm spawn` returned
"FOREIGN KEY constraint failed" for two fresh seats; the same send a few seconds later worked.
Either spawn returns before the agent row is committed, or the send opens the database before
the spawn's write lands. Make spawn return only when the agent row is visible to a new connection.

## The window frame does not come back after quit

Seen 2026-09-22 after U8: the selected session, the expanded rows and the width came back, but
the window reopened at 1257x450 in the screen centre after it had been 1800x1130 at the top left.
The frame autosave does not record a frame set through accessibility; check whether a frame set
by hand is saved, and save the frame explicitly at quit if not.

## The transcript scrolls under the title bar

Seen 2026-09-22 after U9: the first transcript row sits behind the window title when the list is
scrolled to the top. The ScrollView ignores the toolbar's safe area; give it the top inset.
