# swarm

Message bus and pane control for a tree of agent CLIs. One orchestrator spawns children into
terminal panes (tmux or Herdr), the agents talk through a SQLite inbox, and a drainer summarizes
the transcript of any child that exits without a summary.

## Install

```sh
brew install priyanshuupadhyay/tap/swarm   # macOS; installs tmux too
# or from a checkout: cargo install --path .
swarm init          # creates $SWARM_HOME/.swarm with the db, runs/, and adapters/ directories
```

### Swarm app

Run `brew install --cask priyanshuupadhyay/tap/swarm-app`, which also installs the CLI and tmux. Or
download `Swarm-<version>.dmg` from the
[latest release](https://github.com/PriyanshuUpadhyay/swarm/releases/latest), open it, and drag
Swarm onto Applications. The app carries its own `swarm` CLI. It still needs tmux
(`brew install tmux`) and at least one agent CLI (`claude`, `codex`, or `agy`).

The app is not notarized by Apple, so macOS blocks the first launch. Allow it once:

1. Open Swarm from Applications. macOS says it cannot verify the app. Click **Done**.
2. Open **System Settings > Privacy & Security** and scroll to **Security**.
3. Next to "Swarm was blocked", click **Open Anyway**, then confirm with your password.

On macOS 15 and later, a Control-click on the app and **Open** no longer skips this check.
If **Open Anyway** does not show, run `xattr -dr com.apple.quarantine /Applications/Swarm.app`.

## Environment

When `SWARM_HOME` is not set, a build from `main` (or from a detached HEAD) uses `HOME`, and a
build from any other branch uses `~/.swarm-<branch>`, so a branch build never touches the real data
(ADR 0027). A branch name with a character outside `[a-z0-9._-]`, one that starts with `.` or
`-`, or one over 200 bytes gets its letters made lowercase and its other unsafe bytes made `-`, is cut
to 200 bytes, and gets a hash of the whole name added, so `feat/login` uses
`~/.swarm-feat-login+407712bf7898fb7f` and never meets `feat-login`. An uppercase letter also takes
the hash, because the default macOS disk ignores case, so `Feature` and `feature` get two folders. `branch_folder` in `src/paths.rs`
states the exact rule.
`swarm --version` prints the branch after the commit. An explicit `SWARM_HOME` always wins. The data
directory is always `$SWARM_HOME/.swarm`.

| Variable | Meaning |
|---|---|
| `SWARM_HOME` | Parent of the `.swarm/` data directory. Defaults to `$HOME`, or `~/.swarm-<branch>` for a branch build. |
| `SWARM_ADAPTER` | Adapter file name under `.swarm/adapters/`. Defaults to `tmux`. |
| `AGENT_ROUTING_CONFIG` | The old routing file that the first read imports. A path that does not exist is an error. |
| `SWARM_SESSION_ID` | Session the caller belongs to. `spawn` stamps it into each child pane. |
| `SWARM_AGENT_ID` | Identity of the caller. `spawn` stamps it into each child pane. |
| `SWARM_SUMMARIZER` | Shell line `drain` runs with a log on stdin. Required by `drain` only. |

Agent profiles live in `$SWARM_HOME/.swarm/profiles.json` (ADR 0030). A profile is one role, such
as `chat` or `code.complex`, with an ordered list of runners. A runner is a provider, a model, an
effort, and the flags its provider takes. `chat` is always first. With no file, the first read
imports the old routing file (`$AGENT_ROUTING_CONFIG`, else
`$XDG_CONFIG_HOME/agent-routing/roles.json`, else `~/.config/agent-routing/roles.json`) and writes
`profiles.json`. Each route becomes a profile whose runners are the route's runners, then their
substitutes. The old file is never written, and a later edit to it only prints a warning. With
neither file, the `default-profiles.json` built into the binary is used and nothing is written until
the first save. A file that is a broken symlink is an error, never a silent switch to the default.
A save writes through a symlink, so `profiles.json` can live in a dotfiles checkout.

A launch takes the first runner that can run (ADR 0031). It skips a runner whose CLI is not on
PATH, whose accounts are all signed out, or whose best account has less usage left than
`min_usage_left_pct` (default 5). The account read has a 2 s deadline; a read that fails, times
out, or finds no accounts counts as "can run". Each skip is one stderr line, such as
`swarm: code.complex: skipped claude/opus/high: usage 2% left (threshold 5%)`, and a launch where
no runner can run fails with one line per runner.

## Commands

Caller `any` needs no identity. `session` needs `SWARM_SESSION_ID`. `agent` needs both ids. The
`orchestrator` caller also must match the session's orchestrator agent.

| Command | Caller | Effect and output |
|---|---|---|
| `--version` | any | Print the package version and build commit. |
| `init` | any | Create `.swarm/`, `runs/`, `adapters/`, and the database. Shipped adapters stay in the binary; matching old disk copies are removed. |
| `adapter check <name>` | any | Load the shipped adapter plus any disk overrides. Print the verbs that the disk file overrides. |
| `session new <lane\|relay\|open> [--chair <claude\|codex>:<id>]` | any | Create a session for the physical current directory (`pwd -P`) and print its UUID v7 id. Without `--chair`, use a chair id from the current CLI environment when present. |
| `session chair <claude\|codex>:<id>` | orchestrator | Set the chair transcript id. Refuse any other agent. |
| `session continue <new_id> <old_id>` | any | Link a newer session to an older session in the same directory after the new chair receives its context. |
| `session archive <id>...` | any | Archive one or more UUID v7 sessions. |
| `sessions --json` | any | List active sessions and resolved chair logs as JSON. |
| `host-context --provider <claude\|codex\|agy>` | any | Print the session's host contract in that provider's hook format, or nothing outside a visible host. |
| `hook <claude\|codex\|agy> [event]` | any | Read a provider hook's JSON on stdin and record the agent's state (`working`, `waiting`, `done`, `failed`) for `SWARM_AGENT_ID`. Does nothing outside a swarm agent. Always prints `{}` and exits 0. AGY sends no event name, so its hook passes it, as in `swarm hook agy Stop`. |
| `herdr-split` | any | Split a child pane right of `HERDR_PANE_ID`, stack it under earlier children at equal height, and print its id. The herdr adapter's spawn verb. |
| `roles --json` | any | Print every profile, the file's `revision`, and `imported` when the file came from the old routing file. Reads no usage. |
| `roles check --json` | any | Print, for each profile, the runner a launch would take now (`pick`) and each skipped runner with its `code` and `text`. |
| `roles get <role> [--provider <claude\|codex\|agy>]` | any | Print the runner a launch would take as JSON: its fields plus `role`, `runnerId` (`<role>#<n>`), `fallbackRunnerIds`, `skipped`, and `substitutedFor` when a later runner was taken. With `--provider`, that provider's runners are tried first, and a profile with no runner of that provider is refused. |
| `roles save --revision <revision> <profile-json>` | any | Replace one profile, as `{"name", "runners"}`, and print the new revision. Fails when the file changed after `revision` was read. |
| `providers --json` | any | List each provider with its efforts, default effort, whether it has accounts, and the flags a runner of it takes. |
| `models --provider <claude\|codex\|agy> --json` | any | List models for a provider. Codex and AGY use their CLI catalogs, and a Codex model lists its efforts; Claude shows its model aliases. |
| `accounts --provider <claude\|codex\|agy> --json` | any | List accounts for one provider as JSON. |
| `usage --json` | any | List account use meters as JSON. |
| `drain` | any | Run queued summarize jobs, print `done`, `retry`, or `parked` per job. |
| `agent add <id> <role>` | session | Register an agent. The `orchestrator` role also records the caller pane and session adapter. |
| `agents --json` | session | List agents, pane state, agent state, and adapter attach support as JSON. For each live agent it reads the pane's bottom rows (the adapter's `screen` verb) and records `working`, `waiting`, or `done` when the screen shows it and no hook reported in the last 10 s. |
| `messages --json [--after <seq>]` | session | List message metadata and available bodies as JSON. |
| `launch <id> <role> [--provider <claude\|codex\|agy>] [--model <name> for chat] [--account <auto\|name>] [--cwd <dir>] [-- <args>...]` | session | Start the first runner of the role's profile that can run, or, with `chat --provider <provider> --model <name>`, exactly that model once with the chat profile's effort for that provider and no fallback (ADR 0032). `--account` is ignored for a provider with no accounts. Register the agent, split a pane in `--cwd`, and start its provider CLI. A child caller is refused. A Claude child runs from `<cwd>/.herdr/workers`, and the pane dir is pre-trusted for Claude, Codex, and AGY. |
| `spawn <id> <role> [--provider <p>] [--account <auto\|name>] [-- <cmd>...]` | session | Register the agent, split a pane, and optionally run `<cmd>; swarm exited`. Print the pane id. |
| `type <id>` | session | Read text from stdin and type it into a live agent pane. A closed pane causes an error before any input is sent. |
| `interrupt <id>` | session | Send the adapter interrupt action to the agent pane. |
| `attach <id>` | session | Attach to the agent pane when the adapter supports it. |
| `close <id>` | session | Close the pane of `<id>` and forget it. |
| `send <recipient> <kind>` | agent | Store stdin as a message, ring the recipient, print the seq. |
| `finish` | agent | Send stdin as a `summary` to the orchestrator, print the seq. |
| `exited` | agent | Capture the own pane to `runs/<session>/<id>.log`, report a missing summary. |
| `sweep [--every <secs>]` | agent | Report each child whose pane is gone, print `dead <id>`. Re-ring an unseen message at most once after 60 s, for a child or the caller itself, then wait for an ack. With `--every`, repeat every N seconds and warn instead of exit on a failed pass. |
| `inbox` | agent | Print `seq sender kind body_path` per unread message. |
| `ack <seq>` | agent | Mark one message read. |

The Herdr adapter's spawn verb runs `swarm herdr-split` through `$SWARM_EXE`, the path of the binary
that runs the verb, so Swarm.app finds it even with a short `PATH`. Claude, Codex, and AGY run
`swarm host-context --provider <claude|codex|agy>` as a SessionStart hook to learn the host contract.

Swarm.app opens on Home, where routed roles show their models. Open Project adds a folder to the
project list, even when it has no chats. Create Project makes a plain folder and adds it there. New Chat
lets the user choose a provider and model without changing a routed role. Codex and AGY list CLI models; Claude lists
aliases and accepts a full model name in Other model. In a chat, Switch model asks the live chair
for a compact summary, starts the chosen Claude or Codex model, and keeps both parts in one chat
tab. If the old pane has closed, the new chair receives recent messages and makes its own compact
summary.

For a Git project, Create workspace starts a branch from the default branch in a worktree beside the
project, then opens New Chat there. Empty task worktrees stay available from the project view.
Plain folders keep the New Chat action without Git worktrees.
The sidebar lists one row per workspace across projects, in Pinned and My workspaces. Search
finds workspace names, projects, branches, and chat titles. Chats in the selected workspace appear
as underlined tabs above the transcript. The plus button starts another chat in that workspace.
Pins, workspace names, and the last selected chat survive restarts. Archive workspace hides its
row without deleting files, archiving chats, or stopping agents; Archived offers Restore workspace.
Herdr-hosted agents with live panes attach through Herdr's direct terminal stream, so the pane
accepts input in Swarm. A closed connection can be reopened with Reconnect.

## Performance profiling

Turn on **Debug > Performance Logging** in Swarm.app. Then open a slow chat or start a new one.
In Terminal, watch the stage times in milliseconds:

```sh
/usr/bin/log stream --style compact --level info --predicate 'subsystem == "io.github.priyanshuupadhyay.swarm" AND category == "performance"'
```

For a saved trace, use `/usr/bin/log show --last 10m --style compact --info` with the same predicate.
The logs show stage names, times, and item counts. They do not include chat text, paths, or
command arguments. Turn off the menu switch when done. Set `SWARM_PERF=1` to enable the same
timings for a command-line run. In Instruments, use Time Profiler for CPU work and Points of
Interest for the stage intervals. `AppStarted` and `WindowReady` mark startup.
`ChatSelected`, `ChatDetailAppeared`, and `ChatRowsShown` mark the visible chat-open path;
`InitialRefresh`, `WorkspaceTree`, and `TranscriptPoll` show where the time goes before that.

## Agents

Two skills tell an agent CLI how to take part. `skills/swarm-voice` is for a child that
`swarm spawn` started, and `skills/swarm-orchestrator` is for the parent. Inside the repo, Claude Code
finds them through `.claude/skills` and AGY through `.agents/skills`, both links to `skills/`; Codex reads
`AGENTS.md`. `sh scripts/install.sh` links them into every agent CLI on the machine. `demo/herdr.sh` and
`demo/tmux.sh` each run one live voice on that host: `cargo install --path .` then
`VOICE=claude|codex|agy sh demo/herdr.sh` from a Herdr pane, or `sh demo/tmux.sh` from inside tmux.

## Walkthrough

Run the orchestrator inside tmux, so `spawn` has a pane to split.

```sh
export SWARM_SESSION_ID=$(swarm session new lane)   # lane: children talk only to the orchestrator
export SWARM_AGENT_ID=orchestrator
swarm agent add orchestrator orchestrator
swarm spawn coder coder -- my-agent --task "write the parser"   # prints the pane id, e.g. %3

# in the coder pane, SWARM_SESSION_ID and SWARM_AGENT_ID=coder are already set
echo "parser done, tests green" | swarm finish        # prints the seq, e.g. 0

# back in the orchestrator pane
swarm inbox                                            # 0 coder summary runs/<session-id>/0.txt
swarm ack 0
swarm close coder

# a child that exits without finish gets a fallback summary and a summarize job
SWARM_SUMMARIZER='head -c 200' swarm drain            # done 1
```
