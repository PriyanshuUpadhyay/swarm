# Council: PR #2 pre-merge review (logic and communication)

- Artifact: https://github.com/PriyanshuUpadhyay/swarm/pull/2, branch `ui-rebuild` at ead917cb, merge base 0e0a600b.
- Question (user): "get council to review pr2 for issues in logic and communicaiton and fixes we should do before we merge it"
- Round 0: skipped by the chair. The request was clear, so each seat stated its own assumptions.
- Resolved constraints: the rebuild itself (ADR 0014, ADR 0015) and the database recreation are out of scope.
- Models: Claude `claude-fable-5-1` xhigh, GPT `gpt-6-astra` high, Gemini `gemini-3.8-flash-high` high. Every seat ran in a visible Herdr pane. No seat failed or was replaced.
- Converged in round 3.

| Round | Wall time | Result |
|---|---|---|
| 1 | 14:54 to 15:14 | Claude GO-WITH-CHANGES, Gemini GO-WITH-CHANGES, GPT NO-GO |
| 2 | 15:10 to 15:14 | same split, with the must-fix lists nearly aligned |
| 3 | 15:14 to 15:16 | all GO-WITH-CHANGES (GPT: its NO-GO meant "not before fixes", not "redesign") |

## Verdict: GO-WITH-CHANGES (unanimous)

The design is sound. The defects are missing identity checks at boundaries: the chair role versus its id,
a pane id across tmux servers, and a log chosen by time. Each one lets the app show or drive the wrong chat.
Every fix is local and needs no schema change.

## Required changes before merge

Logic:
1. Chair identity. `swarm launch orchestrator <routing role>` stores the routing role, but `orchestrator_of` and
   `set_pane` match role `orchestrator` (src/store.rs:537, :345; src/main.rs:354, :630). In an app-started chat a
   child's `finish` fails with "has no orchestrator". Register the chair with role `orchestrator`, or also match
   `id = 'orchestrator'`. Add a test where an app-launched chair receives a child's finish.
2. `swarm session chair` accepts any caller (src/main.rs:531). With the Claude chair hook on every `launch` argv
   (src/bus.rs:91), any Claude agent started through `launch` becomes the chair. Refuse a caller that is not the
   orchestrator.
3. Close uses the default `tmux-solo` adapter, not the session's (ui/Sources/SwarmCore/Agent/SwarmCLIBus.swift:168,
   :229). On a CLI-started chat it can kill a different chat that holds the same `%N` pane. Close must also close the
   session's children before it archives (ui/Sources/Swarm/SwarmApp.swift:59).
4. `set_pane` clears a pane id across adapters (src/store.rs:342). Scope it to the same adapter.
5. Chair-log discovery keeps its first answer for the app's lifetime (ui/Sources/SwarmCore/Agent/SwarmSessionDetail.swift:73,
   SwarmSessionListing.swift:292). A new Codex chat in the same folder can show the previous chat's transcript and
   title. Re-resolve each poll while the chair id is nil, and drop the cached title when the log changes.
6. `summarize_log` deletes the log before `deliver` can fail, and the job stays `running` (src/main.rs:452, :676, :396).
   Keep the log until delivery succeeds, release the job on failure, and clear a dead pane first.
7. `CLAUDE_SECURESTORAGE_CONFIG_DIR` is built from `SWARM_HOME` (src/profiles.rs:115). A dev build with its own home
   launches Claude with no login. Read `HOME`.
8. When accounts fail to load, the sheet says "Using this repository's settings" but sends `--account auto`
   (ui/Sources/Swarm/NewChatSheet.swift:76, SwarmLaunchChoice.swift:127). Send no account flag.

Communication:
9. README command table and adapter headers describe the old `init` and `adapter check`, and omit the new verbs.
10. adapters/herdr.conf:7 runs `~/.config/herdr/bin/swarm-split.py`, which the repo does not ship. Ship it or name the dependency.
11. PR body: "a pane belongs to one chair" is true only after fix 1. State the ADR 0014 gate result and the deferred `SwarmBusStore`.
12. SwarmBus.swift:6 and SwarmProfiles.swift:6 cite `docs/bus-contract.md` and `docs/profiles-contract.md`, which do not exist.
13. State that `SWARM_HOME` is set by hand for each process and is the parent of `.swarm/`.

## Follow-ups (safe to merge without)

- Stale Bloom-fork comments (LoginShellPath.swift:13, WorktreeListing.swift:9, Shell.swift, TmuxSessions.swift).
- "earliest" should read "nearest" in ChairLogDiscoveryTests.swift:8.
- Agent row committed before the adapter spawns (src/main.rs:354, :365). The failure is visible, and a retry uses a new session.
- About 1.5k lines of lifted code with no caller (SplitLayout, TmuxSessions, ProcessTable, parts of SwarmAgentConversation).
- Polling cost of one `agents --json` per session every 2 s.
- Drop the chair hook from non-chair `launch` argv once fix 2 exists.

## Dissent and residual risk

- Log discovery: GPT holds that re-resolving is not enough. Two Codex chats started in one folder within seconds can
  both pick the first log. GPT wants a verified launch-to-log link. Claude and Gemini accept re-resolve for merge and
  call a verified link design work. Gemini's `delay >= -5` rule was dropped because CLI-started sessions begin after
  their log (ChairLogDiscoveryTests.swift:85).
- Draft cleared during send (ui/Sources/Swarm/SessionDetail.swift:27-31): GPT says must-fix (user text is lost).
  Claude and Gemini say follow-up. Decision: follow-up, 2-1.
- Codex trust on `launch` without `--account` (src/main.rs:641): Gemini says must-fix. Claude says follow-up because
  the app always passes an account. GPT says it becomes must-fix if fix 8 drops the account flag. Because fix 8 does
  that, add the trust write for the default Codex home in the same change.

## Per-model trail

- GEMINI: GO-WITH-CHANGES in every round. Found Codex trust, Close leaving children, and README gaps. Its discovery-window fix was refuted.
- GPT: NO-GO, then NO-GO, then GO-WITH-CHANGES. Found the cross-server pane collision, the chair-hook hijack, and draft loss. It kept its objection on verified log identity.
- CLAUDE: GO-WITH-CHANGES in every round. Found the lost summary log, the dev-login path, missing contract docs, and the herdr script dependency.
