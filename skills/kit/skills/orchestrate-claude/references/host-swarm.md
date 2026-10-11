# Orchestrate from Claude — Swarm host reference

This reference is active under the `[agent-host: herdr]` host contract.

## Session

Create the swarm session. It records this pane as the orchestrator, so a child's finish rings the chair.

```sh
swarm session new lane
```

## Seats

Follow the kit's `references/fan-out.md` for seat launches, asks, and chair work.

Spawn each seat with `swarm launch`.

```sh
swarm launch <id> <role> --cwd <abs path> [-- <extra>]
```

The route owns model, effort, sandbox, and approval settings.
Seat identifiers must carry the run id.

## Work

Write long briefs to a file, and make the ask prompt a one-line pointer to that file.
When you run rounds, write `round: N` on the first line of the ask.

Send the ask prompt to a seat.

```sh
printf '%s' '<ask>' | swarm send <id> ask
```

A reply arrives as a `swarm: new message` prompt in the chair pane.
Read the inbox, read the body file at the folder the prompt names, and acknowledge the message.

```sh
swarm inbox
# Read <folder from the prompt>/<body_path>
swarm ack <seq>
```

Discard any summary if its first line does not match the current round.
Completion is the accepted inbox message, and you must never read the screen for completion.

## Report-only seats

The seat brief must forbid repository edits, commits, worker spawns, and user notifications.
The seat writes only its attributed result to scratch and sends that result with `swarm finish`.

## Uptake

The sweep command re-rings an ask at most once, 60 seconds after the first ring, and only while
the child has not read its inbox.
The `swarm-orchestrator` skill owns how to run the sweep.

If a child pane dies, the orchestrator receives a summary message from that child.

## Close-out

Run `swarm close <id>` for every seat that this run spawned.

```sh
swarm close <id>
```

Confirm the closure with the pane list of the active host.

## Open items

Two open items exist today.
1. No Stop-hook gate covers swarm seats (the old completion gate was removed with herdr-ops).
2. The sandboxed-Codex drop box is not on the swarm path.
