---
status: accepted
date: 2026-10-13
deciders: [user]
related: ["0009", "0014", "0023", "0025", "0028", "0048", "0069", "0072", "0073"]
informed-by:
  - 'owner, 2026-10-12: "it''s a pain to work with right now ... agents are much better with webapps and iteration in swarm is very shitty"'
  - 'owner, 2026-10-12: "after the pilot, go ahead and start the spike" (Swift features freeze only after the pilot passes)'
  - 'owner, 2026-10-13: "okayy let''s goooo start the port with your recommendation"'
  - "~/dotfiles/docs/council/2026-10-12-swarm-web-ui.md (GO-WITH-CHANGES, unanimous)"
  - "tmp/web-pilot/2026-10-12-web-pilot.md (pilot budgets and results, iterations 1-17)"
  - 'owner, 2026-10-13: "you can use a library of ui as well to make it look sweet and follow the conductor.build aesthetics"'
  - "https://github.com/coollabsio/jean package.json (shadcn/ui stack), read 2026-10-13"
links:
  - "web-pilot commits e7242265 (serve feature), c5dcd6cc (swarm serve), 58b342e1 (web UI), 197837a5 (budget suite), c23a15fb (SwarmWeb shell)"
---

# 0075. The UI is a web app on `swarm serve` in a thin Swift shell

## Context and Problem Statement

The SwiftUI app in `ui/` is a client of the `swarm` CLI. It has 31k lines of source and 19k lines
of tests. Agents iterate on it slowly. A check needs a clean-env launch, LaunchServices can start
any of about 22 dev builds, checks drive the app through AX, and branch builds share the user
defaults of the installed app. Agents iterate much faster on a web app, with hot reload and
Playwright DOM checks. The owner wants the UI to stay very fast with long transcripts and 12 or
more live agents.

## Considered Options

- Keep SwiftUI.
- A web app in a thin Swift WKWebView shell.
- A web app in Tauri.
- A web app in a plain browser tab.

## Decision Outcome

Chosen: a web app in a thin Swift WKWebView shell, because the pilot met every speed budget in
this shell and in Playwright, and agents can check the web app without the native app.

- `swarm serve` (cargo feature `serve`) owns the HTTP API. It binds 127.0.0.1 and checks a
  per-launch token, the Host and the Origin on every `/api` request. JSON reads, one SSE stream per
  window, and JSON POST for writes. One coordinator owns all side effects, and reads are pure.
- The Zig transcript tool parses logs on the server. No WASM, and no disk cache in v1.
- `web/` is React 19 with the compiler, and virtua for long lists. Domain rules live in Rust view
  models, not in the web app.
- The look follows conductor.build. The components come from shadcn/ui (Radix primitives and
  Tailwind v4, copied into `web/`), with cmdk for the palette and react-resizable-panels for
  splits, the same stack as jean (Apache-2.0). Rows in the sidebar and the transcript stay plain
  elements, and one menu or tooltip serves a whole list, so the speed budgets hold.
- `SwarmWeb` is the Swift shell. It starts `swarm serve`, shows the page, and maps menu keys to
  `window.swarmCommand(id)`. Only what a page cannot do on macOS stays in Swift: notifications and
  their click to a pane, the Dock badge, the login item, folder pickers and Finder drops that need
  a real path, extra windows, and the usage HUD panel. Keychain reads are already in Rust (0072).
- The current key map is ported as is (0023). The four-chat cache (0025) and the upside-down
  transcript (0028) are kept as rules for the web app.
- Swift features freeze now. Swift changes are bug fixes only, until the web app reaches parity
  and `ui/` (except the shell) is deleted.

## Decision Drivers

- Speed budgets (council round 3): key-to-paint p95 <= 50 ms, warm switch p95 <= 100 ms, cold open
  p95 < 150 ms, 1% or fewer dropped frames for scroll and 12 x 50 events/s live load, a 200k-line
  tool output, and no memory growth over a 1 h soak.
- Fast agent iteration with DOM checks.
- The CLI keeps all state, and no second owner of side effects appears.

## Pros and Cons of the Options

### Keep SwiftUI
- Good, because nothing is rewritten.
- Bad, because the slow agent loop stays.

### Thin Swift WKWebView shell
- Good, because the native parts stay in a language and stack the repo already ships.
- Good, because the pilot measured it: key p95 11 ms, warm switch 32-33 ms, cold open 65-82 ms,
  0% scroll drops, 0.19-0.29% live drops, and no memory growth over a 60 min soak.
- Bad, because a small Swift target stays, so the repo keeps two UI languages.

### Tauri
- Good, because the shell is Rust and no Swift stays.
- Bad, because it adds a second build stack and plugins for the native parts, on the same
  WKWebView, so it gives no speed gain.

### Plain browser tab
- Good, because no shell exists at all.
- Bad, because the browser keeps ⌘1-9, and a tab has no login item, Dock badge, folder path, or notice click to a
  pane (0069).

## Confirmation

`web/perf/run.mjs` and `SwarmWeb --measure` print the budget table. The port keeps every row
passing. Revisit if a budget fails in the shell and no web-side fix brings it back.

## Consequences

- Good: agents build and check the UI with Vite, Vitest and Playwright, without the native app.
- Good: the UI talks to one API that a test can call over HTTP.
- Bad: two UIs exist until parity, and a Swift bug fix may need a second fix in `web/`.
- Bad: `swarm serve` is a local server that can start agents, so its token, Host and Origin checks
  are a trust boundary that every new route must keep.
- When `ui/` is deleted, a new record supersedes 0014, and the cask from 0048 ships the shell and
  the web assets in place of the SwiftUI app.
- Open from the pilot: three soak runs under heavy machine load had a 44 s paint pause with no
  known cause.
