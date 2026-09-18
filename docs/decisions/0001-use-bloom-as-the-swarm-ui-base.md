---
status: accepted
date: 2026-09-16
deciders: [user]
related: []
informed-by:
  - https://github.com/spatie/bloom
  - ~/.claude/reports/2026-09-16-conductor-ui-web.md
links: []
---

# 0001. Use Bloom as the base for the swarm UI

## Context and Problem Statement

Swarm has a message store and pane control, but no graphical UI. We want a UI that looks like
conductor.build (projects in a sidebar, one workspace per git worktree, a changes panel, pull
request actions) and that a non-technical user can work with. Conductor is closed source. We want
a native macOS app, and we can add the pane and agent integration ourselves.

## Considered Options

- spatie/bloom, a native Swift app (MIT) with a Conductor-like layout and split panes
- mrmans0n/alas, a native Swift app (MIT) with a similar layout, 18 stars
- vaayne/mori, a native Swift app (MIT) built on tmux and libghostty
- coollabsio/jean or charannyk06/conductor-oss, Apache-2.0 apps with a Rust backend (Tauri and web)
- dcouple/Pane, an Electron app (AGPL-3.0)
- Build a new UI from scratch

## Decision Outcome

Chosen: spatie/bloom, because its screens already match Conductor. It has a projects sidebar with
workspaces, a center area where tabs split into chat, terminal, and browser panes, and a changes
panel with a pull request button. It is native Swift with two dependencies (SwiftTerm, Sparkle), and
its MIT license lets us change it freely. We keep Bloom's views and replace how it runs agents with
swarm.

### Consequences

- Good: we start from a finished Conductor-style UI and do not design screens from zero.
- Good: SwiftTerm panes can attach to the live tmux panes that swarm already spawns.
- Bad: Bloom's chat drives Claude through `stream-json` and Codex through `app-server` (JSON modes
  with no terminal). Swarm agents are interactive CLIs in panes, so the chat must be rebuilt on swarm
  messages.
- Bad: Bloom has no Gemini or AGY support. We add it through swarm.
- Bad: Bloom is young (created 2026-08-18, 8 contributors), so upstream can change a lot, and we
  must keep Spatie's MIT copyright notice.
- Bad: the UI is macOS only.

## Decision Drivers

- The UI must look like Conductor and suit non-technical users.
- The app must be native Swift.
- The license must allow a fork that we change deeply.
- Split panes must already exist; the pane backend stays ours.

## Pros and Cons of the Options

### spatie/bloom
- Good, because the layout matches Conductor and panes already split.
- Bad, because its agent layer uses JSON modes, not terminals.

### mrmans0n/alas
- Good, because it has the same layout and a Ghostty terminal or chat pane.
- Bad, because it has 18 stars and one author.

### vaayne/mori
- Good, because it already uses tmux sessions per worktree.
- Bad, because it is a terminal app that looks like Herdr, not like Conductor.

### Jean, conductor-oss, Pane
- Good, because they have projects, worktrees, and diffs.
- Bad, because they are not native Swift (Tauri, web, Electron), and Pane is AGPL-3.0.

### Build from scratch
- Good, because nothing upstream constrains the design.
- Bad, because every screen that Bloom already has must be designed and built.

## Confirmation

The decision holds while the fork can show a swarm session's agents under a project and worktree,
with a live pane and a chat for each agent, without a rewrite of Bloom's sidebar and changes panel.
Revisit if upstream Bloom stops being maintained or changes its license.

## Informed by

- The web search report `~/.claude/reports/2026-09-16-conductor-ui-web.md`
- Bloom's README, its screenshot `art/overview.png`, and `docs/PROTOCOL.md` and `docs/CODEX.md`
- The screenshot of mrmans0n/alas `art/alas-acp.png`

## Links

- Sources: https://github.com/spatie/bloom, https://github.com/mrmans0n/alas,
  https://github.com/vaayne/mori
