Bottom rows of agent panes for the screen check tests in `src/screen.rs`.

- `claude-*.txt` are real `tmux capture-pane` output from Claude Code 2.1.284, with the account,
  session id, and paths replaced by neutral text.
- `codex-*.txt` are synthesized from the codex-cli 0.157.1 TUI snapshot tests
  (`codex-rs/tui/src/**/snapshots`), not captured. `codex-failed.txt` is synthesized from the
  error cell in `codex-rs/tui/src/history_cell/notices.rs` (`new_error_event`, a red `■ message`
  row), not captured.
- `agy-*.txt` are synthesized from Herdr 0.9.1's AGY detection rules, not captured. AGY was not
  launched, so these are the weakest fixtures.
