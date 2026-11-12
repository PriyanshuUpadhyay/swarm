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

- Choose the folder by run kind in `references/run-folder.md`. The skill names its artifacts.
- The skill's step table lists the step files `NN-<name>.md`, what each one needs, and what it holds.
  Create the step files with the `start` command in Script below.

## Script

A skill with a step table makes and moves its folder with
`python3 <kit root>/references/step_run.py <start|status|take|done|skip|block|amend> ...`, unless the
skill names its own script. `--help` lists the arguments, and each refusal prints its reason.

- `take` prints the step file with each block's Rules and Example. Do the step from that print.
- `done` needs every todo checked, with its evidence after the last colon. In a plan run, write
  only the paths on the step's `Writes:` line.
- Never set a step done by hand. Pass `<n>` as a bare number, because an unquoted `#1` is a shell
  comment.
- Use `take --force` only after you confirm that the other agent's run is gone.

## Status line

Line 1 of each step file is its status:

`Status: open | active <agent> #<n> | waiting <question> | blocked <reason> | done <revision> | skipped <reason> | unavailable <tool>`

- The revision of a step is the hash of its file below line 1,
  `tail -n +2 <file> | shasum | cut -c1-12`, unless the skill names another revision, such as a
  commit.
- Line 2 is `Uses: <step>@<revision>, ...` for each step that it needs.
- `start` writes the need names on line 2 with no revision (`Uses: 04-impact, 02-design`, or empty
  when the step needs nothing), and `take` adds each `@<revision>`. So the folder carries its own
  step graph for a reader without the script, such as the Swarm app. A step whose revision is a
  commit has a line `Revision: HEAD` below line 2, which the skill's script writes at start; a reader
  takes the first 12 characters of `git rev-parse HEAD` as that step's revision.
- A step is ready when each step that it needs is done or skipped, and no done step on the chain
  before it is stale: its needed steps, the steps they need, and so on, up to a step that is not done.
  A stale step's result is old, so a step two links on would build on it too. When a needed step is
  neither done nor skipped, `take` says which steps it needs. When the chain holds a stale step, `take` and `done`
  name the stale step to take again, and any input that it waits for first.
- A step is stale when a revision in its `Uses:` line is no longer the current one. Its owner does it
  again or confirms that it still holds.

## Pick up

1. Read every step file's line 1. Take the step that the user named, or else every ready step.
   When the skill routes steps to seats, give each ready step to its seat in the same turn by
   `references/fan-out.md`.
2. `take` enforces the active owner. Check what a stale run's files hold before using `--force`.
3. Run `take <folder> <step> <agent>` and read only what its row in the skill's step table names.

## Close and move on

1. Write the step's result and its evidence below line 2, then run
   `python3 <kit root>/references/step_run.py done <folder> <step> <n>`, with the `<n>` from the `#<n>` that `take` printed.
   Use the skill's script when it names one.
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
