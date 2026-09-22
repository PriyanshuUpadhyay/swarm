# Council: why the chat-to-CLI path keeps breaking after each fix

Date: 2026-09-21. Invoked by the owner: "we had just fixed everything.... wtf is wrong with the
setup. get council to audit".

## Artifact

The `ui-bloom-temp` worktree at 9b2be8c4, and the deployed state beside it. Three faults found in
one session, all in the path from the chat pane to the CLI:

1. **A deployed adapter the repository had already changed.** `ed76a9df` changed
   `adapters/tmux-solo.conf:3` from `self = true` to `self = printf '%s' "$TMUX_PANE"`. `swarm init`
   writes a shipped adapter only `if !file.exists()` (`src/main.rs:19-24`), so
   `~/.swarm/adapters/tmux-solo.conf` kept `self = true`. `add_agent` then recorded no pane
   (`src/main.rs:275-279`), and every send died in `swarm type` with `swarm: no pane recorded`
   (`src/main.rs:613-622`). Bus sessions 53 and 54 both hold a NULL `agent.pane_id`.
2. **A log reader resolved once.** `InteractiveChatPane.follow` asked
   `InteractiveChatTranscript.reader` at open. A CLI writes its log at its first turn, so a chat
   opened at creation kept a nil reader and drew only its own sent lines.
3. **A stale in-memory row and a false notice.** Nothing reloaded `WorkspaceModel.sessions` when
   the provider session id was written, so `resumeCLI` started fresh and raised a notice that said
   the chat had lost its session, on every new chat.

`ui/Tools/chat-smoke.sh` passed while all three were live.

## Round 0

- OPUS asked whether the hand edit in the deployed `~/.swarm/adapters/herdr.conf` is permanent,
  because it decides whether `init` may refresh a drifted file or only report it. Put to the owner,
  who answered "just fix it however" and left the choice with the chair. Resolved as: the edit
  stays, and a deployed file must never shadow a shipped fix to a verb it does not name.
- CLAUDE and GEMINI: no questions.

## Models used

Routed through `council.claude` and `council.gemini`: Claude (`claude-fable-5-1`, xhigh) and Gemini
(`gemini-3.8-flash-high`). **The council was degraded.** Both `council.gpt` seats (`gpt-6-astra`,
high) exited at launch, one after the other, each printing only
`codex: using profile 'foyer' (expiring usage first; --profile overrides)`. The third voice is
therefore Claude `opus` at xhigh through `review.challenger`, in its own visible pane.

## Verdict: GO-WITH-CHANGES (unanimous, converged at round 1)

All three voices judged the proposal sound and incomplete. Every voice found, independently, that
the smoke check cannot see this fault class by construction, because it builds a temporary
`SWARM_HOME` and runs `swarm init` into it, so it always gets the embedded adapter and never reads
the deployed one. The three agreed that the empty pane must fail loudly, that `adapter check` must
compare text rather than parse it, and that the check must run against the real `~/.swarm`.

### Required changes

1. **The binary owns the adapters, and a deployed file overrides only the verbs it names.**
   `src/adapter.rs` parses the embedded text as the base and lays a deployed file over it. So the
   owner's `herdr.conf` `spawn` edit stays, and a shipped fix to any other verb still arrives.
2. **`init` stops writing adapter files, and removes one that equals the embedded text.**
   A file on disk is then an edit and nothing else.
3. **`set_pane` rejects an empty pane** (`src/store.rs`), which covers `add_agent` and
   `spawn_agent` at once, rather than guarding one caller.
4. **`adapter check` reports where each verb comes from**, so a shadowed verb is visible before a
   send fails.
5. **`docs/bus-contract.md:47` is corrected**; it still says tmux-solo `self` prints nothing.
6. **The sessions feed reloads every open workspace model**, not the selected one alone
   (`ui/Sources/Swarm/State/AppModel.swift`).

### What the pre-install smoke check must cover

The three lists agree on these assertions:

1. `swarm adapter check` passes over the real `~/.swarm/adapters`, and a stale file fails the run.
2. After the chair starts, `swarm agents --json` shows an orchestrator pane that is neither NULL
   nor empty.
3. The second message goes through the app's own send path to `swarm type`, not through
   `TerminalSessionStore.submitToAgent`.
4. The transcript is read the way the pane reads it, not through a private retry loop in the check.
5. A chat opened before its first turn shows the answer, with no reopen.
6. A restart keeps the provider id, and no notice appears.
7. The installed `swarm` reports the commit the repository is on.

### Dissent and residual risk

The voices differed on where the default adapter text lives. CLAUDE wanted a verb-level overlay,
GEMINI wanted the deployed file to override the whole file, and OPUS wanted files kept and
refreshed against a list of texts ever shipped. The chair took CLAUDE's overlay, because the
owner's one edit is a single verb in `herdr.conf` and the other two designs each shadow a shipped
fix or need a hand-kept list. OPUS's warning stands: nothing in `ui/Tools/build.sh` builds the Rust
binary, so a stale binary still passes every check until `swarm --version` exists. That, and the
one-step install, are not in this change.

### Per-model trail

- **CLAUDE** GO-WITH-CHANGES. Found that the smoke's temporary `SWARM_HOME` makes Fault 1
  invisible, that a hand-kept list of shipped texts reintroduces the same fault, and that
  `type`, `interrupt` and `close` load the environment's adapter rather than the session's.
- **GEMINI** GO-WITH-CHANGES. Wanted the embedded default and no migration logic at all, and
  found that the new sessions feed reloads only the selected workspace model.
- **OPUS** GO-WITH-CHANGES. Found that `spawn_agent` writes its pane with no check at all, that
  `adapter check` proves nothing because it only parses, and that the repository states the
  contract twice, with `docs/bus-contract.md` already wrong.

## Applied after the verdict

Required changes 1 to 6 are in the worktree, with `cargo test` (61 tests) and the SwarmCore suite
(6,273 tests) passing. `swarm adapter check` now reads `ok herdr: shipped, with spawn from
~/.swarm/adapters/herdr.conf`, and `swarm init` removed the two deployed files that only repeated
what the binary says. Of the smoke list, assertion 1 is in `ui/Tools/chat-smoke.sh`, which refuses
to run when `swarm adapter check` fails on the real `~/.swarm`. Assertions 2 to 7 need the check to
drive the pane rather than the library beside it, and are not done.

## Wall time

Round 1 dispatched 17:05, answers at 17:08 (CLAUDE), 17:09 (GEMINI) and 17:10 (OPUS). Two Codex
seats died at 17:05 and 17:06. One round, about five minutes.
