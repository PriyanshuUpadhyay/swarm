---
description: Owner contract for a step run, a skill's work split into step files that any agent can pick up, close, and move past. Read when a skill says it runs as a step run.
---

# Step run

A step run keeps a skill's work in files, one file per step, so that any agent on any provider can
read the folder, finish every ready step, and move on. A chat holds no state that a step needs.

Example. A `research` run stops after `02-local.md` because the session ends. Next day a Codex
session gets "research continue <reports>/2026-10-05-browser-tool/". It reads the folder,
sees `02-local.md` done and `03-web.md` open, does the web step, and goes on to the report.

## Folder

- The skill names the folder. A run tied to a repository uses `references/run-folder.md`. A run
  whose result is a report keeps its folder next to the report, in the reports folder that the
  active runtime adapter names.
- The skill's step table lists the step files `NN-<name>.md`, what each one needs, and what it holds.
  On start, make every step file with `Status: open`.

## Script

A skill with a step table makes its folder with the kit's `references/step_run.py`, unless the skill
names its own script. `start <folder> <SKILL.md>` writes one file per table row, with a todo for what
the row holds and a `## Result` section. `take <folder> <step> <agent>` marks a step active and fills
its `Uses:` line. `done <folder> <step>` sets the step done only when every todo is checked with its
evidence after the colon. `done` accepts no `--force`. `status <folder>` also names each stale step.
Never set a step done by hand.
`take` refuses a step active for another agent; use `--force` after confirming that run is gone.
A dead run holds no lock; `--force` only overrides a stale `Status: active <agent>` line.
`take` and `done` add one line to `<folder>/events.log` with the time, the step, and the event, so the
path of a run, and not only its last state, stays readable. A skill script with its own step graph
or revision, such as flow's `flow_run.py`, imports these functions and passes its own.

## Status line

Line 1 of each step file is its status:

`Status: open | active <agent> | waiting <question> | blocked <reason> | done <revision> | skipped <reason> | unavailable <tool>`

- The revision of a step is the hash of its file below line 1,
  `tail -n +2 <file> | shasum | cut -c1-12`, unless the skill names another revision, such as a
  commit.
- Line 2 is `Uses: <step>@<revision>, ...` for each step that it needs.
- `start` writes the need names on line 2 with no revision (`Uses: 04-impact, 02-design`, or empty
  when the step needs nothing), and `take` adds each `@<revision>`. So the folder carries its own
  step graph for a reader without the script, such as the Swarm app. A step whose revision is a
  commit has a line `Revision: HEAD` below line 2, which the skill's script writes at start; a reader
  takes the first 12 characters of `git rev-parse HEAD` as that step's revision.
- A step is ready when each step that it needs is done or skipped.
- A step is stale when a revision in its `Uses:` line is no longer the current one. Its owner does it
  again or confirms that it still holds.

## Pick up

1. Read every step file's line 1. Take the step that the user named, or else every ready step.
   When the skill routes steps to seats, give each ready step to its seat in the same turn by
   `references/fan-out.md`.
2. `take` enforces the active owner. Check what a stale run's files hold before using `--force`.
3. Run `take <folder> <step> <agent>` and read only what its row in the skill's step table names.

## Close and move on

1. Write the step's result and its evidence below line 2, then set line 1 to `done <revision>`.
2. Go on to every step now ready in the same turn, by `references/fan-out.md`.

## When a step waits for the user

A step waits only for a choice that `AGENTS.md` says to ask about, a diff approval that the
skill's mode requires, or an external write or irreversible action. Set its status to
`waiting <question>` and put the question and the options in the reply. A waiting or blocked step
stops only itself; the other ready steps go on.

A step that cannot go on because an input is missing or two inputs disagree is `blocked <reason>`.
Name the missing input in the reply.

## End

When the last step is done, the skill's own final step says where the result goes. Keep the folder,
because it is the record of how the result was made.
