---
status: accepted
date: 2026-11-24
deciders: [user]
amends: ["0041"]
related: ["0154", "0159"]
informed-by:
  - 'owner, 2026-11-24: "we should not send a swarm message unless and until something blocking is cleared"; "whenever that question is resolved ... an automated check ... if there is a pending message ... it will just deliver it."'
  - 'owner, 2026-11-24: the listing sends the chair one `stall:error:<seq>` when a child goes to `error` with an open ask, with the error text.'
---

# 0160. A ring waits for an open question, and a failed child is a stall

## Context and Problem Statement

A ring types its text into the recipient's pane and presses Enter. On 2026-11-24 a board post rang
the chair while the chair's Pi `ask_user_question` dialog was open. The Enter chose the dialog's
first choice, so the chair took an answer that the owner never gave. The owner's own send
(`swarm type`) already refused a pane with a question; the ring did not.

In the same session a review seat's first turn failed to sign in, and its state was `error`. ADR
0041 reports a child at `done` only, and the ring had proved a turn, so no report told the chair.
The owner had to tell it.

## Decision

- Before a ring types, it checks whether the recipient waits on a question: its stored state is
  `blocked`, or its pane shows a question or a Pi dialog. If so, it types nothing and undoes the
  ring mark. The hold is not a ring, so it never makes a message lost.
- A recipient whose unseen messages are all unrung, as a hold leaves them, is due for a ring at
  once. The listing rings it as soon as its pane shows no question and its state is not `blocked`,
  even while it works, as its first ring would have.
- A child at `error` whose last proven ask has no answer since is a stall, `stall:error:<seq>`.
  It is sent once, like the other stalls, and its body names the stored error text.
