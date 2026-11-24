---
status: accepted
date: 2026-11-24
deciders: [user]
related: ["0130", "0029"]
informed-by:
  - 'owner, 2026-11-24: picked Design C, "Margin notes", from the three chat-annotation mockups.'
---

# 0161. Chat notes are browser drafts sent as one typed message

## Context and Problem Statement

An agent reply often asks for a choice, or holds several points that the owner wants to answer
one at a time. Before this record, the owner had to copy each passage into the composer, quote it
by hand, and write an answer under it. The answers lost their link to the reply text, and a long
answer was easy to lose in the composer.

## Decision Outcome

The owner marks agent reply text and writes a note on it. All notes of a chat go to the agent
as one message, which the app types into the agent like any owner message.

- A note marks one of three things: selected text ("Add note" in the row menu), an item of a
  reply (a paragraph, heading, list item, quote, code block or table), or the whole reply.
  Annotate mode (`chat.annotate`, ⇧⌘A, or the Annotate pill) makes items and the "Whole reply"
  handle take a click, Enter or Space. Esc leaves the mode. The mode never moves a row.
- Each note shows as a numbered card with its quote and a note box. A list at least 1172px wide
  puts the cards in a 280px gutter beside the rows, each at the top of its block. A narrower list
  puts each card right under its block.
- Notes are drafts in the browser only, in `localStorage["swarm.notes.v1"]`, as
  `{ "v": 1, "chats": [[chatId, Note[]], ...] }`. The note box text is part of the draft, so a
  reload keeps it. A saved value of another shape gives no drafts and no error. A chat keeps at
  most 200 notes; the 201st is refused with a notice.
- "Send N notes" sends one message through `App.send`, with this shape:

  ```
  Notes on your reply (2):

  1. > first quote line
     > second quote line
     note text

  2. > [whole reply]
     > reply line 1
     note text
  ```

  The quote is the full marked text, never cut. An item of an ordered list keeps its number. No
  row id goes into the message.
- The drafts go away only after the send goes, also when it goes through the composer banner's
  Retry. A failed send keeps them, and nothing is sent again on its own.
- The server keeps no note state, and the chat shows no app-made "Sent" row (ADR 0130). The agent's
  CLI shows the message as it shows any typed message.
- Child panes show the same transcript (ADR 0029), so notes work there too, under the pane's
  chat id.

### Consequences

- Good, because the owner answers each point next to its text, and the agent gets every answer
  with its quote in one turn.
- Good, because drafts survive a reload and a failed send, with no server change.
- Bad, because drafts stay in one browser; another browser or the HUD does not see them.
- Bad, because a note on text that a fold hides shows its card at the end of the reply, not under
  the text.
- Notes on HTML frames in a reply are a later decision.
