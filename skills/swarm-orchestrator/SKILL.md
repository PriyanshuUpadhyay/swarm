---
name: swarm-orchestrator
description: Run a swarm session as the parent agent, which creates the session, spawns child agent CLIs into visible panes, sends them work, collects their answers, and cleans up. Use when asked to fan work out to other agent CLIs with swarm.
---

# Swarm orchestrator

Needs `swarm` on PATH (`cargo install --path .`), `swarm init` run once, and a pane host: tmux
(default, run inside tmux) or Herdr (run from a Herdr pane; swarm picks it there).

## Session

```sh
swarm session new lane   # lane | relay | open; registers this pane as the orchestrator
```

## Children

```sh
swarm launch <id> <role> --cwd "$PWD" [-- <extra agent flags>]   # prints the pane id
```

`launch` resolves the role to a provider, model and effort, marks the directory trusted when the
owner gave consent, and starts the agent CLI. When it prints `trust-pending <provider> <dir>`, the
owner has not approved folder trust: the seat waits at its CLI's trust question. Tell the owner,
with the diff it printed and the `swarm setup --plan …` command it names; do not answer
the trust question yourself. Use `swarm spawn <id> <role> -- <agent cli and its flags>` only for a command no
role covers. The child must know the voice protocol in
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
A child's `finish` or `send` rings your own pane with `swarm: new message`; on that prompt run `swarm inbox`, read the message, and `ack` it.
When running rounds, put `round: N` on the first line of every ask and discard a summary whose first line does not match the current round.
In lane mode children cannot message each other. In relay mode you receive their child-to-child
messages as kind `relay:<recipient>` and forward them yourself.

## Health and cleanup

```sh
swarm sweep --every 30                                  # a dead pane arrives as a summary from that child
SWARM_SUMMARIZER='<command that reads a log on stdin>' swarm drain   # summarize children that died without finish
swarm close <id>
```

Run the sweep as a background task of your agent harness, never with `&` in a detached shell, so it
stops with the session.

The sweep also sends you a message in a child's name when its work is stuck. `unconfirmed:<seq>`:
two rings of message `<seq>` started no turn; check the pane and send again. `stall:unacked:<seq>`:
the child is done but did not ack `<seq>`. `stall:silent:<seq>`: the child finished after `<seq>`
and sent nothing back. Ack each one like any other message.
