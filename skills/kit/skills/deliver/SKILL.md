---
name: deliver
description: Explicit autonomous delivery of one named task. Use only when the user invokes deliver and wants the agent to drive the task to a checkable exit condition without stopping. Never select it for an ordinary implementation request, and never for chunk-by-chunk work, which `pair` owns.
disable-model-invocation: true
---

# Deliver

You own the exit condition. State it, then drive to it without stopping.

Deliver runs in supervised mode. The agent picks the next step and edits without asking, and it
stops only before a merge, a push, or another irreversible action, or when it is blocked on input
only the user can give.

1. State the exit condition as a checkable predicate before the first change: tests green, the
   repro fixed, the feature exercised on the real surface. Show it in one line and start. Do not
   wait for approval of the predicate. When no `flow` folder is named, first run
   `python3 <flow-skill-dir>/scripts/flow_run.py start` and follow the `flow` skill's entry rule.
   With a `flow` folder, the predicate is the done-when of its `01-frame.md`, and you hold its
   build step. Before the first change, apply the rules and the todo list in its `05-build.md`.
2. Each iteration makes the smallest change the evidence justifies, verifies it against the
   predicate, commits when it advanced, and discards a change that did not help. A change that
   "might help" is reverted, not left to ride. Order the work per the
   `sequence-verifiable-units` skill, and judge each check per the `prove-it-works` skill.
3. A mid-run discovery is yours when it blocks the predicate. Fix it, such as a broken helper or a
   flaky check, in its own commit, then return to the predicate. Do not park reversible work for
   the user. Report any other defect in the final result and leave it unfixed.
4. Keep one task file `<YYYY-MM-DD>-<slug>.md` in the task-file folder that the active runtime
   adapter names, with the predicate, the iterations run, one evidence line per iteration, and the
   next step. Read it first after a `/clear`, a `/compact`, or a resume, and make the first reply a
   status block.
5. When the predicate holds, run `review-check` on `<base>..HEAD`, where `<base>` is the commit
   before this run's first commit. A fix seat may start on one review seat's validated `fix` rows
   after a partial verdict finds no problem in those rows, in its own worktree from the reviewed head.
   Later rows go to the same fix seat as one more ask. The verdict word and the next review start
   wait for every review seat. Fix each `fix` row in one more iteration, then review the
   new range. Handle each `ask` as review-check's Asks section says: answer-only work keeps
   HEAD and the range fixed, then run `verdict` again on the same run. In a flow, you also hold
   the review step, and the last verdict goes in
   `06-review.md`, because it covers the build range. Then print the flow's `Next:` line for the
   close step.
6. Stop only when the predicate is met and the review has no `fix` row, or when blocked on
   user input. After three review rounds that still return `fix` rows, stop and bring the rows
   to the user with their weight (crash, wrong state, or hygiene), and let the user choose to
   go on or to record them as known limits. Before stopping, write each limit the user accepts as
   a `limit:` line in the run's `answers.md`, in review-check's answer shape with the user's words,
   so it reaches the next run's `answers-before.md`. A plateau is not a stop,
   so change the approach. A genuine dead end is reported with the evidence, not spun on. Never
   relax the predicate to declare victory.

The active host contract decides who types. Under a Fable chair the edits go to a routed coding
seat, every worker is a visible foreground pane, and the chair verifies each returned diff itself.
The Parallel fix seats (opt-in) section below names the exception.
Notifications are chair-owned.

## Commit gate

The worker's `unit N ready` message carries the changed paths, its check output, and
`git diff HEAD | shasum`. The chair hashes the same diff. A matching hash lets the chair stage
the reported paths, commit, and send `committed` in one call. The chair then runs the unit's check
on that commit in the background, in a persistent detached git worktree under `tmp/deliver/verify`.
The worker edits N+1 at once, but its commit waits for N green. A red check stops landings; keep
the work, diagnose, then fix or revert in a checked unit. Never revert automatically. A hash
that does not match keeps the current gate; the chair runs the check before commit.

## Parallel fix seats (opt-in)

Only when the user opts in for this run, use at most two fix seats. Before launch, the chair
declares each seat's file set and the shared interfaces that stay frozen. Each seat uses its own
worktree. Run `python3 <skill-dir>/scripts/land.py <decl.json> <worktree>...`, where the declaration
maps each worktree to its file list and names the frozen files; it refuses overlapping files.
The chair lands in the declared order and runs each unit's check at its stack position.
Feature units stay serial. Without opt-in, use one fix seat. The kit's `references/fan-out.md`
owns fan-out policy.

**Reply:** the exit condition, the iterations run, what landed with its hashes, what was
discarded, the final predicate state with its proof, and the review verdict with any open QUESTION.
