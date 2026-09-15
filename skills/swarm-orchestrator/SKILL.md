---
name: swarm-orchestrator
description: Run a swarm session as the parent agent, which creates the session, spawns child agent CLIs into visible panes, sends them work, collects their answers, and cleans up. Use when asked to fan work out to other agent CLIs with swarm.
---

# Swarm orchestrator

Needs `swarm` on PATH (`cargo install --path .`), `swarm init` run once, and a pane host: tmux
(default, run inside tmux) or Herdr (`export SWARM_ADAPTER=herdr`, run from a Herdr pane).

## Session

```sh
export SWARM_SESSION_ID=$(swarm session new lane)   # lane | relay | open
export SWARM_AGENT_ID=orchestrator
swarm agent add orchestrator orchestrator
```

## Children

```sh
swarm spawn <id> <role> -- <agent cli and its flags>   # prints the pane id
```

`spawn` stamps `SWARM_HOME`, `SWARM_ADAPTER`, `SWARM_SESSION_ID`, and `SWARM_AGENT_ID` into the
pane; never put ids into the command. The child must know the voice protocol in
`skills/swarm-voice/SKILL.md`. Claude Code loads it when the pane's cwd is this repo, and
Codex and AGY find it through `AGENTS.md`; otherwise give it as the child's first prompt, for
example "Read <repo>/skills/swarm-voice/SKILL.md and follow it".

## Work

```sh
printf '%s' '<question>' | swarm send <id> ask   # stores the message and rings the child's pane
swarm inbox                                      # seq sender kind body_path, unread only
swarm ack <seq>
```

A child ends its turn after it answers. Ring it by sending; do not poll its pane.
In lane mode children cannot message each other. In relay mode you receive their child-to-child
messages as kind `relay:<recipient>` and forward them yourself.

## Health and cleanup

```sh
swarm sweep --every 5 &                                 # a dead pane arrives as a summary from that child
SWARM_SUMMARIZER='<command that reads a log on stdin>' swarm drain   # summarize children that died without finish
swarm close <id>
```
