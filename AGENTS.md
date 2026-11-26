# Agents

This repo ships two skills for agent CLIs that take part in a swarm.

- If your pane was started by `swarm launch`, read
  `skills/kit/skills/swarm-voice/SKILL.md` and follow it.
- If you are asked to run a swarm session, read `skills/kit/skills/swarm-orchestrator/SKILL.md`.

Inside this repo, Claude Code finds them through `.claude/skills` and AGY through `.agents/skills`; Codex reads this file, so open the skill you need before you act. Both ship in the plugin `swarm`, so `swarm setup plugin` installs them in every agent CLI on this machine.

When you write or review a test, follow the test rule in `skills/kit/blocks/prove-it-works/SKILL.md` (ADR 0134): one main-path test per behavior, a failure test only for a real bug or a guard, and no edge-case lists.

To build and test a worktree, use `scripts/dev.sh`. Do not write the build and test commands yourself. It runs the CI steps in order, writes each log to `tmp/dev/<step>.log`, and removes the pane's `SWARM_*`, `HERDR_*` and `TMUX` variables, so no build or test reaches the owner's live session.

- In a new worktree, or when a build fails, run `scripts/dev.sh doctor` first. It prints a `fix` command for each missing part.
- `scripts/dev.sh build` installs and builds the web UI, then runs `cargo build --features serve --bins`.
- `scripts/dev.sh check <step>...` runs only the named steps, for a quick test loop, such as `scripts/dev.sh check test-serve`. An unknown step name prints the list of steps.
- Before you land, run `scripts/dev.sh check`, which runs all the CI checks. Add `--e2e` when the change touches the web UI or the server.

A new chat on a hub's main worktree runs in its own new worktree, and its first prompt names the branch (ADR 0146). Edit and commit there, then run `swarm task land` from that folder; it opens a PR, and `--merge` merges directly (ADR 0150). Land never removes the worktree: after it, offer the cleanup to the owner, and run `swarm task drop` only after approval (ADR 0151). The guard blocks edits to main.
