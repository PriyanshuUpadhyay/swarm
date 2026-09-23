# Council: is the Swarm UI approach right? (2026-09-17)

Artifact: branch `ui-bloom` at eaafc97a (PR #1), the Bloom fork in `ui/`, `docs/bus-contract.md`,
`docs/decisions/`, the 2026-09-16 Conductor-style UI research report, and
https://github.com/charannyk06/conductor-oss.

Question: review the problem statement and the approach (fork of spatie/bloom renamed Swarm), after
the owner called it "pure bloat", hard to navigate, with no easy model switch, no context usage, and
a Codex chat about CDC that vanished after an app restart.

## Round 0

Folded into round 1 as a QUESTIONS line. All three voices answered NO QUESTIONS.

## Models used

- CLAUDE: route `council.claude` (claude-fable-5-1, xhigh); the seat ran on Opus 5 after the CLI fell
  back from Fable.
- GPT: `council.gpt`, gpt-6-astra, high.
- GEMINI: `council.gemini`, gemini-3.8-flash-high.

## Verdict: GO-WITH-CHANGES, keep-and-cut (unanimous, converged at round 2)

Round 1 split 2-1: CLAUDE and GPT keep-and-cut, GEMINI NO-GO with a 2-3 day thin rebuild. In round 2
GEMINI re-priced a daily-use rebuild at 11 days and moved to keep-and-cut; CLAUDE raised its
keep-and-cut figure to 6-8 days and adopted GPT's trial as the next step.

Consensus reasoning: every fault the owner named is a default or an unwritten feature, not the base.
The CDC chat died because terminal persistence is off by default and the launch sweep then kills every
tmux session (`ui/Sources/Swarm/Views/Terminal/TerminalPersistence.swift:14-16`,
`ui/Sources/SwarmCore/System/TmuxSessions.swift:159-161`). The Ghostty look is appearance inheritance
(`ui/Sources/Swarm/Views/Terminal/TerminalGhostty.swift:34`). A thin rebuild costs 10-14 days (GPT
10-18) and arrives with the same missing model and context strip; conductor-oss owns its own PTYs and
has one author.

Required changes:
1. Separate three lifetimes: quitting the UI detaches; Stop ends the process; Archive hides history.
   Turn persistence on, never archive or kill sessions at quit, reconnect to a live pane first, and
   leave a dead session visible with its history and a resume action.
2. Read non-Claude history (Codex rollout logs) for sessions with no `chair_log`
   (`docs/bus-contract.md:152-155`).
3. Cut the built-in agent runner and message queue, the Settings path back to the built-in chat, the
   embedded browser (52 `Browser*.swift` files), Preview, Code, Oceans, and unused side features, by
   their callers. Keep the `Swarm*.swift` bus files in `SwarmCore/Agent` and the transcript rows the
   session view reads.
4. Add a strip beside each pane with provider, real model, effort and context, from provider data,
   showing "Unavailable" when the provider gives none. A model change must be confirmed, not assumed.
5. One navigation tree: project, worktree, sessions (app- and CLI-started), with search and clear
   running, waiting and stopped states. Default the terminal theme to the app's own.

Next step put to the owner: a two-day trial in the fork. Pass means one Codex chat and one CLI-started
swarm session in one tree, Escape reaching only the selected pane, then quit and reopen with both rows
live and the CDC conversation readable. Failure that needs broad changes outside the lifetime boundary
means rebuild-thin.

Dissent / residual risk: the cut can stall and leave ~215k lines of Bloom Swift in place (CLAUDE).
Estimates are planning figures, not measurements; provider context data may be missing (GPT).
Embedded Herdr terminals are not possible without an `attach` verb in `herdr.conf`
(`docs/bus-contract.md:46-47`).

Per-model trail:
- GEMINI: R1 NO-GO, rebuild-thin 2-3 days; R2 conceded persistence is a default and re-priced rebuild
  to 11 days, keep-and-cut 4.5 days.
- GPT: R1 GO-WITH-CHANGES keep-and-cut 6-10 days behind a two-day trial; R2 held, refuted whole-folder
  deletion and "three small features".
- CLAUDE: R1 GO-WITH-CHANGES keep-and-cut 4-6 days, one-day persistence fix; R2 conceded the browser
  and the Codex history gap, 6-8 days, backed GPT's trial.

Round wall times: R1 17:53:44-17:58:38 (5 min); R2 18:00:24-18:02:36 (2 min).

The decision to run the trial, cut, or rebuild stays with the owner.
