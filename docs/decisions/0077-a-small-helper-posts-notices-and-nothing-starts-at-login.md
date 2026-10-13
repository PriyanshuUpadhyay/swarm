---
status: proposed
date: 2026-10-13
deciders: [user]
amends: "0069"
related: ["0044", "0057", "0068", "0069", "0075", "0076"]
informed-by:
  - 'owner, 2026-10-13: "if it needs swarm to be availabel it should not start the big ass app for notify no?.. it eats up a lot of battery and cpu"'
  - "tmp/web-port/brief-s6-natives.md (S6, Scope v1 item 12)"
---

# 0077. A small helper posts notices, and nothing starts at login

## Context and Problem Statement

ADR 0069 made the app own every notice. When the app does not hold `app.lock`, `swarm hook` and
`swarm notify` run `open -g` on the full Swarm.app, so each waiting change can start the whole UI.
The owner said this uses too much battery and CPU. The web shell (0075) also needs a notice owner,
a Dock badge, a folder picker, and a decision on the login item.

## Considered Options

- Keep 0069: start the full app (or the web shell) for a notice.
- A headless mode of the web shell: the same app with no window.
- A small helper app inside Swarm.app that posts notices and exits when idle.

## Decision Outcome

Chosen: the small helper, because it has no window, no web view and no `swarm serve`, and it runs
only while a notice is pending or a click is handled.

- `SwarmNotify.app` lives at `Swarm.app/Contents/Helpers/SwarmNotify.app`, next to the bundled
  `swarm`. Its bundle id is `io.github.priyanshuupadhyay.swarm.notify`, and `LSUIElement` keeps it
  out of the Dock. It is signed with the app.
- The needs-input notice becomes a store row. At a change to waiting (hook or screen read),
  `swarm` inserts a `notice` row with the session, the agent, and a body from the workspace folder
  name and the state kind. `swarm notify` inserts its row as before. So every notice is a row, and
  no notice is derived from a tree diff in a UI.
- After the row, `swarm` starts the helper. It runs
  `open -g <helper> --env SWARM_HOME=<home> --env SWARM_BIN=<this swarm>` detached, and never
  waits. The helper path is `SWARM_NOTIFY_APP` if set, else `SwarmNotify.app` next to the running
  `swarm`, else the one in `/Applications/Swarm.app`. If none exists, nothing starts and the row
  waits. There is no `open -b` fallback, because LaunchServices can pick any dev build.
- If the helper runs already, `open` sends it a reopen event, and it drains again. The helper
  does not poll.
- The helper runs `swarm notices --due`. Rust takes the unposted rows and applies the notice
  preferences: `post`, `mutedProjects`, `done` (a row whose agent is not waiting is a done notice)
  and `sound`. It prints only the rows to post. The helper posts them with
  UNUserNotificationCenter. The rule lives in Rust, so the helper has no domain logic.
- Each posted notice carries the session id, the agent id, the swarm path and the home in its
  user info. A click runs `swarm focus <session> <agent>` with that home. macOS starts the helper
  for a click if it has exited. A failed focus posts one "Swarm" notice with the error text and
  no click action, as in 0069.
- The helper exits 20 s after its last drain or click.
- The notice preferences stay in `choices.json` under `prefs.notices` in the swarm home (0057).
  The SwiftUI app, `swarm serve` and `swarm notices --due` read the same file, and the web
  settings (S5) write it through `/api/choices`.
- Nothing starts at login. The helper starts on demand, so the login item has no job.
  `startAtLogin` is no longer read by the shell; the SwiftUI app keeps its switch until cutover.
- The web shell, when open, keeps the Dock badge. It listens to `/api/stream` and reads
  `/api/badge` on each snapshot frame. Rust counts the chats that wait, or returns 0 when
  `prefs.notices.badge` is off.
- The web shell gives the page one script message, `swarm`, with the shape
  `{"kind": "pickFolder"}`. It shows an open panel for one folder and replies with the path or
  null. Any other shape is refused.
- Until cutover, the SwiftUI app still drains rows while it runs. It stops deriving needs-input
  notices from its tree, because the row now carries them; otherwise each one would post twice.

## Consequences

- Good: a notice starts only a small helper, and nothing runs while no notice is pending.
- Good: every notice is one row with one Rust rule, so a test proves what posts.
- Bad: the helper has its own bundle id, so macOS asks once for notice permission for it.
- Bad: a click on an old notice starts whichever helper LaunchServices registered for the bundle
  id. The user info names the swarm and home, so the focus still reaches the right store.
- Bad: one helper instance serves one home at a time. A second home's rows wait until the next
  start after the helper exits.
- Bad: at cutover the done notice from a tree change ends. A chair still sends `swarm notify`
  when it is done (0044).
- The waiting body names the workspace folder, not the chat title, because the hook has a 3 s
  budget and no tree.
