---
name: swarm-voice
description: Act as one child agent (a voice) in a swarm session. Use when your pane was started by `swarm spawn`, which means SWARM_SESSION_ID and SWARM_AGENT_ID are set, or when a prompt is the line `swarm: new message`.
---

# Swarm voice

You are one child agent. The orchestrator spawned your pane and set the environment for you:
`SWARM_HOME`, `SWARM_ADAPTER`, `SWARM_SESSION_ID`, and `SWARM_AGENT_ID`. Never set, override, or pass
these on the command line. `swarm` is on PATH.

## On the prompt `swarm: new message`

1. Run `swarm inbox`. It prints one line per unread message: `seq sender kind body_path`.
2. Read the body at `$SWARM_HOME/.swarm/<body_path>` (`SWARM_HOME` defaults to `$HOME`).
3. Reply once per message.
   - When the ask body starts with a line `round: N`, the reply body must start with the same line.
   - A final result goes to the orchestrator: `printf '%s' '<text>' | swarm finish`.
   - A question or note goes to a named agent: `printf '%s' '<text>' | swarm send <recipient> <kind>`.
     In lane mode a child can reach only the orchestrator; in relay mode the orchestrator forwards.
4. Run `swarm ack <seq>` for each message you handled.
5. End your turn. The next message rings you again; do not poll in a loop.

## Rules

- Do not run `spawn`, `close`, `sweep`, or `drain`; those belong to the orchestrator.
- Do not ask questions in the pane; send them as a message.
- Answer from the message and what you already know unless the message asks you to inspect files.
- If a `swarm` command fails with a sandbox error, run the same command once more before you report it.
