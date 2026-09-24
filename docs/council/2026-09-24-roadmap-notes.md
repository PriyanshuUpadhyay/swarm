# Council: the owner's Swarm roadmap notes

Date: 2026-09-24. Invoked by the owner: "get council to opine" on eight pasted notes (spawn Enter
race, coupling of jello/swarm/dotfiles/UsageHUD, branch-specific DB, worktrees, bare clone for new
projects, monorepo, other additions, hiding mechanics from the model).

## Round 0

- CLAUDE asked whether swarm goes public while jello and dotfiles stay private. Owner: "Stays
  private". So swarm may own its Swarm-specific glue.
- GPT, GEMINI: no questions.

## Models

council.claude = claude-fable-5-1 (xhigh), council.gpt = gpt-6-astra (high), council.gemini =
gemini-3.8-flash-high (agy). No fallback used.

## Verdict: GO-WITH-CHANGES (unanimous, converged in round 2)

Every voice put one Swarm-owned `swarm launch` first. It takes over trust, the worker pool, `--cwd`,
and provider flags from `~/.claude/scripts/swarm-spawn-role.py`. It waits for the child to report
ready, and a launch-success test covers it. The Enter race is handled inside that step. There is no
separate fix, and a longer delay is not a fix. No voice supported a monorepo.

Required changes, in order:

1. One `swarm launch`, with trust, pool, `--cwd`, a ready-wait, and a launch-success test that
   covers an immediate send and a start that fails. After parity, delete `swarm-spawn-role.py`.
2. The FOREIGN KEY failure (`todo.md:15-20`) is separate. Its recorded cause is wrong, because the
   agent row commits before the pane opens (`src/main.rs:356`). The suspect is an unregistered
   caller, so make `spawn` and `send` refuse one, add a test, and rewrite the todo entry.
3. `swarm start` (session, agent add, and sweep in the binary) and `swarm receive` (print and ack).
   The host banner shrinks to a few verbs. This answers note 8.
4. `swarm doctor` checks adapter drift, binary commit vs repo, routing and yelo reachability, cwd
   trust, and the `wt` guard. Recent outages were drift (2026-09-21 council).
5. Worktrees go through Worktrunk. In the UI, New Chat runs `wt switch --create <b> --base <ref>`
   and passes the path as the chat directory. The UI already lists worktrees
   (`GitRepositoryPaths.swift`, `SessionsTree.swift`).
6. Move the Swarm-owned glue (swarm-split.py, host-swarm.md, agent-host-context) into the swarm
   repo. Jello keeps yelo, accounts, and UsageHUD behind the existing command seams.
7. Add one README line that says one DB per `SWARM_HOME`, with sessions keyed by cwd and not by
   branch. A separate `SWARM_HOME` isolates dev builds only.

Answered with no build needed:

- Note 3 (branch DB). Not branch-specific, and it should stay that way (`src/paths.rs`, ADR 0015).
- Note 5 (bclone). `wt clone` already works. The dotfiles `~/.local/bin/wt` guard passes it to
  Worktrunk, and `~/.local/bin/wt-clone` makes `~/work/<name>/.bare` + `wt/<branch>`. `wt --help`
  does not list it. Doctor checks that it resolves. An empty new project (no remote) is not covered.
- Note 7 (model check). Partly done in d852906a, which warns. What remains is error propagation
  with suggested fixes, AGY catalog checks, and account access.

## Dissent / residual risk

- GPT: the SessionStart hook records chair identity only (`src/bus.rs:91,216`). A lost Enter on the
  launch line itself means no process starts, so the ready-wait needs a timeout that reports failure.
- GEMINI wanted the session to record branch metadata beside cwd. The others did not take this up.
- CLAUDE's biggest risk: folding the launcher into Rust may break jello ADR 0016. The account pick
  lives in the interactive `claude()` shell function, so first prove it with one Fable seat.
- GEMINI round 2 said `wt clone` does not exist. The chair checked, and the dotfiles helper proves it
  does. GEMINI's claim was rejected.

## Per-model trail

- GEMINI: R1 fix the race first, add a bclone command. R2 conceded one launch first and FK separate.
- GPT: R1 test launch first, no monorepo. R2 conceded one launch path and found wt-clone.
- CLAUDE: R1 one launch, doctor, race last. R2 conceded the launch test, FK cause, `--cwd`, receive.

## Timing

Round 1 about 8 min, round 2 about 6 min. Pinned brief and rounds: /tmp/councils/db60c3a6-c217-4bcb-88ed-0d5069b59231.

# Part 2: four more notes

Owner: "get them to opine on these as well.. then combine both the reports". Notes: blocked-pane
detection that is adapter-agnostic, automatic updates, ring de-duplication that ignores a
different sender, and `swarm notify` for child notifications. Fresh seats, same routes.

## Round 0

CLAUDE asked who may call `swarm notify`. Owner: "Chair only". GPT, GEMINI: no questions.

## Verdict: GO-WITH-CHANGES (unanimous, converged in round 1 after the owner's answer)

- **Blocked pane.** This is real. `sweep_once` checks only that the pane exists and the ask was
  read (`src/main.rs:490`), and the adapter has no state verb (`src/adapter.rs:2`). Add an
  optional adapter verb `state` that returns working, idle, blocked, or unknown. On Herdr it reads
  `herdr pane get` agent_status (`herdr src/detect/mod.rs:11`). tmux leaves it out, so its result
  is unknown. Launch uses it for the ready-wait, and the sweep sends one note to the chair when a
  pane becomes blocked. Swarm never answers a prompt by itself.
- **Updates.** This is real. The installed binary is 2ac68f98 from `wt/ui-composer`, and main
  (72c27b01, with PR #5) is newer. The binary is the only part that drifts, because adapters are
  compiled into it and skills are symlinks. `ui/Tools/build.sh:17` reinstalls only at UI build.
  `swarm doctor` compares the commits, and `swarm doctor --fix` (or `swarm update`) builds before
  it swaps and does not swap during an active session. There is no daemon. Yelo updates stay in
  jello.
- **Ring de-duplication.** This is a confirmed bug, and the sender is not the cause.
  `has_rung_unread` (`src/store.rs:439-457`) blocks a new ring while any message is rung but not
  acked. That includes a message the recipient already listed. The sweep also skips the
  orchestrator (`src/store.rs:517`), so a lost ring to the chair is never repaired. This council
  lost Gemini's part-1 finish ring at 06:34:39Z and Gemini's part-2 ask ring. Fix it in about 15
  lines. (a) Add `seen_at IS NULL` to the check. (b) The sweep also re-rings the sweeper's own
  unseen messages. (c) Add tests for arrival after inbox, mixed senders, and a failed ring.
- **`swarm notify`.** Chair only (owner). Add an optional adapter verb `notify`, which is
  `herdr notification show` on Herdr and osascript on tmux, and a `swarm notify <title>` command.
  Workers report to the chair, and the chair notifies.

Dissent / residual risk:

- CLAUDE says the adapter verb is enough and a separate module is not needed. The owner's note
  allowed a module. This is left open.
- GEMINI wanted a ring when a new sender arrives. CLAUDE and GPT hold that the `seen_at` fix covers
  it, because one `swarm inbox` lists every sender.
- Risks: a guessed pane state must never become authority to send input. More rings can land while
  the chair is mid-turn.

# Combined order (part 1 + part 2)

1. One `swarm launch`: trust, pool, `--cwd`, a ready-wait on the `state` verb with a timeout that
   reports failure, and a launch test. Delete `swarm-spawn-role.py`.
2. Ring repair: de-duplicate on `seen_at`, sweep re-rings the chair, and tests.
3. FK error: `spawn` and `send` refuse an unregistered caller, add a test, and fix `todo.md`.
4. `swarm start` + `swarm receive`, the sweep reports blocked panes, and `swarm notify` for the chair.
5. `swarm doctor`: adapter drift, binary commit vs main, routing/yelo, cwd trust, `wt` guard,
   and `--fix` to rebuild.
6. Worktrees through Worktrunk from UI New Chat.
7. Move Swarm-owned glue into the swarm repo. Jello keeps yelo, accounts, and UsageHUD.
8. README line: one DB per `SWARM_HOME`, keyed by cwd.

Part 2 per-model trail:
- GEMINI: ring fix before start, and `swarm update` plus notify late.
- GPT: ring fix third, updates and notify last, children request and chair delivers.
- CLAUDE: ring fix second, with the lost-ring timestamps as evidence. Two optional adapter verbs
  and no module.

Part 2 timing: round 1 about 7 min.
