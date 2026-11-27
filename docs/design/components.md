# Components

This file is part of [`DESIGN.md`](../../DESIGN.md) at the repo root.

## Components

- **The glyph** (default) is a hexagon of seven dots. At rest it is the app mark. While an agent works, the lit dots move. When the state is unknown, all dots are unlit.
- **The PR chip** (default) has a 9 px CI mark (check, X or dot, none when unknown). The chip is a link, because the owner wants to open the PR from the sidebar (owner).
- **Keyboard focus is a 2 px ring in the text colour** (default), with a 2 px gap on buttons and inside the row on sidebar rows. It does not use yellow, so yellow keeps its one job.
- **The question card shows number keys 1 to 9 on its choices and a free-text line** (owner), so the owner can answer from the keyboard.
- **In the Changes panel, "Send to agent" is a quiet outlined button with the count** (owner), so yellow stays with the composer's Send.
- **A large diff in the Changes panel ends with the cut line and "Show all"** (owner). Today the app shows 200 lines and says to open the editor.
- **The ⌘J task list is ordered by owner** (owner). The order is the chat, each agent with its tasks, then the shells, because a terminal belongs to one agent.
- **A selected diff range is the `selected` fill plus a 2 px bar in the text colour** (owner), so the cue does not depend on a colour.
- **With every panel open, the chat can get small** (owner). No rule limits the panels at 900 px height.
- **When a branch has more than one open PR, every one of them is reachable** (owner). The chip shows the PR that was updated last and a count, `#195 +2` (owner). The count is its own hit target after a 1 px divider.
- **The other PRs open in a small list** (owner). Hover on the chip for 300 ms, a click on the count, or the Down key on the focused chip opens it. The list is 460 px wide with the menu look, and it scrolls above eight rows. The sidebar row and the branch bar use the same component (owner). A row is the state mark, the number, the cut title, the CI word and the age, and each row is a link.
- **A dialog is the `panel` fill with a 1 px border, a 12 px radius and shadow-2** (owner). The dim layer is black at 50 % in dark and `#1d1b18` at 25 % in light.
- **A switch is neutral, never yellow** (owner). On is a track in the text colour with the knob at the right. Off is a 1 px border track with a muted knob at the left.
- **The one confirming action of a form dialog is the yellow primary button** (owner), such as "Save", because the dialog covers the composer.
- **A destructive action is an outlined button with failed-red text, never yellow** (owner). Examples are "End 3 Chats" and "Delete profile".
- **The usage meter shows what is left** (owner). The fill is the text colour above 20 %, waiting amber at 20 % or less, and failed red at 5 % or less. The percent text repeats the state.
- **A selected palette or menu row has the `menu-hover` fill, and key chords use the 20 px mono key box** (owner).
- **A text field or select has the surface fill, a 1 px border, an 8 px radius and 32 px height** (owner). A settings row has its label and one help line at the left and its control at the right.
- **A letter box is 16 px with an 11 px mono letter** (owner). In the Skills dialog it shows the block kind: `R` rule, `S` step, `P` pack, `F` family. It replaces four dot colours, because colour is kept for agent state. The picker uses the same box for a CLI with no mark.
- **The detail of a selected block opens in place under its card** (owner), as the app does today.
- **"Copy" in the Composer is a quiet outlined button** (owner), because it commits nothing.
- **Run-tree marks are neutral** (owner): a filled dot for the front door, a hollow dot for a step, and a 1 px line. A repeat or offered block has a dashed border.
- **Many background tasks stay on one line** (owner), such as "In background · 3 agents · 17 shells", with a red X and "2 failed". With many tasks, the ⌘J panel groups them by state, failed first (owner). The groups start above 8 tasks (owner).
- **An opened thought is cut at 12 lines, and a long user message is cut at 12 lines** (owner). Each cut follows the cut rule.
- **Many agents from one call are one group card** (owner), with counts by state, failed first, and a cut after five rows.
- **"Needs you" folds after five chats** (owner), with `… N more` and "Show all".
- **A diff that the agent shows on purpose is shown in full, in a window that scrolls** (owner). An example is a chunk in pair programming. The edit card of a tool step keeps the cut. The window is at most 20 lines high (360 px), and the head says how many lines the diff has (owner).
- **Queued messages show the Up key hint** (owner). The Up key on an empty draft pulls every queued message back into the draft, as the app does today. A message has no "Edit" or "Cancel" of its own, because the CLI cannot do that.
- **A failed turn shows the CLI's own error text as one row** (owner). The row has the red X mark, the word "Error", and the text in mono. It has no button, because the chat shows what the CLI shows (ADR 0099). "Retry" and "Dismiss" are only in the app's banner for a failed app action, above the composer.
- **An ended chat shows "Resume" in place of the composer** (owner). "Resume" is the yellow primary button, because no composer shows. It reads "Resuming…" with the disabled look while it runs. The run chip sits at its left and opens the same picker, and "Resume" goes on in the same chat with the picked model. The chip replaces "Continue chat".
- **A usage limit shows in the status line, not in the chat** (owner). The line has the amber dot, "Waiting for the limit · resets 16:00", and the quiet link "Switch account". The app cannot detect a limit today, so this needs new code.
- **A permission request is the question card with a command preview** (owner). The preview follows the cut rule. The choices carry the CLI's own words. The card has no yellow button. The composer is disabled while the card shows unless you were typing a draft when it arrived; that draft stays editable, but Send stays blocked (owner).
- **Choices stay in one column** (owner). Choices 1 to 9 have the key box. A later choice shows its number as muted mono text with no box, because it has no key.
- **A lost connection shows the banner "Reconnecting… Live updates have stopped." with the last-update time** (owner). The banner has no button, because the app tries again each second. The glyph is unlit while the state is unknown, and the sidebar shows "Offline. Showing the last list."
- **A long code block scrolls in the same window as a diff shown for review** (owner). The window is at most 20 lines high, and the head says how many lines the block has. A code block has no cut line.
- **The sidebar head has two icon buttons** (owner). Row 1 is the app mark and "Swarm" as the Home button, then two quiet 24 px icon buttons: Add project ("+") and Settings. Row 2 is "New chat". Row 3 is a 32 px search field with "Search…" and the key box "⌘K". Skills opens from the command palette, and Appearance is a page of Settings, so the head loses those two buttons of today.
- **The sidebar is one list** (owner). It has no "Workspaces" and "Recent" switch. The order is "Needs you", then "Pinned" when something is pinned, then the projects in their fixed order.
- **A project shows only when it has an active chat** (owner). A chat is active when it works, waits, failed, has an unread result, is pinned, or had activity in the last 7 days. The other projects fold into one row at the bottom, such as "4 more projects".
- **Under a project, each active chat is one row** (owner), newest activity first. A chat that works, waits or failed is never folded. After five rows comes the cut line with the count of every chat that is not shown, and "Show all".
- **A worktree with one chat is that one chat row** (owner). A workspace row shows only when one worktree holds two or more active chats.
- **Three kinds of rows leave the list** (owner): a workspace with no chat, the row "N more worktrees", and the fold "Archived · N" under each project. The "+" of a project header opens the composer, which lists the worktrees. One "Archive" button at the foot opens the archive.
- **Search finds every chat, the hidden ones included** (owner), so a hidden chat always has a second way in.
- **One picker holds the model, the thinking level and the account** (owner). It opens from the run chip and with ⇧⌘M. A pick applies at once, so the picker has no "Apply" or "Restart and apply" button. One line at its foot says what the pick does.
- **A switch to another CLI keeps the same chat** (owner). The tab, the sidebar row and the transcript stay. This changes ADR 0110. The app does not ask a question on each switch, because the kept choice applies.
- **One line in the chat shows where the model changed** (owner). It is the one app line in the transcript, so it is an exception to ADR 0099.
- **The picker has a rail and a list** (owner). The rail is 136 px wide and holds "Favorites" and one item per CLI. The list opens on the CLI of the chat. It has a search field, a "Recent" group of up to four models from any CLI, then the models of the CLI. The picker is 560 px wide.
- **Each CLI has its product mark in one colour** (owner). An icon is muted, and it has the text colour on the selected item. The selected rail item and the selected pill have the `menu-selected` fill. A CLI with no mark has the letter box. The other parts have small Lucide line icons. Icons carry no colour, because colour is kept for agent state. The marks are trademarks, so the brand rules of each vendor need a check before code.
- **⌃⌘M goes to the next favourite of this CLI with no menu** (owner). The boards show ⌥⌘M, which macOS uses for "Minimize All". A tooltip above the run chip names the model.
- **A pick is for this chat only** (owner). The link "Use for new chats" on a row sets the model of new chats.
- **The app never changes the model by itself** (owner). A model that the CLI does not list now stays selected with the tag "unavailable".
- **A pick while the agent works does not apply and does not wait** (owner). The picker says that the agent works and offers "Stop". When the turn ends, a neutral banner above the composer offers "Switch now" and "Dismiss".
- **Another CLI gets the whole chat as a file** (owner). A summary is the second choice, and no history is the third. The choice is kept for the next switch. The foot of the picker says what the new CLI does not get: the thinking, the tool state, and the attached files.
- **A usage limit offers "Switch model" beside "Switch account"** (owner). The switch then uses the chat file and does not wait for a summary.
- **The run chip reads account, model, thinking** (owner), such as `work · opus · high`. A CLI with one account shows the CLI name in place of the account, such as `codex · gpt-6.1-terra · high`.
- **The free-text line of a question card reads "Or type your own answer…"** (owner). The app says "Type your answer" today.
- **The diff head hides "Wrap" while "Unified" is on** (owner), as the app does today.
- **A list with no active chat says "No chat was active in the last 7 days."** (owner).
- **The sidebar head and the small line icons use the Lucide set** (owner), which the app uses today.
- **A banner has two forms** (owner, as drawn in round 16). The connection banner is a 28 px row with a 10 % amber fill and a 1 px amber rule under it. The banner of an app action sits above the composer with a 1 px border, an 8 px radius, 6 px padding, mono 12 px text, and its buttons at the right. A failed action has the failed-red border and an alert icon. A neutral one has the `menu-border` border and no icon.
- **While the picker is open on a working chat, "Stop" shows in the picker foot and on the status line** (owner).
- **An age is written as the app writes it today** (owner), in seconds, minutes, hours or days, such as "9d" and "400d". There is no week or year form.

