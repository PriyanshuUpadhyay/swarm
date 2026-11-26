# Voice, rules and open points

This file is part of [`DESIGN.md`](../../DESIGN.md) at the repo root.

## Voice & Content Strategy

- **Working text names the work, never a bare spinner** (default). An example is "Running the full test suite · 0:42". The owner must know what each agent does.
- **The waiting state has one name, `waiting`** (default). Today the code says `blocked`, so the two names drift apart.
- **"Needs you" differs from "state changed"** (owner for the group, default for the rule). A usage limit keeps its amber dot in its project group. Alerts lose meaning when they come too often.
- **A cut always says how much is left out** (default). The form is one quiet mono line, `… +N lines` or `… N more`, then "Show all". Nothing is hidden without it, because the owner needs the evidence.
- **The tone is plain, short and second person, and labels are verb phrases in sentence case** (default), such as "Allow once". The app already writes this way.
- **An error says what failed** (default). A CLI error shows in the CLI's own words, for example "API Error: 529 Overloaded · after 3 retries". An error of the app also offers a next step, such as "Retry".
- **Status parts join with " · ", and timers use `m:ss`** (default), so lines stay short.

## Do's and Don'ts

- Do use yellow for the one primary action. Don't use it for links, focus or highlights.
- Do mark state with shape and colour. Don't use colour for decoration.
- Do say how much a cut leaves out. Don't fold or hide content silently.
- Do keep a running item's place fixed. Don't let streamed text push the layout.
- Do name the work in the status line. Don't show a bare spinner.
- Do put only chats that wait for an answer in "Needs you". Don't put state changes there.
- Do keep background work on its own line. Don't write it into the chat log.
- Do change hover and focus at once. Don't animate frequent actions.
- Do use one name, `waiting`. Don't use `blocked` for the same state.
- Do keep the full title in the sidebar row. Don't cut a title to make room for a chip.

## Open points

- In the light theme the draft and merged chip text equals the muted text (owner accepts this), so those chips differ from an open chip only by the dashed border and the fill.
- Panes have four undecided points. The settings pages Notifications, API keys, and Setup & doctor are not drawn.
- The Mac HUD, the app icon and the sound have no rules yet.
- The boards do not test motion. No person has viewed the glyph motion page in a browser.
- Two rules need an ADR change before code: the same chat on another CLI (ADR 0110) and the line where the model changed (ADR 0099). The one-list sidebar also changes ADR 0142, because the row "N more worktrees" leaves the list.
- The picker and the model switch are confirmed (rounds 17, 20 and 21). The vendor marks need a trademark check before code. The codex mark is weak at 16 px. In light, the selected pill has one colour for its fill and its border. ⇧⌘M is not checked against browser keys.
