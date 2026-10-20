# Environment

When `SWARM_HOME` is not set, only a release build uses `HOME`. release.yml marks it with
`SWARM_RELEASE_BUILD=1`. Every other build is a dev build and uses `~/.swarm-<branch>`, `main` too
(`~/.swarm-main`), so a dev build never touches the real data (ADR 0027, amended in ADR 0048). A
dev build from a detached HEAD uses `~/.swarm-head+4b9253d8ff1ee183`. A branch name with a character outside `[a-z0-9._-]`, one that starts with `.` or
`-`, or one over 200 bytes gets its letters made lowercase and its other unsafe bytes made `-`, is cut
to 200 bytes, and gets a hash of the whole name added, so `feat/login` uses
`~/.swarm-feat-login+407712bf7898fb7f` and never meets `feat-login`. An uppercase letter also takes
the hash, because the default macOS disk ignores case, so `Feature` and `feature` get two folders. `branch_folder` in `src/paths.rs`
states the exact rule.
`swarm --version` prints the branch after the commit, nothing for a release build and `HEAD` for
a detached one. An explicit `SWARM_HOME` always wins, and an empty one is an error. The data
directory is always `$SWARM_HOME/.swarm`.

Swarm writes there only when the folder is its own (ADR 0036). A missing or empty folder gets the
marker file `.swarm/swarm-home` before any other file. A folder from an older swarm, with swarm's
`swarm.db` and no marker, gets the marker. Any other folder is refused with no write, and the
message names the folder and `SWARM_HOME`.

| Variable | Meaning |
|---|---|
| `SWARM_HOME` | Parent of the `.swarm/` data directory. Defaults to `$HOME` for a release build, or `~/.swarm-<branch>` for a dev build. |
| `SWARM_ADAPTER` | Adapter file name under `.swarm/adapters/`. Defaults to `herdr` in a Herdr pane, else `superset` in a Superset terminal, else `rex` in a Rex block (`REX_BLOCK` set and `TERM_PROGRAM=rex`), else `tmux`. |
| `AGENT_ROUTING_CONFIG` | The old routing file that the first read imports. A path that does not exist is an error. |
| `SWARM_SESSION_ID` | Session the caller belongs to. `spawn` stamps it into each child pane. |
| `SWARM_AGENT_ID` | Identity of the caller. `spawn` stamps it into each child pane. |
| `SWARM_SUMMARIZER` | Shell line `drain` runs with a log on stdin. Required by `drain` only. |
