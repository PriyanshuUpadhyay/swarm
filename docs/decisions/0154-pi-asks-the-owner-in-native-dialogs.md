---
status: proposed
date: 2026-11-19
related: ["0029", "0086", "0131", "0147"]
---

# 0154. Pi asks the owner in native dialogs

Pi has no built-in model tool for a question or plan approval. The Claude bridge does not expose Claude Code's tools for these actions.

## Decision

- Swarm's Pi extension registers `ask_user_question` and `approve_plan`. Questions use Pi's native choice or text dialog. Plan approval shows the full plan and returns approval only when the owner chooses **Approve**. This tool does not switch Pi into a restricted execution mode.
- Pi reports the wait through OSC 7501 `blocked:kind=question`. Swarm uses the existing provider state path and the HUD notice inbox. Pi's UI prompt events do not need to send a second status report.
- When the current pane shows the complete selector, the existing question card and `swarm answer` use its labels and cursor. The owner reply path checks the current prompt ID and uses arrow keys and Enter. It does not reconstruct a clipped selector from scrollback.
- For a text answer or a selector that Swarm cannot read in full, use **Open** and answer in the pane. This keeps the existing choice-index API and does not add a channel for typing free text remotely.

This reuses the native UI and the existing owner gate. If a Pi upgrade changes the dialog layout, users can still answer in the pane, but Swarm may not show choice buttons until its screen reader is updated.
