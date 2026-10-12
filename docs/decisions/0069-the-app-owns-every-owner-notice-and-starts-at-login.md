---
status: accepted
date: 2026-10-12
deciders: [user]
supersedes: "0058"
related: ["0044", "0068"]
informed-by:
  - "Owner request on 2026-10-12: the app owns every notice and a click lands on a pane or reports that it is gone"
---

# 0069. The app owns every owner notice and starts at login

## Context and Problem Statement

With Swarm stopped, osascript posts a notice that has no click action. With Swarm running,
a failed pane focus still selects a chat. The owner needs each click to reach a host pane,
or to show why it cannot.

## Considered Options

- Keep osascript when the app is stopped and chat selection when focus fails.
- Keep the app running and let it post every notice, with a pane focus or an error notice.

## Decision Outcome

We chose the app path because only the running app has a notification delegate that owns a click.
Swarm registers `SMAppService.mainApp` on launch when the `startAtLogin` notice preference is on.
The switch defaults to on and is shown in Settings › Notifications. The app keeps running with
all windows closed and adds no menu-bar extra.

`swarm notify` inserts a `notice` store row with title, body, and chair ids when known.
`swarm notices --json` reads unposted rows and marks them posted in one transaction.
The app reads these rows on refresh and applies its post, mute, sound, and done preferences.
A row with no known agent state is a done notice. Hooks and screen checks write the waiting
state; the app derives the needs-input notice from that change.

If the app does not hold `APP_LOCK`, explicit notices and waiting changes spawn detached
`open -g -b io.github.priyanshuupadhyay.swarm`. They never wait on `open`, so the hook keeps its
stdin and store budgets. No adapter posts an osascript notice.

A click runs `swarm focus` with the stored session and agent ids. A missing or failed agent pane
gets one retry on the session chair pane. If both fail, the command exits 1 with `swarm: pane closed`.
The app posts one notice titled "Swarm" with the error text. It never selects a chat.
An error notice has no click action, so it cannot repeat the failed focus.

After focus succeeds, Herdr and tmux can report their client pid. Swarm walks its parents to
an `.app/Contents/MacOS/` process and opens that app. With several Ghostty windows, the last
focused window rises; a per-window choice would need accessibility access.
Superset and tmux-solo still lack focus and client verbs. Their click shows the `has no focus` error.

### Consequences

- Each notice has one app owner and uses the owner's notice settings.
- A click reaches the agent pane, the chair pane, or an error notice.
- Login registration needs a signed app and may need consent in System Settings.
- The store marks a notice before macOS posts it, so an app crash or a denied permission can lose that notice.
- An unsupported host still cannot focus a pane.
