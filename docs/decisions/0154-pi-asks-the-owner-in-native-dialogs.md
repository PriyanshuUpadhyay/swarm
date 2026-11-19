---
status: proposed
date: 2026-11-19
related: ["0029", "0086", "0131", "0147"]
---

# 0154. Pi asks the owner in native dialogs

Pi has no built-in model tool for a question or plan approval. The Claude bridge does not expose Claude Code's tools for these actions.

## Decision

- Swarm's Pi extension registers `ask_user_question` and `approve_plan`. Questions use Pi's native choice or text dialog. Plan approval renders the full plan in the selector title and returns approval only when the owner chooses **Approve**. **Request changes** opens a pane input for optional feedback. This tool does not switch Pi into a restricted execution mode.
- Pi reports the wait through OSC 7501 `blocked:kind=question`. Swarm uses the existing provider state path and the HUD notice inbox. Pi's UI prompt events do not need to send a second status report.
- When a plan fits in Pi's native selector, the existing question card and `swarm answer` use its labels and cursor. The owner reply path checks the current prompt ID and uses arrow keys and Enter. It does not reconstruct a clipped selector from scrollback.
- A taller plan opens a blocking, pageable Pi overlay. Page Up and Page Down move through the plan while the decisions stay on the screen. The overlay reports an Open pointer, without remote decision buttons, because its current page does not show the full plan.
- The HUD offers plan decision buttons only when the whole plan fits in the card. If it does not fit, or the pane view clips the plan, the card directs the owner to **Open** and read it in the chat or Pi pane. Text answers still go in the pane. This keeps the existing choice-index API and does not add a channel for typing free text remotely.

This reuses the native UI and the existing owner gate. If a Pi upgrade changes the dialog layout, users can still answer in the pane, but Swarm may not show choice buttons until its screen reader is updated.
