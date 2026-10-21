---
status: accepted
date: 2026-10-21
deciders: [user]
amends: ["0069", "0077"]
related: ["0084", "0088"]
informed-by:
  - 'owner, 2026-10-21: "need to fix both b1 and b2. Also if we need additional permission the app should hand hold the user in doing that successfully. the app should take the user to the setting"'
  - "council 2026-10-21 swarm-notices-focus, B1 split 2-1 (~/dotfiles/docs/council/2026-10-21-swarm-notices-focus.md)"
  - "RCA 2026-10-21: notices were posted, and NotificationCenter logged each one as muted by DND; a click on an app chat's notice ran `swarm focus`, which failed with `tmux-swarm has no focus`"
---

# 0102. A notice click opens the app chat, and the app guides notice permission

## Context and Problem Statement

App chats run on the `tmux-swarm` adapter (ADR 0084), which has no focus verb. A click on their
notice ran `swarm focus`, failed with `swarm: tmux-swarm has no focus`, and posted that error as a
new notice (ADR 0069 accepted this for tmux-solo). Separately, a Focus such as Do Not Disturb held
every notice, and nothing in the app told the owner or showed the way out.

## Considered Options

- Click: a `focus` line in `adapters/tmux-swarm.conf` that runs `swarm ui`, or a Rust branch in
  `swarm focus` that opens the app on the chat.
- Permission: advice text in `swarm doctor` only, or a card in the app that reads the state, shows
  the steps, and opens the System Settings pane.

## Decision Outcome

Chosen: a Rust branch and an app card, because the branch keeps a real `open` failure visible and
not mapped to `pane closed`, and the card meets the owner where they work.

- `swarm focus <session> <agent>` on `tmux-swarm` opens Swarm.app on that chat. An open page gets
  a `show` frame through `POST /api/show {"session"}` and opens the chat; with no open page, the
  app starts on `/chat/<id>`. The session id must be 1-64 of `[A-Za-z0-9-]`. A failed `open` is
  an error for a click; `swarm ui` keeps going as before. The click prints no URL, so the token
  stays off its output.
- The SwarmNotify helper gains `--status` and `--authorize`, which print the helper's notice
  settings as one JSON line. It holds no rule (ADR 0077).
- `swarm notices health --json` owns the rule: `ok`, `allow` (macOS has not asked), `blocked`
  (denied, or alert style None), `held` (the unified log shows the newest presented notice muted by
  DND), `missing`, or `unknown`, with steps and one `action`. More states may come. A failed read,
  or a value swarm does not know, is `unknown`, never `ok`. `swarm doctor`'s notices row reads it.
  A test notice skips the done switch, because it checks delivery.
- `swarm notices allow|test --json` and `swarm notices settings <notifications|focus>` run the
  fixes; the settings URLs are fixed in Rust. `GET|POST /api/notices` expose them to the app.
- Home checks on start and when the window comes back, at most once each 30 s, and shows a card
  for `allow`, `blocked` and `held` with numbered steps and the button. Only Home shows it, so a
  chat never moves. After a fix makes the state `ok`, the card says notices work; a fix that ends
  `unknown` keeps the card with its reason. Not now hides it until the page reloads.
- A failed health read shows no card; Settings › Setup shows its reason (owner, 2026-10-21).

### Consequences

- Good: a click on an app chat's notice lands on that chat, not on an error notice.
- Good: the owner sees why notices are quiet and reaches the right pane in one click.
- Bad: `held` reads the unified log, whose message text macOS may change; then the state is `ok`
  or `unknown`, not wrong advice.
- Bad: the click selects the chat but not one agent column inside it.
- Bad: a Focus that is on reads `ok` until it holds a swarm notice, because macOS does not tell
  another app whether a Focus is on. The owner accepted this and needs no Focus bypass
  (2026-10-21: "dnd bypass is not needed").
- Bad: time-sensitive notices, which pass a Focus, still need a paid signing identity.

## Links

- Amends: 0069 (tmux-solo click error), 0077 (click error notice for app chats). Related: 0084, 0088.
