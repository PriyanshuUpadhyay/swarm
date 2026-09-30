Bottom rows of agent panes for the screen check tests in `src/screen.rs`.

- `claude-*.txt` are real `tmux capture-pane` output from Claude Code 2.1.284, with the account,
  session id, and paths replaced by neutral text. `claude-question.txt` is an AskUserQuestion
  prompt captured on 2026-09-29.
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

Digit keys, checked live on the same versions: a digit picks a Claude permission or
AskUserQuestion choice, a Codex approval choice, and an AGY `ask_question` choice with no Enter.
On the Codex folder trust screen a digit only moves the cursor, so Enter follows.
