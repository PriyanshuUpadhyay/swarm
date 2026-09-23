# Council: drop the Bloom fork and rebuild the UI on main, or keep and cut? (2026-09-22)

Invoked by the owner: "by adopting a pre-existing solution we are modifying it to the extent that
it is not the same product anymore ... drop whatever we have implemented here and take whatever is
present in main and build on top of it so that we own all the decisions ... maybe can extract out
some of the helpers we might need. get council to opine".

## Artifact

`main` at aba6ee60 (Rust `swarm` CLI, adapters, `packages/transcript`, no UI) and `ui-bloom-temp`
at 23453b1a (main + 120 commits: the squashed spatie/bloom subtree in `ui/`, 1,357 Swift files,
231,876 lines, plus 19 Rust/SQL bus commits, ADRs 0001-0013 and four UI council logs). Prior on
the same question: `2026-09-17-swarm-ui-approach.md` (keep-and-cut, gated on a two-day trial).

Options put to the seats: A keep-and-cut (status quo); B rebuild on main, lifting helpers from
the fork; C new shell over the whole SwarmCore.

## Round 0

- GPT and GEMINI: NO QUESTIONS.
- CLAUDE asked three. Owner's answers, verbatim: target for "today's feature level" is "Swarm chat
  + pane + sessions tree only"; dark period "No, a dark period is fine"; the 2026-09-17 trial was
  "Never run as a trial".
- Chair-resolved: the fork's 19 Rust/SQL commits (+1743/-121 vs main) are swarm's own code, kept
  under every option and landed on main first.

## Models used

`council.claude` = claude-fable-5-1 xhigh (Claude Code, pane wE7:p2); `council.gpt` = gpt-6-astra
high (Codex, wE7:p3); `council.gemini` = gemini-3.8-flash-high (AGY, wE7:p4). Bus session 71.
All three seats ran every round; no fallback.

## Verdict: GO-WITH-CHANGES, option B (unanimous, converged at round 2)

Consensus reasoning. The owner's narrowed target removes the reason the 09-17 council kept the
fork. Chat, live pane and sessions tree are about 25 swarm-specific files (6,076 lines) plus
about 2.7k lines of tmux, process and SwiftTerm helpers inside a 231k-line tree that otherwise
serves excluded features. The rule that decides what the tree shows needs a Bloom `Store` row
(`ui/Sources/SwarmCore/Agent/SwarmSessionListing.swift:294`,
`ui/Sources/Swarm/State/AppModel+SwarmSessions.swift:86`), so C would move the base one layer
down, not remove it. Both swarm chat views still build Bloom's `TranscriptModel`
(`InteractiveChatView.swift:288-289`, `SwarmSessionView.swift:145`), and none of the 09-17 cut
items (runner, queue, browser, Preview, Code, Oceans) has landed in five days.

Required changes (all accepted by all three seats):
1. Land the fork's Rust and SQL bus commits on main before any UI work.
2. Extract by behavior with a dependency audit per file, from a written candidate list, as one
   compile-only spike commit on day one. Split `SwarmBus.swift` at lift (records and protocol
   `:1-160,318-358`; not the chair launch plan `:168-243`). Lift the two process-boundary rules
   from `TerminalSessionStore` (views made once keyed by pane id `:5-10`; quit detaches the tmux
   client `:960-988`), not the store. Never run the old orphan sweep (`:876-909`) from the new app.
3. Cut the renderer at `TranscriptTool.TranscriptEvent` (`TranscriptEvent.swift:184-201`). Drop
   `TranscriptModel`, `TranscriptEventRows`, `TranscriptMapping`, `OpaqueRecord`, the `Message`
   envelope and `SubagentTranscript`. Keep the chunk-assembly behavior and fixtures as tests
   (`TranscriptEventRowsTests.swift:216-229`), and the history limit from
   `ToolTranscriptReader.swift:136-176`. Package the Zig binary as `ui/Tools/build.sh:50-77` does.
4. Data model from the bus: project = repository path, worktree = `git worktree list`
   (`WorktreeListing.swift`, `GitRepositoryPaths.swift:11-30`, `Git+Worktrees.swift:16-18`),
   session = a bus row matched by `cwd`. No `Store`, no migrations, no deliveries.
5. `SwarmBusStore` (direct SQLite read) is out of the day-one spike; the app polls the CLI. It
   comes in before the gate as slice two, with `SQLite.swift` and `WorktreeWatcher.swift`, unless
   the measured poll cost no longer applies (`SwarmBusStore.swift:7-12`).
6. Day one answers CLI prompts in the live pane; the permission-card path is a later slice, and
   the copied launch code must stop installing the file-answer hook
   (`AgentKind+Interactive.swift:55-61,95-101,180-184`).
7. Keep `ui/` on the branch as a quarry until the gate passes; retire it in a later commit. Every
   Bloom-origin file lifted keeps the MIT notice.
8. Gate before "done": the 09-21 smoke assertions 2-7 (`2026-09-21-chat-setup-drift.md:69-77`)
   plus the never-run 09-17 trial (one app chat and one CLI-started session in one tree, Escape
   reaching only the selected pane, quit and reopen with both rows live and readable), driven
   through the real pane, not the library.

Helpers to extract (core, no UI import): `SwarmBus` (split), `SwarmCLIBus`, `SwarmSessionListing`,
`SwarmAgentConversation`, `SwarmLaunchChoice`, `SwarmProfiles`, `SwarmCLIProfileSource`,
`AgentKind+Interactive`, `InteractiveHookEvent`; `TranscriptTool/*`; the reader's log discovery
(`InteractiveChatTranscript.swift:83-118,216-229,291-323`); `TmuxSessions`, `ProcessTable`,
`Shell`, `LoginShell`, `ChildProcessEnvironment`; `SplitLayout`; the git identity and worktree
helpers named above; and their tests. App target: `TerminalView` (`SwarmTerminalView` `:105`,
colours `:326-330`), `SwarmAgentPaneTerminalView`, `SwarmAgentPaneView`,
`SwarmAgentPaneReattachStrip`, `SwarmAgentPaneState`; `SwarmAgentWorkspaceModel`,
`SwarmSessionReaderModel`, `AppModel+SwarmSessions` re-homed on a project list; `Palette`, `Typo`,
`Metrics` and the conductor preset only. Not lifted: `TerminalGhostty`, the other eleven presets,
Store, Workspace, Git add/remove, GitHub, Bridge, Delivery, AgentRunner, Codex and Grok runners,
Views/Transcript, Inspector, Browser*, Oceans, Code, Preview, Markdown, Ask, Home, RepoSettings,
CreateWorkspace.

Estimate (planning ranges, not measured): CLAUDE 8-12 working days, GPT 8-12, GEMINI 6-8, after
the Rust/SQL merge. Basis: 2-3 days extraction and re-homing, 3-5 days shell, tree, row builder
and input, 2-4 days gate, packaging and pane-lifecycle faults.

Dissent / residual risk:
- Fallback to C. CLAUDE: automatic if the compile spike does not build in two days. GPT: day two
  is a review point, never an automatic switch. Not resolved; the owner decides at day two.
- `SplitLayout` and arrangement restore. GPT retains the 09-22 restore and inner-split behaviors;
  CLAUDE and GEMINI hold them outside the 2b target (shell tabs are not in scope), which is the
  1-2 day gap between the ranges.
- Biggest risk, all seats: the lifted files are less separable than their names suggest
  (`SwarmAgentWorkspaceModel.swift:41-48` takes `Store?`; 44 `SessionID`, 15 `WorkspaceID`, 12
  `Store` references in the set), and SwiftTerm view identity and Escape routing under a new
  container. No seat built, tested or launched anything this run.
- Corrections to the chair's brief: `Package.swift` no longer lists Sparkle; 23453b1a is 70 files
  +820/-1151; new files over upstream are 59 rename-aware.

Per-model trail:
- GEMINI: R1 GO-WITH-CHANGES B, 4-5 days, ~2k-line greenfield; R2 conceded behavior-driven
  extraction, the `TranscriptEvent` cut and the missing gate line, refuted GPT's split budget, 6-8.
- GPT: R1 GO-WITH-CHANGES B, 9-14 days, extract by behavior, `SwarmBusStore` optional; R2 conceded
  a written list and the terminal permission flow, refuted the fixed 25-file unit and GEMINI's
  4-5, 8-12.
- CLAUDE: R1 GO-WITH-CHANGES B, 7-10 days, fixed 25-file lift; R2 conceded ten GPT points
  (renderer cut, `SwarmBus` split, packaging, git helpers, `SwarmBusStore` as slice two), refuted
  GEMINI's import day and `TerminalGhostty`, and GPT's split retention, 8-12.

Round wall times: R0 15:38:35-15:46:14 (8 min); R1 15:54:49-16:05:16 (10 min); R2
16:05:58-16:11:16 (5 min). Scratch: /tmp/councils/990e96ab-ada6-428c-b392-4ebc202778c4 (removed
after this log).

The decision to start the rebuild, and the day-two C fallback, remain with the owner.
