# Real Codex and AGY screens for the screen check (ADR 0021, ADR 0070)

Date: 2026-10-24. Scope: the todo.md item "Real screen captures for Codex and AGY agent states".
Versions: codex-cli 0.162.1, Antigravity CLI (AGY) 1.4.7, tmux 3.8, all on this Mac. Research only.
No repo code or fixture changed.

## Result

Two real states read wrong today. A Codex pane whose turn failed reads as done when its history is
short, and every AGY failure reads as done. The other states match.

Example: a fresh 120×40 Codex pane runs one turn with a bad model name. Codex draws the red
`■ {"type":"error",…}` row on row 9 and pins its composer to row 37. `screen_state` reads only the
bottom 15 rows (`ROWS`), so it never sees the `■` row and returns `Idle`. The listing shows "done".

Rule: a state that the classifier needs must sit in the bottom 15 rows, and Codex's error row does
not until the history fills the pane.

Each capture below went through `swarm::screen::screen_state`, `failure_detail`, and `prompt` from
this worktree (a scratch crate in `/tmp` that depends on it by path).

| Capture | What the pane shows | Screen check reads | Correct? |
|---|---|---|---|
| Codex trust | `Trust this folder?` … `2. Quit` · `enter continue · esc quit` | Waiting, prompt | yes |
| Codex idle | `› Ask Codex to do anything`, then the status line | Idle | yes |
| Codex working | `• Working (2s • esc to interrupt)` | Working | yes |
| Codex approval | `Would you like to run the following command?` … `Press enter to confirm or esc to cancel` | Waiting, prompt | yes |
| Codex denied / Esc | `■ Conversation interrupted - use /feedback if something went wrong` | Idle | yes |
| Codex failed (bad model) | `■ {"type":"error","status":400,…}` 28 rows above the composer | **Idle** | **no**, should be Failed |
| AGY trust | `Do you trust the contents of this project?` | Waiting, prompt | yes |
| AGY idle | `>` box between two `─` rules, `? for shortcuts` footer | Idle | yes |
| AGY working | `⣯  Generating...`, `esc to cancel` footer | Working | yes |
| AGY approval | `Requesting permission for:` … `Run this command?` … `tab Amend` | Waiting, prompt | yes |
| AGY denied / Esc | `⎿  Interrupted · What should Antigravity CLI do instead?` | Idle | yes |
| AGY failed (no network) | `⚠ failed to construct executor: …` + `Error ID: <id>` | **Idle** | **no**, should be Failed |

## Patterns that differ from the fixtures

### Codex 0.162.1 (fixtures: 0.157.1 snapshots, 0.159.0 captures)

1. **The error row is far from the composer in a short history.** Codex runs in the alternate
   screen (`codex-rs/tui/src/lib.rs:1943`, `determine_alt_screen_mode`), writes history from the
   top, and pins the composer to the bottom. In the bad-model capture the `■` row is row 9 and the
   composer is row 37. In a capture with three turns, the last `■` row is row 20, still 17 rows
   above. `codex-failed.txt` puts the error row two rows above the composer, so the test passes
   but a real pane does not. The row moves into the bottom 15 rows only after the history fills
   the pane (inferred from these two captures, not captured).
2. **A real error message is often raw JSON and wraps.** The `screen` verb has no `-J`, so the
   message wraps onto a second row and `failure_detail` would keep only the first row. The source
   draws `format!("■ {message}")` as one red line (`codex-rs/tui/src/history_cell/notices.rs:331-335`
   at `rust-v0.162.1`); the width does the wrap.
3. **The interrupt row text changed.** 0.162.1 draws
   `■ Conversation interrupted - use /feedback if something went wrong`
   (`chatwidget/input_restore.rs:332`). 0.157.1 drew `Conversation interrupted - tell the model what
   to do differently. Something went wrong? Hit /feedback…` (`chatwidget/turn_runtime.rs:565`).
   `codex_failure` matches only the prefix `Conversation interrupted`, so both read as idle.
4. **The trust screen's second choice depends on the flow, not the version.** A plain start shows
   `2. Quit` and `enter continue · esc quit`. `codex-trust.txt` has `2. Back to Agent Command Center`
   and `esc back`, which the agent-overview flow draws (`onboarding/trust_directory.rs:93-94`).
   Both read as Waiting.
5. **The footer is user-configurable.** This Mac's `[tui] status_line` replaces
   `100% context left` with `GPT-5.6-Sol xhigh · main · Ask for approval · Context 0% used · …`.
   The classifier does not read the footer, so nothing breaks. A fixture must not depend on it.
6. **Request-input shortcut glyphs changed** from `ctrl+j`/`ctrl+x` to `⌃j`/`⌃x`
   (`bottom_pane/request_user_input/mod.rs:2526-2548`). The classifier reads `to submit answer`,
   which did not change. Not captured, because a plain prompt cannot make Codex call that tool.
7. Unchanged since 0.157.1, and confirmed live: `• Working (… • esc to interrupt)`
   (`status_indicator_widget.rs:99,452`), the exec approval modal and its footer
   (`chatwidget/snapshots/…approval_modal_exec.snap`), and `› Ask Codex to do anything`.

### AGY 1.4.7 (fixtures: 1.2.13 captures and Herdr 0.9.1 rules)

1. **Idle prompt.** `agy-idle.txt` has ` > Type a message`. The real prompt is an empty `>` (or a
   mode hint such as `> Accept-edits mode: file edits auto-approved (shift+tab to cycle)`) between
   two full-width `─` rules, with `? for shortcuts` and the mode and model on the row below. The
   classifier's "any text is idle" fallback still gives Idle, so this is the pattern the
   `ponytail:` comment in `classify.rs` asks for.
2. **Working row.** `agy-working.txt` has ` ⠹ Thinking (4s)`. The real row is a Braille glyph, two
   spaces, and a label: `⣯  Generating...`, `⣽  Loading...`, `⣷  Running command...`, or the
   model's own thought line. The footer changes from `? for shortcuts` to `esc to cancel`. The
   classifier's "Braille then a space" rule matches.
3. **Approval dialog.** `agy-waiting.txt` is not what AGY draws. The real dialog is:

   ```
   Requesting permission for:
      touch probe5.txt
   Run this command?
   > 1. Yes, run command
     2. Yes, and always allow in this conversation for commands that start with 'touch'
     3. Yes, and always allow for commands that start with 'touch' (Persist to settings.json)
     4. No, cancel
     ↑/↓ Navigate · tab Amend · ctrl+g edit/expand command
   esc to cancel
   ```

   There is no `Agent is` before `requesting`, no `Do you want to proceed?`, and the choices are
   numbered. The classifier still says Waiting, because `requesting permission for:` and `tab amend`
   both match. `prompt()` also finds it.
4. **Failure has rows of its own.** A failed turn draws `⚠ <message>` and then `Error ID: <uuid>`
   above the idle box. The classifier has no AGY failure pattern, so it reads Idle. A startup
   `⚠ Warning` row (for example a retired model) has no `Error ID:` row, so `Error ID:` is the
   mark that tells the two apart. A failed tool step shows `● Ran (touch probe5.txt) failed`.
5. **Denied and interrupted turns** both end in `⎿  Interrupted · What should Antigravity CLI do
   instead?` above the idle box. They read as Idle, which is correct.

## How to get each state under a private tmux server

All captures used `tmux -L t4screens new-session -d -s cap -x 120 -y 40 -c <scratch git folder>`,
`send-keys -l` for the prompt, then a separate `send-keys Enter`, and `capture-pane -p` (no `-J`,
the same as the `screen` verb). A capture must replace the account e-mail (AGY's banner shows it)
and the thread id (in Codex's status line).

| State | Codex 0.162.1 | AGY 1.4.7 |
|---|---|---|
| trust | start in a new git folder | start in a new folder |
| idle | wait for `? for shortcuts` | wait for `? for shortcuts` |
| working | capture 1–3 s after Enter | capture 1–3 s after Enter |
| approval | `-a on-request -s read-only -c approvals_reviewer=user`, prompt "Run `touch probe.txt`" | set `/config` → Tool Permission `request-review`, then cycle to the default agent mode with shift+tab, prompt the same |
| failure | `-m no-such-model` (the server answers 400) | `HTTPS_PROXY=http://127.0.0.1:9` (no network) |
| interrupt | Esc during a turn, or choice 3 on the approval | Esc during a turn, or choice 4 on the approval |

Traps found on the way:

- `-c approval_policy=untrusted` stops Codex at start: `approval_policy = "untrusted" is no longer
  supported`. `-a` accepts only `on-request` and `never`.
- Without `-c approvals_reviewer=user`, a config with `approvals_reviewer = "auto_review"` sends the
  approval to the reviewer model ("Approve for me") and no modal shows.
- A `-c projects."<dir>".trust_level="trusted"` override does not skip the Codex trust screen;
  choosing "Trust and continue" writes the folder into `config.toml`.
- AGY's `Tool Permission` setting has the values `always-proceed`, `proceed-in-sandbox`,
  `request-review`, and `strict`. With `always-proceed` (this Mac), no command asks, whatever the
  sandbox or mode. With the sandbox on, AGY ran a sandbox-blocked command a second time outside the
  sandbox without asking. A project asklist rule `command(touch)` did not make it ask either.
  `--model no-such-model` does not fail; AGY falls back to its default model.

## Confirming a state when the adapter has no `screen` verb

Today `read_screen` (`src/cli/read_screen.rs:23`) returns None when the adapter has no `screen`
verb, so a `working` or `waiting` hook report turns to no report after `STALE_S` (45 s). Four
signals can confirm it without a `screen` verb, in the order of ADR 0070 (host records before
screen reads):

1. **AGY sends OSC 7501 itself, but not under tmux by default.** A raw `pipe-pane` record of AGY
   1.4.7 with `AGY_CLI_OSC7501=on` holds
   `ESC]7501;state=working:app=antigravity`, `state=idle`, `state=done:…:msg=<base64 answer>`,
   `state=error:…:msg=<base64 message>`, and at start `state=blocked:kind=auth:msg=<"Sign in
   required">`. Without the variable, AGY sends only the query `ESC]7501;?`; tmux does not answer,
   so AGY sends no status. The binary's release note says: "Set `AGY_CLI_OSC7501=off` to turn it
   off, `AGY_CLI_OSC7501=on` for a terminal that supports it but doesn't answer the check". So
   `swarm launch` can set `AGY_CLI_OSC7501=on` for AGY, and a host that keeps OSC 7501 records
   (Rex's `status` verb, ADR 0086 for pi) gives AGY a host record with Failed, which closes the
   AGY failure gap above too. Today `host_record_of` reads records only for pi
   (`src/cli/ring.rs:205`). Not checked: the approval record (`state=blocked` with a kind), because
   the approval capture ran without the pipe.
2. **Codex sets the pane title.** `pipe-pane` shows `ESC]0;<title>BEL` writes
   (`codex-rs/tui/src/terminal_title.rs:108`). Live, `tmux display -p '#{pane_title}'` gave
   `⠧ work` while working, `[ ! ] Action Required | Run touch probe9.txt | work` (blinking with
   `[ . ]`) while the approval waited, and `Run touch probe9.txt | work` when idle
   (`chatwidget/tests/terminal_title.rs:142` asserts `[ ! ] Action Required | project`). Failure has
   no title mark. The title is a config item (`[tui].terminal_title`, `core/src/config/edit.rs:128`) that a user can change, and a
   user's shell or tmux can rewrite it.
3. **The `capture` verb, which every adapter has.** `chair_idle` and `ring` already fall back to
   `capture` when `screen` is missing (`src/cli/chair_idle.rs:21-24`, `src/cli/ring.rs:341-344`);
   the listing does not. `capture` uses `-J` and the whole scrollback, so the bottom 15 rows are
   the same rows unwrapped. This is still a screen read, so ADR 0070 puts it last.
4. **Liveness only.** A process check proves the agent is alive, not its state. A transcript file's
   change time proves a recent write, not work in progress. tmux-agent-tracker uses the process
   tree only to tie a pane to its agent (github.com/ulas96/tmux-agent-tracker). agent-squad guesses
   running, waiting, or done from the transcript's last event and its age, which is a timing
   guess (pkg.go.dev/github.com/adhikrysh/agent-squad, "Classifying state").

## Sources

- Live captures, 2026-10-24, under `tmux -L t4screens` (scratch, not kept in the repo).
- `github.com/openai/codex` at tags `rust-v0.162.1` and `rust-v0.157.1`, paths under
  `codex-rs/tui/src/` as named above.
- AGY 1.4.7 binary strings (`strings -n 6 ~/.local/bin/agy`) for the OSC 7501 release note and
  the permission modes; `agy --help`. The `agy changelog` output stops at 1.2.13.
- AGY docs on permission choices: https://www.antigravity.google/docs/cli/features.
- Herdr's own AGY integration reports only the conversation id, not state:
  https://github.com/herdrdev/herdr (docs/…/integrations.mdx).
