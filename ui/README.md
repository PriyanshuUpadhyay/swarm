# Swarm UI

Swarm is a macOS chat app for the swarm bus. ADR 0014 sets its scope to projects, worktrees, sessions, a chair transcript, and a live pane.

The `Swarm` target owns views and focus.
`SwarmCore` owns bus calls, rules, and process work.
`TranscriptTool` reads the Zig transcript stream.

Run `make build` to compile the Swift targets.
Run `make test` to build the transcript tool and run tests.
Run `make lint` to check the source boundaries.
Run `make app` to build `.build/release/Swarm.app`.
Run `make install` to put it in `~/Applications` and keep the old app.
Run `make run` to build and launch the app.
For a development build, set `SWARM_HOME=~/.swarm-<branch>` to keep its data apart.
