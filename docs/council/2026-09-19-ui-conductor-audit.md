# Council: whole UI audit after the Conductor restyle

Date: 2026-09-19. Invoked by the owner: "get the whole ui audited by council".

## Artifact

The uncommitted `ui-bloom` worktree after the Conductor restyle: warm Conductor preset as the
default, twelve presets with their own code and terminal schemes, per-appearance colour overrides,
meaning inks moved to read on each theme (`ThemeSurfaces.readable`), sidebar navigation rows,
last-activity ages, underlined tabs, a neutral turn box, a solid composer, and noise rows hidden.
Evidence: before and after captures in dark, light, Dracula, Solarized, status grouping and
Settings, plus Conductor's own 2026 screenshots.

## Round 0

- CLAUDE asked whether the verdict covers the restyle or the whole diff, and whether presets other
  than Conductor count. Resolved by the chair from the owner's words: the whole UI and the whole
  diff ("the whole ui"); every preset counts ("very customizable"); pixel parity is judged on the
  Conductor preset only.
- GEMINI and GPT: no questions.
- Also recorded as resolved: the create window is chat only (owner: "Chat only"), and agents
  appear in the chat as read-only summaries (owner: "Summary row, read only").

## Models used

Routed through `council.claude`, `council.gpt` and `council.gemini`: Claude, GPT (`gpt-6-astra`,
high) and Gemini (`gemini-3.8-flash-high`). The first GPT seat exited at launch; the retry stopped
on Codex's folder trust prompt for the worktree; the third seat, rooted in the council directory,
ran every round.

## Live bug traced during the run

"ring failed: can't find pane: %0" on a chat that had just started. The pane was alive in Swarm's
own tmux server; the `tmux` adapter's plain `tmux` looked in the default server. The "Stopped"
badge was false because `ProcessTable.interactiveAgentProcess(ofShell:)` ignored an agent that
replaced the pane's shell. Fixed in the worktree before Round 2 (`ProcessTable.swift`,
`TerminalSessionStore.sendToAgent`, `SwarmSessionLocalChair`).

## Verdict: GO-WITH-CHANGES (unanimous, converged at round 3)

Consensus: the Conductor preset matches the reference in both appearances, and every preset's
meaning inks read. What remains are point fixes in the existing design, none a redesign. The two
that matter most are status colours that collapse into a theme's accent, and label ink that does
not follow an overridden background.

## Required changes

1. [major] `Theme.swift:323` give `positive` its own green through `readable`, apart from the
   accent; make the running-versus-positive test in `PaletteContrastTests` walk every preset.
2. [major] `Theme.swift:140-141`, `UserTurnRowView.swift:203` route label and message ink through
   `readable` for overridden grounds; add `bubble` and `selected` to
   `ThemeSurfaces.textGrounds` (`ThemeSurfaces.swift:39`).
3. [major] `NewTabMenu.swift:136-147` select the workspace after opening a terminal or browser tab
   from a swarm session, so the new tab is shown.
4. [major] `ThemeSurfaces.swift:98`, `ThemeGallery.swift:121-125` show the real inherited message
   fill and write only the edited appearance's member.
5. [major] `SwarmSessionView.swift:277-297` keep the agent report visible when the chair log fails.
6. [major] `SwarmSessionView.swift:51-97` use `TabStrip(pane: .content)` for the session header,
   keeping Resume; `:486` use `ComposerSendButton`.
7. [major] `SwarmSessionSidebarRow.swift:16-31` one line, `HomeAge.short` trailing, honour
   `SidebarRowDetail`.
8. [major] `SwarmWindowToolbar.swift:18`, `SessionTabsView.swift:101` put an unsplit pane's strip
   in the title bar; keep one strip per pane when split.
9. [minor] `ThemeGallery.swift:107` reset only the current appearance's member.
10. [minor] `TranscriptListView.swift:1011` back the pinned question with a full-width band.
11. [minor] `ThemeSurfaces.swift:79` / `TabStrip.swift:105` make "Title bar and tabs" match what
    it paints.
12. [minor] `WorkspaceHoverCard+PullRequest.swift:94` do not say "Nothing has changed" when the
    comparison failed; `Shell.swift:28` show stderr alone.
13. [minor] `SettingsView.swift:67-69` align the form to the navigation column.
14. [minor] `SidebarView.swift:979` collapse button in `.navigation` placement;
    `WorkspaceRow.swift:214-236` repo tile before the age.

## Dissent and residual risk

- GPT's terminal requirement (Solarized light ANSI white 2.84:1, bright white 1.88:1) was withdrawn
  in round 3: those slots are dim by convention and the foreground reads at 13 to 14 to 1. Recorded
  as residual risk, not a requirement.
- CLAUDE's "offer resume with the draft when the pane refuses" (its item 12) is not in the merged
  list; the other two did not take it up.
- Rejected parity ideas: a circular send button, an accent-coloured tab underline, a selection bar,
  an app-name header row, and a run dock in the inspector.
- Unverified by every seat: builds, tests and live input. The chair ran the suite (6,247 passed)
  after the ring fix.

## Per-model trail

- GEMINI: GO-WITH-CHANGES in every round; conceded the ring cause, the circle button and the accent
  underline.
- GPT: NO-GO in rounds 1 and 2; withdrew two findings that contradicted owner decisions, then the
  terminal-contrast demand; GO-WITH-CHANGES in round 3.
- CLAUDE: GO-WITH-CHANGES in every round; conceded label ink under overrides, hidden new tabs and the
  failed-log report.

## Wall time

Round 0 21:58 to 22:01 (with two GPT relaunches), Round 1 22:02 to 22:12, Round 2 22:14 to 22:19,
Round 3 22:20 to 22:22.
