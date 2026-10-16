---
status: accepted
date: 2026-10-16
deciders: [user]
related: ["0043", "0087", "0090", "0091"]
informed-by:
  - 'owner, 2026-10-16: "swarm setup" alone lists what can be set up; every setup option goes through it'
  - 'owner, 2026-10-16: no herdr item, no skills item, no undo; the plugin covers pi; move swarm-voice and swarm-orchestrator into the plugin'
---

# 0092. swarm setup is the one entry point for every setup item

## Context and Problem Statement

Setting up swarm took five commands: `swarm setup --only <group>`, `swarm hooks setup`,
`swarm setup plugin`, `swarm hud install`, and `swarm doctor` to see the state. Bare
`swarm setup` wrote every plan group at once. The plugin and the HUD had no plan or digest, so
the web server had special routes for them. The `herdr` group never planned anything, and the
`skills` group linked a copy of the kit skills that the plugin already installs.

## Considered Options

- One command, `swarm setup <items>`, where bare `swarm setup` only lists.
- Keep each command and add a listing command beside them.

## Decision Outcome

Chosen: one command, because the owner asked for one entry point and a list that writes nothing is
safer than a bare command that writes everything.

- `swarm setup` and `swarm setup --json` print the doctor rows and write nothing.
- The items are `hooks`, `trust`, `plugin`, and `hud`. `swarm setup <item>,...` plans those, and
  `swarm setup all` plans each item whose row reads `setup`.
- Every item is part of one plan with one digest. The plugin part is the `~/.swarm/kit` link and
  each CLI's install command, and the HUD part is the LaunchAgent text.
- A terminal run shows the plan and asks `Apply this plan? [y/N]`. An answer applies only the plan
  it answered. `--yes` applies what is pending once the trust lock is free, and `--digest` applies
  only the plan with that digest. A child pane may plan but not apply.
- The `herdr` and `skills` groups are gone. `swarm-voice` and `swarm-orchestrator` move into the
  kit, so the plugin installs them in Claude, Codex, AGY and pi. The plugin plan removes the skill
  links that an older swarm recorded. `swarm skills refresh` and `swarm skills manifest` are gone.
- `swarm hooks setup`, `swarm hud install`, and `--only` stop with an error that names the new
  command. `swarm hud uninstall` and the read-only status commands stay.
- `POST /api/setup/apply` takes any item list and its digest. The page builds Set up all from the
  doctor rows, so no file copies the item list.
- There is no `swarm setup undo`. `swarm managed revert` still removes a file entry.

### Consequences

- Good: one command and one consent rule for every item, in the terminal and on the page.
- Good: the plugin and the HUD get the same preview and stale-plan check as the file items.
- Bad: a script that ran bare `swarm setup` to apply now only lists. It must name the items.
- Bad: the plugin plan runs `codex plugin marketplace list` for each Codex home, so it is slower.
- Bad: the old `~/.swarm/skills` copy stays on disk until the owner removes it.

## Links

- Supersedes the bare apply in 0043, the plugin flags in 0090, the HUD install command in 0087,
  and the split group lists in 0091.
