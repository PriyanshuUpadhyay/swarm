# Council: the grouping and layout model, and a ground-up UI rebuild

Date: 2026-09-22. Invoked by the owner: "Get council to review and opine and find simplificaiton
of the system. i would much rather prefer to build it from ground up now."

## Artifact

The `ui-bloom-temp` worktree at `c8224d2d`, `ui/Sources/Swarm` and `ui/Sources/SwarmCore`. The
owner's words: a project has many chats, opened as tabs side by side inside one group, a project
can have many groups, but the app has "five types of group", and worker panes appear beside the
chat in one place and beside the title in another.

## Round 0

- GPT: no questions.
- CLAUDE and GEMINI asked where "group" lives, whether the strip may leave the title bar and
  whether worker panes are peers, and what must survive day one. GEMINI also asked whether the
  rebuild stops at `Sources/Swarm`; the chair answered that from the brief (SwarmCore stays).
- Owner answers, verbatim choices: "Workspace is the group"; "Always in-window, never title bar"
  with worker panes as peers of chat, terminal and browser; day one keeps "Restore arrangement on
  relaunch" and "Split inside a terminal tab", and the bottom panel strip need not survive.

## Models used

`council.claude` (Claude Fable 5.1, xhigh), `council.gpt` (`gpt-6-astra`, high, rooted in the
council directory to avoid the Codex trust prompt) and `council.gemini` (`gemini-3.8-flash-high`).
All three ran every round.

## Concepts today (agreed by all seats)

1. Workspace tab strip with one pane tree per tab. `WorkspaceTabsStore.swift:48,89,94`,
   drawn by `CenterPanesView.swift:15`, persisted through `StoredPaneArrangement`.
2. Tool tab list. `CenterTabStore.swift:20` owns terminal, browser, swarmAgent, review and notes
   records and the live `BrowserSession` views. It holds no tree.
3. A second split tree inside a terminal tab. `TerminalSplitStore.swift:14-19`, drawn by
   `TerminalSplitView.swift:15` with `SplitPaneDivider.swift`, which calls itself a near twin.
4. Swarm agent tab inside a workspace. `SwarmAgentTabView.swift:79-82` keeps a private
   `HSplitView` of chat and pane; `PaneSplit.canJoin` (`PaneSplit.swift:83-86`) bars it from the
   tab tree, so it can never be a peer.
5. Swarm session as its own sidebar destination. `SwarmSessionView.swift:5` with a "Panes" header
   tab (:58-75) that swaps the body for a seat grid (`SwarmSessionPaneStripView` :491,
   `SwarmPaneStripLayout` :672), and its own changes column (:222) when `workspaceID` is nil.

## Verdict: GO-WITH-CHANGES (unanimous, converged at round 2)

Consensus: rebuild the centre column, not the app. `SwarmCore` already is the target model
(`SplitLayout`, `PaneContent`, `TabSet`, `StoredPaneArrangement`, `TabDefaults`, `PaneSplit`).
The "beside the chat or beside the title" drift is one worker pane drawn by two mechanisms, the
private `HSplitView` at `SwarmAgentTabView.swift:79` and the "Panes" header tab at
`SwarmSessionView.swift:65,112`. Both go only when a worker is a leaf of the tab tree. There is no
title-bar tab strip in this checkout; `SwarmWindowToolbar.swift:18-34` holds the title and the
`+` menu only, and council 2026-09-19 change 8 is withdrawn as a placement rule.

## The convention

Project (`Repo`) > Group (`Workspace`, one per worktree) > one tab strip, always drawn inside the
window at the top of the centre column, also for a lone tab > one `SplitLayout` per tab > leaves
are `PaneContent`, `.chat(SessionID)` or `.tool(id)` of kind terminal, browser, swarmAgent, review,
notes. A worker agent is a swarmAgent leaf; its transcript is another leaf if the user splits.
A terminal leaf keeps its inner shell split (`TerminalSplitStore`). The `+` returns to the strip.

## Required changes

1. Flip `PaneSplit.canJoin` (`PaneSplit.swift:85`) so a swarmAgent tab joins the tab tree, with
   its test (`PaneSplitTests.swift:50`). The chair rules this inside the "SwarmCore stays" limit:
   one rule and its test, not a model or store change. Never copy the opposite rule into a view.
2. Show the strip always. Drop the lone-tab hide (`TabStripVisibility`, `CenterColumnView.swift:47`,
   `SessionTabsView.swift:36`) and move `NewTabMenu` from the toolbar into the strip.
3. Delete `SwarmAgentTabView`'s private `HSplitView` (:79-82) and its `DisplayMode` (:187); the
   worker pane is a leaf beside the chat.
4. Delete the seat grid: `SwarmSessionPaneStripView` (`SwarmSessionView.swift:491`),
   `SwarmPaneStripLayout` (:672), the "Panes" header tab (:58-75) and the `showsTerminal`
   branch (:112).
5. A bus session whose `cwd` matches a workspace opens that workspace and shows its seats as
   swarmAgent leaves; `AppModel+Workspaces.swift:418-420` must select `.workspace`, not
   `.swarmSession`. An unmatched session (home directory, Worktrunk hub,
   `SidebarSelection.swift:86-89`) keeps the plain chat view with no grid.
6. Keep `TerminalSplitStore` as the inner terminal split on day one. A fold into the tab tree is a
   later commit of its own, and it must re-point `TerminalPaneCensus`
   (`TerminalSplitStore.swift:24-28`) in the same commit or the orphan sweep kills live shells.
7. Keep `CenterTabStore` for tool lifetime and `WorkspaceTabsStore` for arrangement and
   selection during the change; retire the third singleton only when the new container is live.
8. Drop `TabPane.sunken` and the bottom strip in the first cut (owner: need not survive day one).
9. Before the new container replaces the old routes, a terminal with two shells must reopen with
   the same pane ids (GPT's gate). Selection was never persisted (`WorkspaceTabsStore.swift:91-95`);
   restoring it is new work, not a regression.

## Dissent and residual risk

- CLAUDE wanted the terminal fold in the rebuild; conceded to day-one retention in round 2.
- CLAUDE wanted every bus session filed under a workspace by path; conceded that nil is normal.
- GEMINI's round 1 title-bar strip claim rested on a wrong fact and was withdrawn.
- Unverified by every seat: builds, tests and live behaviour. SwiftTerm and WebKit view
  identity under a rebuilt container is GEMINI's named risk; `CenterPanesView`'s flat `ForEach`
  keyed by pane id is the existing protection and must be kept.

## Per-model trail

- GEMINI: GO-WITH-CHANGES both rounds; conceded the title-bar fact and the terminal fold.
- GPT: NO-GO in round 1 on the `canJoin` conflict and the fold; GO-WITH-CHANGES in round 2 once
  the scope was narrowed to the centre column.
- CLAUDE: GO-WITH-CHANGES both rounds; conceded the fold and the nil-workspace route.

## Wall time

Round 0 12:52 to 12:56, Round 1 12:58 to 13:05, Round 2 13:07 to 13:12. Converged at round 2.
