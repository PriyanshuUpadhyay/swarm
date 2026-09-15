# swarm

Message bus and pane control for a tree of agent CLIs. One orchestrator spawns children into
terminal panes (tmux or Herdr), the agents talk through a SQLite inbox, and a drainer summarizes
the transcript of any child that exits without a summary.

## Install

```sh
cargo install --path .
swarm init          # creates $SWARM_HOME/.swarm with the db, runs/, and the shipped adapters
```

## Environment

| Variable | Meaning |
|---|---|
| `SWARM_HOME` | Root for `.swarm/`. Defaults to `$HOME`. |
| `SWARM_ADAPTER` | Adapter file name under `.swarm/adapters/`. Defaults to `tmux`. |
| `SWARM_SESSION_ID` | Session the caller belongs to. `spawn` stamps it into each child pane. |
| `SWARM_AGENT_ID` | Identity of the caller. `spawn` stamps it into each child pane. |
| `SWARM_SUMMARIZER` | Shell line `drain` runs with a log on stdin. Required by `drain` only. |

## Commands

Caller `any` needs no identity. `session` needs `SWARM_SESSION_ID`. `agent` needs both ids.

| Command | Caller | Effect and output |
|---|---|---|
| `init` | any | Create `.swarm/`, the db, `runs/`, and the shipped adapter files. |
| `adapter check <name>` | any | Parse `.swarm/adapters/<name>.conf`, print `ok <name>`. |
| `session new <lane\|relay\|open>` | any | Create a session, print its id. |
| `drain` | any | Run queued summarize jobs, print `done`, `retry`, or `parked` per job. |
| `agent add <id> <role>` | session | Register an agent without a pane. |
| `spawn <id> <role> [-- <cmd>...]` | session | Register, split a pane, run `<cmd>; swarm exited` in it, print the pane id. |
| `close <id>` | session | Close the pane of `<id>` and forget it. |
| `send <recipient> <kind>` | agent | Store stdin as a message, ring the recipient, print the seq. |
| `finish` | agent | Send stdin as a `summary` to the orchestrator, print the seq. |
| `exited` | agent | Capture the own pane to `runs/<session>/<id>.log`, report a missing summary. |
| `sweep [--every <secs>]` | agent | Report each child whose pane is gone, print `dead <id>`. With `--every`, repeat every N seconds and warn instead of exit on a failed pass. |
| `inbox` | agent | Print `seq sender kind body_path` per unread message. |
| `ack <seq>` | agent | Mark one message read. |

## Walkthrough

Run the orchestrator inside tmux, so `spawn` has a pane to split.

```sh
export SWARM_SESSION_ID=$(swarm session new lane)   # lane: children talk only to the orchestrator
export SWARM_AGENT_ID=orchestrator
swarm agent add orchestrator orchestrator
swarm spawn coder coder -- my-agent --task "write the parser"   # prints the pane id, e.g. %3

# in the coder pane, SWARM_SESSION_ID and SWARM_AGENT_ID=coder are already set
echo "parser done, tests green" | swarm finish        # prints the seq, e.g. 1

# back in the orchestrator pane
swarm inbox                                            # 1 coder summary runs/1/1.txt
swarm ack 1
swarm close coder

# a child that exits without finish gets a fallback summary and a summarize job
SWARM_SUMMARIZER='head -c 200' swarm drain            # done 1
```
