---
name: swarm-orchestrator
user-invocable: false
description: Run a swarm session as the parent agent. It creates the session, spawns child agent CLIs into visible panes, sends them work, collects answers, and cleans up. Use when asked to fan work out to other agent CLIs.
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
with the diff it printed and the `swarm setup trust --plan …` command it names; do not answer
the trust question yourself. Use `swarm spawn <id> <role> -- <agent cli and its flags>` only for a command no
role covers. The child must know the voice protocol in the `swarm-voice` skill, which the
plugin `swarm` installs in each agent CLI; otherwise give it as the child's first prompt, for
example "Read ~/.swarm/kit/skills/swarm-voice/SKILL.md and follow it".

Launch every seat of a stage in one turn and send their asks in one turn, as the agent kit's
`references/fan-out.md` says.

## Work

Seat identifiers carry the run id. Write long briefs to a file, and send a one-line pointer as the ask.

```sh
printf '%s' '<question>' | swarm send <id> ask   # stores the message and rings the child's pane
swarm inbox                                      # seq sender kind body_path, unread only
swarm ack <seq>
```

Apply `blocks/run-workers/SKILL.md` for result acceptance; this host's durable result channel is
its inbox message from the named seat. The kit root is two folders up from this skill's folder.

A child ends its turn after it answers. Ring it by sending; do not poll its pane.
A child's `finish` or `send` rings your own pane with `swarm: new message`; on that prompt run `swarm inbox`, read the message, and `ack` it.
After a send, end your turn. Do not wait with `sleep`, and do not loop on `swarm inbox` or a pane read; the ring starts your next turn, and the sweep below reports a seat that went quiet.
When running rounds, put `round: N` on the first line of every ask and discard a summary whose first line does not match the current round.
In lane mode children cannot message each other. In relay mode you receive their child-to-child
messages as kind `relay:<recipient>` and forward them yourself.

## Project board

You are the board's lead. Run `swarm board read` before you write the first brief, and put each
post that applies to the task in that brief. Post with `swarm board post <kind>`, and
answer with `swarm board answer B-n`. Run `swarm board` for the flags; a refused post prints how to fix it.

## Reports to the owner

Give each report, plan, review, or status the form that the kit's `references/explain-formats.md`
sets. The kit root is two folders up from this skill's folder.

## Owner

Swarm notifies the owner when any agent waits on a permission or a question. When the run's work
is done, or you need the owner for something else, run
`swarm notify "swarm: <project> done" --body "<one line>"`. This is the only way to notify the
owner, and only the orchestrator may run it.

## Health and cleanup

```sh
swarm sweep --every 30                                  # a dead pane arrives as a summary from that child
swarm close <id>...                                     # ask for a wrap-up, save it, close the seat named by run-workers; waits up to 120 s, so give the call a 180 s timeout
swarm close --now <id>                                  # close at once, with no wrap-up
```

Run the sweep as a background task of your agent harness, never with `&` in a detached shell, so it
stops with the session.

The sweep also sends you a message in a child's name when its work is stuck. `unconfirmed:<seq>`:
two rings of message `<seq>` started no turn; check the pane and send again. `stall:unacked:<seq>`:
the child is done but did not ack `<seq>`. `stall:silent:<seq>`: the child finished after `<seq>`
and sent nothing back. Ack each one like any other message.
