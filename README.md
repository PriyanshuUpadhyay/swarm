# swarm

Message bus and pane control for a tree of agent CLIs. One orchestrator spawns children into
terminal panes (tmux or Herdr), the agents talk through a SQLite inbox, and a drainer summarizes
the transcript of any child that exits without a summary.

## Install

```sh
cargo install --path .
swarm init          # creates $SWARM_HOME/.swarm with the db, runs/, and adapters/ directories
```

## Environment

Set `SWARM_HOME` by hand for each process that needs a separate data set. For example, a development
process can use `SWARM_HOME=~/.swarm-<branch>`. Nothing sets it from the branch or build. When it is
not set, the binary uses `HOME`. The data directory is always `$SWARM_HOME/.swarm`.

| Variable | Meaning |
|---|---|
| `SWARM_HOME` | Parent of the `.swarm/` data directory. Set it for each process. Defaults to `$HOME`. |
| `SWARM_ADAPTER` | Adapter file name under `.swarm/adapters/`. Defaults to `tmux`. |
| `SWARM_SESSION_ID` | Session the caller belongs to. `spawn` stamps it into each child pane. |
| `SWARM_AGENT_ID` | Identity of the caller. `spawn` stamps it into each child pane. |
| `SWARM_SUMMARIZER` | Shell line `drain` runs with a log on stdin. Required by `drain` only. |

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
| `roles --json` | any | List routed roles and every provider choice for each role as JSON. |
| `roles set-model <runner> <model>` | any | Save a runner's model in the shared routing config. Every role using that runner changes. |
| `models --provider <claude\|codex\|agy> --json` | any | List models for a new chat. Codex and AGY use their CLI catalogs; Claude shows its model aliases. |
| `accounts --provider <claude\|codex\|agy> --json` | any | List accounts for one provider as JSON. |
| `usage --json` | any | List account use meters as JSON. |
| `drain` | any | Run queued summarize jobs, print `done`, `retry`, or `parked` per job. |
| `agent add <id> <role>` | session | Register an agent. The `orchestrator` role also records the caller pane and session adapter. |
| `agents --json` | session | List agents, pane state, and adapter attach support as JSON. |
| `messages --json [--after <seq>]` | session | List message metadata and available bodies as JSON. |
| `launch <id> <role> [--provider <claude\|codex\|agy>] [--model <name> for chat] [--account <auto\|name>] [--cwd <dir>] [-- <args>...]` | session | Resolve a routed role, or use `chat --provider <provider> --model <name>` for a direct model choice. Register the agent, split a pane in `--cwd`, and start its provider CLI. A child caller is refused. A Claude child runs from `<cwd>/.herdr/workers`, and the pane dir is pre-trusted for Claude, Codex, and AGY. |
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

The Herdr adapter runs `~/.config/herdr/bin/swarm-split.py` to create its pane layout. The repository
ships it as `adapters/swarm-split.py`, and `sh scripts/install.sh` links it there.

Swarm.app opens on Home, where routed roles show their models. Open Project adds a folder to the
sidebar, even when it has no chats. Create Project makes a plain folder and adds it there. New Chat
lets the user choose a provider and model without changing a routed role. Codex and AGY list CLI models; Claude lists
aliases and accepts a full model name in Other model. In a chat, Switch model asks the live chair
for a compact summary, starts the chosen Claude or Codex model, and keeps both parts in one sidebar
row. If the old pane has closed, the new chair receives recent messages and makes its own compact
summary.

For a Git project, New Task starts a branch from the default branch in a worktree beside the
project, then opens New Chat there. Empty task worktrees stay available from the project view.
Plain folders keep the New Chat action without Git worktrees.
Chats in one workspace appear as tabs above the transcript. The plus button starts another chat
in that workspace; chats in other worktrees stay in their own tab groups.

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
`AGENTS.md`. `sh scripts/install.sh` links them into every agent CLI on the machine, with the host files the
skills and adapters run. `demo/herdr.sh` and
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
