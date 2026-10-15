---
status: accepted
date: 2026-10-15
deciders: [user]
amends: ["0080"]
related: ["0043", "0079"]
informed-by:
  - 'owner, 2026-10-15: "It should be part of the swarm CLI command ... if I write swarm setup plugin, then it should auto-install them directly by asking the user for permission ... Claude, Codex, Agy, Pi"'
  - 'owner, 2026-10-15: "It has to be embedded in the binary, right? Or the tab that we are going to release." and "The fixed path is a better option ... I don''t want to point and installed it from a local directory."'
  - "Scratch-home trials on 2026-10-15 with Claude Code 2.1.300, Codex 0.162.1, AGY 1.3.5 and pi 1.2.0: each installs the kit folder once it holds its own marketplace.json; a second folder under the name swarm is taken by Claude and AGY, refused by Codex, and added beside the first by pi"
---

# 0090. swarm setup plugin installs the shipped kit in each CLI

## Context and Problem Statement

ADR 0080 ships the kit only as the plugin `swarm` and leaves the install to each CLI, so the owner
and every other user had to find and type four sets of plugin commands. The kit in the release
had no marketplace file, so Claude Code and Codex could not install it at all. Homebrew deletes
`Cellar/swarm/<version>` on upgrade, so a CLI that reads the kit in place loses it.

## Considered Options

- A `plugin` group in `swarm setup`, with a digest and a managed record for each change.
- A separate `swarm setup plugin` that runs each CLI's own install command after a yes.
- Install steps in the README only.

## Decision Outcome

Chosen: `swarm setup plugin [--yes]`, because a CLI's plugin command is not a file edit that
swarm can plan as a diff or undo, and a README step was never run.

- The command installs only the kit that ships in `libexec/skills/kit`. It takes no source folder.
- It points `~/.swarm/kit` at the kit through `<prefix>/opt/swarm` when that is the same folder,
  and each CLI installs from the link, so Claude Code and pi keep the kit over `brew upgrade`.
- It runs the commands of Claude Code, Codex (once for each Codex home), AGY and pi that are on
  `PATH`, after a `y` on the terminal or with `--yes`. With no terminal and no `--yes` it changes
  nothing. A child pane is refused, as for `swarm setup`.
- A Codex home whose `swarm` marketplace holds another folder removes it first.
- The kit holds `.claude-plugin/marketplace.json` with the source `./`, so the kit folder is its
  own marketplace.
- Skills of the same names in the owner's skill folders stay, and the command names them.

### Consequences

- Good: a new user runs `brew install swarm` and then `swarm setup plugin`, with no clone and no path.
- Good: a second run is safe and updates the copies that Codex and AGY keep.
- Bad: swarm records no undo; the owner removes the plugin with each CLI's own command.
- Bad: after each `brew upgrade` the owner must run it again, or Codex and AGY keep the old kit.
- Bad: kit edits in a checkout reach no CLI until a release ships them.

## Links

- Amends: 0080. Related: 0043, 0079.
