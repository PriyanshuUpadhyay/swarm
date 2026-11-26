# Rules that go with the Swarm design, direction B

This file goes with `DESIGN.md` at the repository root. `DESIGN.md` holds the values and the rules of the look.
This file holds the rules for work with the design: which file wins, how a board is made and reviewed, what to
check before code, and where each board is.

Written 2026-11-26.

## 1. Which file is the design

- `DESIGN.md` is the design.
- **The design file wins over a board.** A board is a picture of a rule at one moment. Section 5 lists the
  places where a board differs from the file.

## 2. How to read the marks

Each value and each rule in `DESIGN.md` has one mark.

- `owner`: the owner said it or approved the board that shows it.
- `default`: the chair chose it and a board shows it. The owner did not confirm it by name.
- `open`: nobody decided it.

No comment is not a yes. A point with no answer stays `default` until the owner answers.
An answer such as "looks good" applies only to the board that the last reply was about.

## 3. Rules for a mockup board

These rules came from the owner during the run. They apply to each later board.

1. **A board does not invent behaviour.** Before a board for a screen that exists, a read-only survey lists what
   the app does today, with the exact visible strings and `path:line` references. The board uses those strings.
   Each invented string is listed in the board's result list.
2. **If the code contradicts the brief on today's behaviour, the board shows the code.** The difference goes in
   result list.
3. **Each point that needs the owner's eye has a marker.** The marker is a big red arrow with a number on the
   canvas, and a layer named `EYE n · <frame> · <tag>`. An index card "Needs your eye · N points" is at the top
   left. The marker red `#ff2d55` is not a product colour, and a marker covers no content.
4. **Each frame has a legend** with a "Today:" line and a "Proposal:" line.
5. **A cut always says how much is left out.** The form is `… +N lines` or `… N more`, then "Show all".
   A diff that is shown on purpose and a long code block are never cut. They scroll in a window.
6. **A board uses worst-case data, not filler.** Check the empty case, the case with one item, and the case with
   very many items. A value with no limit is a finding.
7. **The chat shows what the agent CLI shows (ADR 0099).** The app adds no row of its own to the transcript.
   App state goes in the status line, a banner, or a card. The one exception is the line where the model changed.
8. **Colour is kept for agent state.** Icons, marks and letter boxes have one colour.
9. **A vendor mark comes from a named icon package.** Nobody draws one by hand. The packages used are
   `simple-icons` and `@lobehub/icons-static-svg` 1.106.0.
10. **The exported PNG is the check that counts.** Cut text with no "…", a wrong arrow tip, a wrong mark, or
    marker red in a frame PNG is a fault.
11. **A reply about a board names the place to look:** the page, the frame, and the `EYE` layer.

## 4. Checks before code

- **Trademarks:** the CLI marks (Claude, Codex, Antigravity, Pi) are trademarks. The icon package is MIT for its
  files, but that does not cover the brand rules of each vendor.
- **ADR 0110:** a switch to another CLI keeps the same chat. The ADR says that it starts a new chat.
- **ADR 0099:** the line where the model changed is an app line in the transcript.
- **ADR 0142:** the one-list sidebar removes the row "N more worktrees".
- **Keys:** ⇧⌘M (open the picker) is not checked against browser keys. ⌃⌘M (next favourite) is free in the app.
- **New code that the design needs:** the 52 px rail, the usage limit in the status line, the "Needs you" group,
  the PR data for the chip (URL, draft and merged state, more than one PR), favourites and recent models.
- **Motion:** no board tests motion. Nobody has viewed `mockups-r8/glyph-motion.html` in a browser.
- **Naming:** the code says `blocked` and the design says `waiting`.

## 5. The boards

OpenPencil opens the `.fig` files. The files are in `docs/design/boards/`; the README there says what each shows.
A board in this list is the final one for its area. The names `r10`, `r19` and so on in the text below are the
round numbers of the design run; the file names are the ones in the table.

### The final board for each area

| Area | File |
|---|---|
| Chat, new chat, values of direction B | `docs/design/boards/swarm-b-settled-eye.fig` |
| Light values, shadows, glyph | `docs/design/boards/swarm-b-values.fig` |
| Changes panel, PR tab, ⌘J panel | `docs/design/boards/swarm-b-panels-eye.fig` |
| Settings, command palette, dialogs, thinking | `docs/design/boards/swarm-b-other-screens.fig` |
| More than one PR | `docs/design/boards/swarm-b-more-prs.fig` |
| Skills dialog | `docs/design/boards/swarm-b-skills.fig` |
| Edge cases | `docs/design/boards/swarm-b-edge-cases-eye.fig`, `docs/design/boards/swarm-b-edge-cases-2-eye.fig` |
| Shown diff, queued messages | `docs/design/boards/swarm-b-diff-and-queue.fig` |
| Nine edge cases, drawn again | `docs/design/boards/swarm-b-open-nine.fig` |
| Sidebar | `docs/design/boards/swarm-b-sidebar-one-list.fig` |
| Model picker and model switch | `docs/design/boards/swarm-b-model-switch-2.fig` |
| Small values | `docs/design/boards/swarm-b-small-values.fig` |

### Boards that are replaced

- Round 17 (model switch) is replaced by round 20.
- Round 18 (sidebar views) is replaced by round 19.
- The rounds before 10 are earlier steps. Rounds 5 and 6 still hold the only drawings of the PR chip states, the
  tab menu, the slash menu, the todo and plan rows, and the toasts.
- The replaced and earlier boards (also rounds 5 and 6) are not in the repository.

### Where a board differs from the design file

The design file is correct in each case.

- Rounds 1 to 4 and the round 14 frames show one title bar. The app and the later boards have a tab strip.
- Each board before round 16 shows the old sidebar head. The rule is sidebar head B.
- No board before round 19 shows the one-list sidebar.
- On the two edge-case boards of round 14, eight frames are replaced by rounds 15 and 16: the failed turn, the
  ended chat, the usage limit, the permission card, eleven choices, the lost connection, the cut code block, and
  the queued "Edit" and "Cancel".
- Round 7 shows the old light chip values. Round 8 has the correct values.
- Round 15 shows "Wrap" with "Unified" on. The rule hides it.
- Round 19 writes ages as "2 w". The rule is the app's form, such as "21d". Three texts are cut with no "…".
- Round 20 shows ⌥⌘M. The rule is ⌃⌘M. The codex chip reads `default · gpt-6.1-terra · high`. The rule is the
  CLI name. Codex has a letter box "X". The rule is the product mark. The selected fill is `#383530`. The rule is
  `#45413a`.
- Round 21, tile 5: the label of choice C says "months", but its values use weeks.
- Chat names, PR titles, account names, model notes and many small texts on the boards are invented. Each
  board's result list names them.

## 6. Not designed yet

- Panes have four open points: a failed child has no look of its own, no rule orders the panes, no view shows
  all panes, and nothing brings a pane into view when its question arrives.
- The Settings pages Notifications, API keys, and Setup & doctor are not drawn.
- The Mac HUD, the app icon and the sound have no rules.
- A PR chip count with two or three digits and a run of more than 24 hours are not drawn.

## 7. Records

- The owner's words that led to the design are quoted in ADR 0164 to 0171 (`docs/decisions/`).
- What the app does today, before the design, is in the code and in those records.
