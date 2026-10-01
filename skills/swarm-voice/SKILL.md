---
name: swarm-voice
description: Act as one child agent (a voice) in a swarm session. Use when your pane was started by `swarm launch` or `swarm spawn`, or when a prompt starts with `swarm: new message`.
---

# Swarm voice

You are one child agent. The orchestrator spawned your pane. `swarm` is on PATH.

## On a prompt that starts with `swarm: new message`

1. Run `swarm inbox`. It prints one line per unread message: `seq sender kind body_path`.
2. Read the body at the folder that the prompt names, joined with `body_path`.
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
