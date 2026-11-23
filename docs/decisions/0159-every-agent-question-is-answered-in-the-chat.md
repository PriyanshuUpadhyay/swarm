---
status: accepted
date: 2026-11-24
deciders: [user]
amends: ["0029", "0154"]
related: ["0121", "0120"]
informed-by:
  - 'owner, 2026-11-24: "Nothing should go into pane. Nothing should ask the user to open the pane." The pane is only for debugging.'
---

# 0159. Every agent question is answered in the chat; the pane is for debugging

## Context and Problem Statement

ADR 0029 reads an agent's question from its screen or log and shows it as a card with one button
per choice. When Swarm could not read the question, the app told the owner to answer it in the
pane or to run `swarm attach`. That happened in these cases:

- A question whose top is above the pane, with no log that names it.
- Codex's folder trust screen, which shows before any hook fires.
- A picker that a slash command opens, such as Claude's `/model`. The `/` menu only inserted such
  a command and marked it "opens in pane".
- Two waiting Claude permission hooks for one dialog.

The owner does not use the pane to answer. The pane is only for debugging.

## Decision Outcome

When an agent waits on the owner and Swarm cannot read its question as choices, the listing
gives a **screen card** (`src/screen/screen_prompt.rs`). The prompt has `screen: true`. Its
`question` holds the last 200 rows of the pane. Its choices are `↑`, `↓`, `Enter` and `Esc`, and
each one presses that key. It also takes typed text, which Swarm types and ends with Enter. The
card id is made from the rows without digits, so a clock on the screen does not change it.

An agent waits on the owner when one of these is true:

- It is blocked or done, its CLI's input box is readable, and the screen shows no input box.
  This covers a picker that is open while the agent is idle.
- It is blocked and Swarm cannot read its CLI's input box.

A question that Swarm can read keeps its ordinary card. The screen card is the last fallback.

- The web card shows the rows in a monospace block that opens scrolled to its end, with the four
  key buttons and a text line. The digit keys do not pick a button.
- The HUD cannot show the rows, so its card says "Click to answer in the chat." and has no
  buttons. The inbox marks such a prompt `kind: "screen"`.
- The composer text for a blocked agent is "The agent is asking a question. Answer it above
  first." for the chair and for a child. It no longer names `swarm attach`.
- The `/` menu sends a command that opens a picker, as it sends any other command. The picker
  shows as a screen card. The `dialog` row field is gone.
- When two Claude permission hooks match one dialog, the answer goes as keys to the dialog. It no
  longer fails with "answer it in the terminal".
- A terminal host without a `key` verb, such as Superset, cannot press the keys. A key button
  there shows the adapter error in the chat banner. Typed text still goes through `ring`.

### Consequences

- Good, because no question sends the owner to the pane.
- Bad, because a screen card shows raw terminal rows, which are harder to read than a choice
  list. A better reader for a CLI's dialog still replaces the screen card for that dialog.
