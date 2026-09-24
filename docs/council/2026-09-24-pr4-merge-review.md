# Council: PR #4 pre-merge review (logic and communication)

- Artifact: https://github.com/PriyanshuUpadhyay/swarm/pull/4, branch `ui-composer` at c003de7f, merge base a9f5a6af.
- Question (user): "get the pr 4 reviewed by council."
- Round 0: skipped by the chair. The request was clear, and the PR #2 log gave the scope shape.
- Resolved constraints: Phase 2 (model picker, effort, plan, fast, goal, context meter) is out of scope. The composer must stay one module behind a small contract.
- Models: Claude `claude-fable-5-1` xhigh, GPT `gpt-6-astra` high, Gemini `gemini-3.8-flash-high` high. Every seat ran in a visible Herdr pane. No seat failed or was replaced.
- Seat limits: GPT could not reach GitHub (sandbox network) and could not run `swift test` (sandbox_apply). Claude ran `swift test`: 124 pass, 2 fail only without `SWARM_TRANSCRIPT_TOOL`.
- Converged in round 3 on the verdict. Two small items stayed 2-1.

| Round | Wall time | Result |
|---|---|---|
| 1 | 11:09 to 11:17 | all GO-WITH-CHANGES, but the required lists differed |
| 2 | 11:17 to 11:21 | four fixes agreed; cursor, account sources, trim form open |
| 3 | 11:21 to 11:23 | all GO-WITH-CHANGES; cursor and account moved to follow-up |

## Verdict: GO-WITH-CHANGES (unanimous)

The module boundary holds: ComposerView names no session, tree, pane, or detail type, lint enforces it, and
SessionDetail builds the contract at one call site. The PR #2 draft-loss follow-up is fixed. The defects are in the
view's key and send paths, which have no test target, so helper tests passed while the real path was wrong.

## Required changes before merge

1. Send the raw draft. `submit()` trims before `send` (ui/Sources/Swarm/Composer/ComposerView.swift:292, :297), but
   `ComposerSendState.finish` compares with the untrimmed draft (ui/Sources/SwarmCore/Composer/ComposerState.swift:46).
   A pick ends in a space (ComposerMenu.swift:48), so "/review " stays after a successful send, invites a second send,
   and hides Stop. Snapshot `draft.wrappedValue`, pass it, keep the exact compare. Test the "/review " round trip.
2. Stop and Command+. whenever a turn runs. Both exist only when the draft is empty (ComposerView.swift:101-122, :289).
   Show Stop whenever `isRunning`, and Send beside it when the draft has text.
3. Return sends when the completion menu has no rows. Today "ping @ops" with no match swallows Return
   (ComposerMenu.swift:86, ComposerView.swift:250-252). Put the rule in `ComposerKeyRouter`, Tab ignored, Escape dismisses. Add a test.
4. Reconcile chips with the draft. Chips never clear after a send (ComposerView.swift:23, :291-304). Drop a chip whose
   path left the draft, in the draft handler (ComposerView.swift:59-62). Do not clear all chips on success, because a
   file added during a send must keep its chip.
5. Communication. The PR body must say the `/` menu lists user-level entries from both providers' default dirs, not
   project dirs or the chair's account dir. State near ComposerView.swift:196 that completion works only at the end of the draft.

## Follow-ups (safe to merge without)

- Chair account catalog first: pick the account whose home holds `SwarmSession.chairLog` (SwarmBus.swift:90), then provider filtering and project dirs from the session cwd.
- Real caret and selection (needs an NSTextView-backed editor).
- Late image and drop callbacks can re-add a path after Escape (ComposerView.swift:328, :344, :376).
- Multiline YAML descriptions (ComposerCommands.swift:211-223); UTF-8 cut at 8,192 bytes (:198-200); fuzzy scoring on every redraw (ComposerView.swift:204-240).
- Dead composer routes in KeyRouting.swift:24-26; redundant `.id` or `.onChange(of: sessionID)`; unpruned draft keys (ComposerState.swift:61-72); unquoted paths with spaces (ComposerState.swift:11-14); "photo" icon for non-image files (ComposerView.swift:133).
- AGY and Gemini built-in commands (ComposerCommands.swift:86-116).
- PR body "126 passed" should name `SWARM_TRANSCRIPT_TOOL`.

## Dissent and residual risk

- Chip match (2-1): Claude and Gemini accept `draft.contains(path)` because pasted images have UUID names and a
  token match breaks paths with spaces. GPT wants whole-path boundaries, because /tmp/a.txt keeps its chip after an
  edit to /tmp/a.txt.bak and its remove button can then damage the new path. Decision left to the owner.
- Caret move (1-2): Claude wants the menu dismissed on arrow keys and tap, so Return after a caret move sends. GPT
  and Gemini accept a comment only. Decision left to the owner.
- Turn end depends on the transcript tool emitting `turn_ended`, and the repo has only one hand-written fixture
  (ui/Tests/TranscriptToolTests/TranscriptEventTests.swift:45). If a provider never emits it, Stop sticks; fix 2 and "text always shows Send" bound the damage.
- Sending during a run relies on the chair CLI queuing typed text. Nobody verified this on both providers.

## Per-model trail

- GEMINI: GO-WITH-CHANGES every round. Found hidden Command+. and missing project commands. Its trimmed compare and clear-on-success were refuted.
- GPT: GO-WITH-CHANGES every round. Found the trimmed-draft mismatch and the caret limit. It withdrew caret and account as merge gates in round 3 and kept path boundaries for chips.
- CLAUDE: GO-WITH-CHANGES every round. Found the empty-menu Return swallow, hidden Stop, and the account-dir source. It showed that the account needs no CLI change.
