# Fan out

Example. Two ready source steps need only the question. Start both in one turn, then merge
after their results pass their checks. The calling workflow owns which inputs each step needs.

## When to fan out

A step, seat, read, ack, or send that needs no result from a sibling in the same
stage goes out in the same turn as its siblings. Launch all seats of a stage in
one turn; one command with a loop is fine. Send all asks in one turn. Put
independent reads, acks, and sends in one command.

## Chair work while seats run

Write the next brief, run the next start, and read what the next step needs.
End the turn when nothing is ready, because a seat's finish rings the chair.
Take each result when it arrives and act on it. Wait for all seats only when
the step's verdict needs all of them.

## Work that runs in sequence

- Edits to shared state or the same file.
- A step that reads another step's result.
- Landing and integration.
- Verification of a stack position.
- Deep review seats; a run has only one at a time.
- More seats than the caps allow.

## Caps

- 8 live child seats per run, including fix seats.
- 2 coding seats.
- 3 shards per cheap review aspect.
- 1 live deep review seat.

## Liveness

The host owns liveness. Run `swarm sweep` as the `swarm-orchestrator` skill says.
