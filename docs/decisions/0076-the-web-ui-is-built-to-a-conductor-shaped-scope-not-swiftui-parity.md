---
status: accepted
date: 2026-10-13
deciders: [user]
related: ["0073", "0075"]
informed-by:
  - 'owner, 2026-10-13: "Why are we porting it one-to-one? ... One-to-one conversion is not really a good choice here because the UI and the iteration was very shitty and it was very bulky."'
  - 'owner, 2026-10-13: "okay lean app is cool", after the jean spike'
  - "tmp/web-port/spike-jean-result.md (jean bda4576e on swarm serve: scroll 2.53% / 18.97% dropped, WebKit key 67 ms, load 383-446 ms)"
---

# 0076. The web UI is built to a Conductor-shaped scope, not SwiftUI parity

## Context and Problem Statement

ADR 0075 moved the UI to a web app and said `ui/` is deleted at parity. The port then copied the
SwiftUI app screen by screen, with a parity checklist from its 31k lines. The owner found the
SwiftUI app bulky and asked to reuse an existing Conductor-style web UI. A spike ran jean, the
closest open-source match, on `swarm serve`.

## Considered Options

- Keep the SwiftUI parity checklist.
- Fork jean's web UI.
- Keep the lean web app and build a short Conductor-shaped feature list, with jean's look.

## Decision Outcome

Chosen: the lean app with a Conductor-shaped scope, because jean fails the speed budgets by
design (its chat list is not windowed), 69% of its surface is jean-only, and it changes about 80
times a week; the lean app passes every budget and shares jean's stack, so its look is cheap to
copy.

- Scope v1: sidebar (projects, workspaces, chats), chat, agent panes, tabs, keyboard map and ⌘K
  palette, new workspace and chat, switch model, Changes diff, PR and CI status, usage meters,
  minimal settings, shell natives, workspace from a GitHub issue, a Recent list across projects,
  diff line comments sent to the agent, and a setup script on new workspaces.
- Dropped: tab groups with colours, and the notch usage HUD panel (0073). Compact usage meters stay
  in the app.
- Later: Runs graph, Skills editor, Guards, Managed changes, Setup sheet, Files tree, chat windows,
  raw data mode.
- `ui/` (except the shell) is deleted when Scope v1 ships, not at SwiftUI parity.
- jean's visual language and four helpers are copied with their Apache-2.0 notice.

## Consequences

- Good: less UI to build, and each feature is chosen for use, not for parity.
- Bad: SwiftUI features outside Scope v1 and "Later" disappear when `ui/` is deleted.

## Amendment 2026-10-13: the Skills editor leaves the web UI

Deciders: [user]. Informed by the owner answer "drop the skills eidtor right now... we can pick it up
later", after the kit started to ship as the plugin `swarm` (`docs/blocks.md`, Packaging).

The kit's skills and blocks are plain files in `skills/kit/`, and each CLI loads them from the
plugin. So the web UI has no Skills view, editor or skills-folder setting, `swarm serve` has no
skill routes, and the web app does not run `swarm skills refresh` at start. The Swift Skills code
goes with `ui/` at cutover. A later editor, if any, edits the kit in `skills/kit/`. Guards, the
Setup sheet and Managed Changes stay "Later".
