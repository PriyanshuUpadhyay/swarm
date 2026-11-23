---
status: proposed
date: 2026-11-19
related: ["0029", "0086", "0131", "0147"]
---

# 0154. Pi asks the owner in native dialogs

Pi has no built-in model tool for a question or plan approval. The Claude bridge does not expose Claude Code's tools for these actions.

## Decision

- Swarm's Pi extension registers `ask_user_question` and `approve_plan`. Questions use Pi's native choice or text dialog. Plan approval renders the full plan in the selector title and returns approval only when the owner chooses **Approve**. **Request changes** opens a Pi text dialog for optional feedback, which the chat card answers with its text box. This tool does not switch Pi into a restricted execution mode.
- Pi reports the wait through OSC 7501 `blocked:kind=question`. Swarm uses the existing provider state path and the HUD notice inbox. Pi's UI prompt events do not need to send a second status report.
- When a plan fits in Pi's native selector, the existing question card and `swarm answer` use its labels and cursor. The owner reply path checks the current prompt ID and uses arrow keys and Enter. It does not reconstruct a clipped selector from scrollback.
- A taller plan opens a blocking, pageable Pi overlay. Page Up and Page Down move through the plan while the decisions stay on the screen.
- When the screen shows only part of a plan, in the overlay or in a clipped native selector, Swarm reads the whole plan from the newest `approve_plan` call in Pi's session log, as it reads Claude and Codex questions from their logs. The screen still proves that the dialog is open and gives the cursor. The log call must have no tool result, so a closed or older plan never gets buttons. The chat card then shows the whole plan with **Request changes** and **Approve**, and `swarm answer` sends arrow keys and Enter as for the native selector. The pane is for debugging, not the normal way to answer.
- No Pi question sends the owner to the pane. The overlay keeps its two choice rows on a pane of any height, so the card can read the cursor and press the keys. An `ask_user_question` choice or text dialog taller than the pane also comes from its open log call; for a choice dialog, the choices on the screen must match the logged choices. A typed answer first clears any text left in Pi's answer box with Ctrl-E and Ctrl-U.
- If the log has no open plan call, the card says that Swarm could not read the plan and shows no decision buttons, so nothing is approved blind. Pi writes the call to its log before it opens the dialog, so this state does not last.
- The HUD offers plan decision buttons only when the whole plan fits in its small card. Otherwise a click opens the chat, which shows the plan and its buttons. This keeps the existing choice-index API.

This reuses the native UI and the existing owner gate. The pane is for debugging. If a Pi upgrade changes the dialog layout, the dialog shows as a screen card (ADR 0159) until its screen reader is updated.
