Bottom rows of agent panes for the screen check tests in `src/screen.rs`.

- `claude-*.txt` are real `tmux capture-pane` output from Claude Code 2.1.284, with the account,
  session id, and paths replaced by neutral text. `claude-question.txt` is an AskUserQuestion
  prompt captured on 2026-09-29. `claude-submit-answers.txt` is the submit tab of a question with
  two tabs, captured from Claude Code 2.1.312 on 2026-10-28; the digit `1` submitted it.
- `codex-waiting.txt` and `codex-trust.txt` are real captures from codex-cli 0.159.0 on
  2026-09-29 (an escalated command approval with `approvals_reviewer=user`, and the folder trust
  screen). The other `codex-*.txt` are synthesized from the codex-cli 0.157.1 TUI snapshot tests
  (`codex-rs/tui/src/**/snapshots`), not captured. `codex-failed.txt` is synthesized from the
  error cell in `codex-rs/tui/src/history_cell/notices.rs` (`new_error_event`, a red `■ message`
  row), not captured.
- `agy-trust.txt` and `agy-question.txt` are real captures from Antigravity CLI 1.2.13 on
  2026-09-29 (the folder trust screen and an `ask_question` prompt). The other `agy-*.txt` are
  synthesized from Herdr 0.9.1's AGY detection rules, not captured. `agy-waiting.txt` stays
  synthesized: with the terminal sandbox on, AGY ran every probe command without an approval.
- `pi-*.txt` are real `tmux capture-pane -p -J` output from pi 1.2.0 on 2026-10-15 (an empty
  input box, a ring left unsent in it, and a running turn), cut to the rows from the box up and
  with the folder replaced by `~/project`.
  `pi-trust.txt` is pi 1.2.0's folder trust screen, captured on 2026-10-29 from a new folder with a
  `.pi` folder, under a private tmux server, with the paths replaced by `/Users/you`.
- `*-turn-*.txt` are real `tmux capture-pane -p -J -S -` output of one turn (one `ls docs`, then a
  short history of the bicycle), captured on 2026-10-29 from Claude Code 2.1.312, codex-cli
  0.162.1, pi with its claude-bridge model, and Antigravity CLI 1.4.10, for the live turn cut in
  `src/screen/live.rs` (ADR 0131). The account, email, and home path are replaced by neutral text.
  `claude-turn-text.txt` is the turn just after its end, with the block mark scrolled off; its
  `✻ Sautéed for 11s · done 6:52 PM` row is replaced by a running row, so it reads as if the turn
  still ran.

Digit keys, checked live on the same versions: a digit picks a Claude permission or
AskUserQuestion choice, a Codex approval choice, and an AGY `ask_question` choice with no Enter.
On the Codex folder trust screen a digit only moves the cursor, so Enter follows.
