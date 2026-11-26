# Foundations

This file is part of [`DESIGN.md`](../../DESIGN.md) at the repo root.

## Colors

- **Colour is kept for agent state** (default). Then a colour always means a state, so one glance at the sidebar is enough.
- **Yellow `#fcd452` has one job, the primary action** (default). The lit glyph dots are its only other use. Then the main action is easy to find.
- **In the light theme, the lit dots are `#a37a00`** (default), because yellow is too light on cream. It measures 3.71:1.
- **Light state colours are the darker set** (owner). Each passes 4.5:1 on surface, panel and the selected row, as measured in round 8.
- **A running item shows the glyph, not blue** (owner). A running child agent in a dense list also shows the 12 px glyph, because a blue dot and the amber dot would differ only by colour. The blue token has no use for now.
- **State is never colour alone** (default). Each state also has a shape, because some people do not see colour differences.
- **Diff bands stay at low opacity** (default), so a diff stays quiet.

## Role to CSS variable

The design names a role, and `web/src/styles-base.css` holds the CSS variable. The names cross once: design `surface` is `--background`, and design `panel` is `--surface`.

| Design role | CSS variable | Tailwind utility |
|---|---|---|
| surface | `--background`, `--sidebar` | `bg-background`, `bg-sidebar` |
| panel | `--surface`, `--code` | `bg-surface`, `bg-code` |
| selected | `--selected` | `bg-selected`, `bg-accent` |
| row-hover | `--hover` | `bg-hover` |
| border | `--border` | `border-border` |
| on-surface | `--foreground` | `text-foreground` |
| muted | `--muted-foreground` | `text-muted-foreground` |
| disabled-text | `--disabled-foreground` | `text-disabled-foreground` |
| primary | `--brand` | `bg-brand` |
| on-primary | `--brand-foreground` | `text-brand-foreground` |
| primary-hover, primary-pressed | `--brand-hover`, `--brand-pressed` | `bg-brand-hover`, `bg-brand-pressed` |
| primary-disabled | `--brand-disabled` | `bg-brand-disabled` |
| quiet-hover, quiet-pressed | `--quiet-hover`, `--quiet-pressed` | `bg-quiet-hover`, `bg-quiet-pressed` |
| focus ring (text colour) | `--ring` | `ring-ring` |
| state-failed, state-waiting, state-done, state-working, state-ended | `--status-failed`, `--status-waiting`, `--status-done`, `--status-working`, `--status-ended` | `text-failed`, `text-waiting`, `text-done`, `text-working`, `text-ended` |
| glyph-lit, glyph-unlit | `--glyph-lit`, `--glyph-unlit` | none (SVG reads `var()`) |
| pr-quiet-text, pr-draft-border | `--pr-quiet`, `--pr-draft-border` | none |
| menu | `--popover` | `bg-popover` |
| menu-border | `--menu-border` | `border-menu-border` |
| menu-hover, menu-selected | `--menu-hover`, `--menu-selected` | `bg-menu-hover`, `bg-menu-selected` |
| tooltip | `--tooltip` | `bg-tooltip` |
| dim layer | `--overlay` | `bg-overlay` |
| shadow-1, shadow-2 | `--shadow-1`, `--shadow-2` | `shadow-1`, `shadow-2` |

## Typography

- **DM Sans is the interface face, and JetBrains Mono is for code, commands, branches, chips and timers** (default). Two faces split words from machine text.
- **Instrument Serif appears only as the question on the new-chat screen** (owner). It marks the start of new work.
- **Every size comes from the scale** (default), because today 34 raw sizes skip it. The scale is 11, 12, 13, 14 and 15 px, plus 44 px for the serif question. The 10 px size of today is dropped (owner).

- **Reply text in the chat is 16 px with a 1.5 line** (owner). Its first two heading levels are 1.3 times that size, levels 3 and 4 are 1.12 times, and levels 5 and 6 use the reply text size. Its inline code is 0.875 times the text around it, which gives 14 px in a paragraph. The code of a fenced block that is not a diff is 12.25 px, because that block is set to 14 px with a 1.45 line and the same 0.875 applies. A reading trial found 13 px too small for long text. All other text follows the type scale.

## Layout

- **Sidebar rows follow "way 4"** (owner). Line 2 ends with the PR chip. Only the selected row adds a third line with the branch and diff stat. Other rows stay two lines high, so the list stays calm.
- **The open chat has the selected fill and a 3 px bar in the text colour** (owner), so it differs from a chat that is marked with ⌘-click or Shift-click.
- **"Needs you" is a plain group at the top, with a count badge and an amber dot on each row** (owner). It comes first, because those chats are blocked on the owner.
- **The child badge "3 · 1 waiting" sits at the right end of the title line** (owner), so a parent shows its children's state at a glance.
- **A thin 36 px branch bar sits under the 40 px tab strip** (owner), so the open chat's branch and PR stay in view.
- **There are no tab groups for now** (owner), because the owner said "not now".
- **Above the composer there are two lines** (owner for the second). The background line ("In background · 2 agents · 1 shell") stays outside the chat log, because background work is not part of the conversation.
- **The layout does not move while text streams** (default). A live thought shows a fixed 4-line window, because a person cannot read text that jumps. The window has no border box (owner), because a boxed thought looks heavy.
- **In a narrow window the sidebar becomes a 52 px rail of state marks** (owner), so the chat keeps its width. The rail holds the app mark, a "+", and one state mark per chat. The rail starts under a window width of 1024 px (owner), because the sidebar and the chat column need 264 px and 760 px. The app has no rail today, so this is new behaviour.
- **Spacing uses the 4 px step** (default). Whether to round the board values 6 and 10 px is open (open).

## Elevation & Depth

- **Depth comes from tonal fills and 1 px borders, not shadows** (default). Flat layers keep the surface quiet.
- **Menus and tooltips use their own fill and a 1 px border** (default). The shadow scale has two steps, shadow-1 for a tooltip and a toast and shadow-2 for a menu, a popover and a dialog.

- **A pane divider and the search box border take the muted text colour on hover** (owner), because a 1 px drag target needs a cue.

## Shapes

- **Radii run 4, 6, 8, 10, 12 px, plus full for badges** (default). Small parts get small radii, so the parts look like one family. The 14 px step of today is dropped (owner).

## Motion & Micro-interaction

- **The seven-dot glyph is the one moving element** (default). Then motion always means "an agent works".
- **No motion on frequent actions** (default). Hover, focus and keys act at once, because motion there feels slow.
- **Other motion stays under 300 ms with ease-out, and can be interrupted** (default), because a dense app has few large moves. The tokens use the Material 3 short durations.
- **Working text shows after 150 to 300 ms and stays at least 300 to 500 ms** (default), so a short task does not flash.
- **With `prefers-reduced-motion`, nothing animates, the glyph included** (default). The still form is the working form with three lit dots, and the moving form is the wave pattern at 1.8 s per loop (owner).

