---
status: accepted
date: 2026-11-25
deciders: [user]
supersedes: "0161"
related: ["0161", "0029", "0030"]
informed-by:
  - 'owner, 2026-11-25: picked "all notes go to the main chat (the orchestrator), each note names the child agent and quotes its text".'
  - 'owner, 2026-11-25: "send should not send directly; paste in the main chat''s input; if the input has text ask to clear or append, with an option to copy and dismiss".'
  - 'owner, 2026-11-25: "when notes are added to the message, annotate mode stops".'
  - 'owner, 2026-11-25: "make the notes float with absolute and relative position; the current setup messes up scroll".'
  - 'owner, 2026-11-25: picked collapsed badges for narrow lists.'
---

# 0162. Chat notes go to the main chat's message box and float beside the rows

## Context and Problem Statement

ADR 0161 lets the owner mark agent reply text, write a note on it, and send all notes of a chat
as one typed message. Use showed three problems. A note made in a child agent's column went to
that child, under the pane's chat id, but the owner wants the main chat (the orchestrator) to get
every note. "Send N notes" sent at once, so the owner could not edit the message first. The cards
sat inside the rows, so a card changed a row's height and the list jumped while the owner
scrolled.

## Decision Outcome

The owner marks agent reply text and writes a note on it. All notes belong to the main chat. The
app puts them as one message in the main chat's message box, and the owner reviews and sends it.
The cards float beside the rows and never move them.

### Marking and annotate mode

- A note marks one of three things: selected text ("Add note" in the row menu), an item of a
  reply (a paragraph, heading, list item, quote, code block or table), or the whole reply.
- Annotate mode (`chat.annotate`, ⇧⌘A, or the Annotate pill) makes items and the "Whole reply"
  handle take a click, Enter or Space. Esc leaves the mode. The mode never moves a row.
- Each note shows as a numbered card with its quote and a note box.

### Notes belong to the main chat

- Notes are keyed by the main chat's session id. A note made in a child agent's column stores
  `agent: "<child id>"` in its Note.
- A child's column shows only its own cards. Card numbers follow the main chat's order, so a
  child's cards can show, for example, 2 and 4.
- The main chat's bar counts all notes. A child column with notes shows the bar as
  "N notes for the main chat".

### Saved drafts

- Notes are drafts in the browser only, in `localStorage["swarm.notes.v1"]`, as
  `{ "v": 1, "chats": [[chatId, Note[]], ...] }`. The shape stays v1; `agent` is optional in a
  Note.
- The note box text is part of the draft, so a reload keeps it. A saved value of another shape
  gives no drafts and no error.
- A chat keeps at most 200 notes; the 201st is refused with a notice.
- The server keeps no note state, and the chat shows no app-made "Sent" row (ADR 0130).

### The message

- When no note names an agent, the message has the 0161 shape, with the header
  `Notes on your reply (N):`.
- When any note names an agent, the header is
  `Notes on replies (N); [name] marks a child agent's reply, no name marks yours:`. A child
  note's first quote line starts with `[agent] `. Example, with one note from the child
  `reviewer` and one on the main chat's reply:

  ```
  Notes on replies (2); [name] marks a child agent's reply, no name marks yours:

  1. [reviewer] > [whole reply]
     > reply line 1
     > reply line 2
     note text

  2. > first quote line
     > second quote line
     note text
  ```

- The quote is the full marked text, never cut. An item of an ordered list keeps its number. No
  row id goes into the message.

### Add to message, not send

- The bar's button "Add N notes to message" puts the message in the main chat's message box.
  Nothing is sent. The owner reviews the text and sends it.
- If the box is empty, the text goes in and the box takes the focus.
- If the box has text, a dialog "The message box has text" asks what to do:
  - Append (the default): the notes go under the text, after a blank line.
  - Replace Text: the old text goes to the box's stash, as Reuse does, and the notes go in.
  - Copy and Dismiss: the app copies the message and removes the notes.
  - Cancel: nothing changes.
- After Append, Replace Text or Copy and Dismiss, the notes are removed and annotate mode ends.
- The 0161 rules for a failed send and for Retry are gone, with `Banner.onSent` and
  `Input.sendingFor`. The app does not send, so no send can fail.

### Layout

- All cards of a column sit in one `.note-layer`. The layer is absolute inside the scrolling list
  (`.transcript` is `position: relative`) and outside the rows. A card never changes a row's
  height, so the list never jumps.
- `placeCards` sets each card's top to its anchor: the marked block, or the row for a whole reply
  or for a passage that a fold hides. It pushes a card down to clear the card above, with an 8px
  gap.
- It places the cards again when a row mounts again (a MutationObserver) and when the rows or
  cards change size (a ResizeObserver).
- Wide list (at least `--wide-notes`, 1172px, with notes): the rows keep 820px on the left, and
  the cards fill a 280px gutter.
- Narrow list: a card shows only its number badge on the top-right corner of its block, in place
  of the block's corner badge. The card opens over the text while it holds the focus. A click on
  the number puts the caret in the note box. The card closes when the focus leaves.
- A card whose row is not mounted keeps its last place, or stays hidden until its row first
  shows.

### Consequences

- Good, because notes made in child panes reach the orchestrator, each with its source.
- Good, because the owner can edit the message before sending it.
- Good, because a card never moves a row, so the list does not jump on scroll.
- Bad, because drafts saved before this change under a child pane's id (`"<session>:<agent>"`) no
  longer show. This is at most one day of drafts, and there is no migration.
- Bad, because an open narrow card covers reply text while it has the focus.
- Bad, because cards on the last row can make the list's scroll height larger.
- Bad, because a card on an unmounted row shows at its old place until the row mounts.
- Bad, as in 0161, because drafts stay in one browser; another browser or the HUD does not see
  them.
- Notes on HTML frames in a reply are still a later decision.
