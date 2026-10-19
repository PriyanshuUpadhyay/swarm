---
status: accepted
date: 2026-10-19
deciders: [user]
amends: ["0079", "0088"]
related: ["0075", "0078", "0087"]
informed-by:
  - 'owner, 2026-10-19: "when in full screen only the swarm app window can show the background around the notch? right now it''s all black"'
  - 'owner, 2026-10-19: "also can it always open in full screen and when swarm ui is called it should open and focus the window or if already opened focus the window"'
  - 'owner, 2026-10-19: "option b" (a thin native shell, over driving the browser window through Accessibility from SwarmHUD)'
  - 'owner, 2026-10-19: "hud dino works... no badge" (icon options: the web icon, the HUD dino, a live icon; badge options: waiting count, count with an alert, a dot)'
  - "/tmp/swarm-app-test window list, 2026-10-19: full-screen window 1800x1130 at y=39 below a 38 pt camera row; the strip panel at y=0 only after constrainFrameRect returns its rect"
---

# 0095. swarm ui opens Swarm.app, a full-screen WKWebView shell

## Context and Problem Statement

`swarm ui` opened the page as an app window of the default Chromium browser (ADR 0079, ADR 0088).
The owner wants three things that a page cannot do: start in full screen, bring the window to the
front when `swarm ui` runs again, and show the page's background in the camera row above a
full-screen window. macOS keeps that row black for every full-screen window. Only a separate
window can paint it, and only the process that owns the window knows when it is full screen.

## Considered Options

- A thin native shell, `Swarm.app`, with one WKWebView.
- Keep the browser window, and let SwarmHUD find it through the Accessibility API, raise it, set it
  to full screen, and paint the row.
- Keep the browser window as it is.

## Decision Outcome

Chosen: the thin shell, because it owns its window, so it needs no permission, no window-title
match, and no running HUD. The Accessibility path needs a permission that macOS can drop after
each ad hoc signed upgrade, and works only while the opt-in HUD runs.

- `SwarmApp` ships at `libexec/Swarm.app` (bundle id `io.github.priyanshuupadhyay.swarm`, ad hoc
  signed). It shows the `swarm ui` URL from its first argument in one WKWebView. It does not start
  or own the server; `swarm ui` still does (ADR 0088).
- `swarm ui` runs `open -a Swarm.app --args <url>` after it reloads the open pages, also when a
  page answered, because that `open` brings a running app to the front. `SWARM_APP` points it at
  another build. With no app beside `swarm`, it opens the browser as before.
- The window goes full screen at the app's first activation. macOS can refuse to activate an app
  that a background process starts, and full screen needs the active app.
- In full screen on a notched screen, a borderless child panel covers the camera row in the page's
  `underPageBackgroundColor`, so it follows the theme. It overrides `constrainFrameRect`, because
  AppKit moves a window out of the menu-bar row. It goes away when full screen ends.
- The shell keeps only what the page cannot do: the Edit menu for text keys, ⌃⌘F, and opening
  `target="_blank"` links in the default browser. The page keeps its own key map.
- The icon is the HUD's Deno guest (vscode-pets, MIT, already in `MASCOT-NOTICE`), its still idle
  frame on `#101010` in a rounded square. `make package` draws it with `SwarmHUD --app-icon` and
  `iconutil`, so the art has one source. The app shows no Dock badge; the HUD and the notices
  already say when an agent waits.
- This amends ADR 0079's "no app bundle needs a window". The release still ships one formula and
  no DMG, and the app lives in `libexec` beside `SwarmNotify.app` and `SwarmHUD.app`.

### Consequences

- Good: one command opens or focuses one full-screen Swarm window, and the camera row matches the
  page.
- Bad: a native target is back, with its own build step, and the page runs in WebKit, not the
  owner's browser.
