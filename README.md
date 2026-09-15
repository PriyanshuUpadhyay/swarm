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
