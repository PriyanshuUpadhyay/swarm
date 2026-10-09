---
name: flow
description: Run one change through seven steps (frame, design, contracts, impact, build, review, close), with one status file per step under tmp/flow at the repo root and, for each step, the skills that the domain catalog gives the repo's domains. Use when the user says "flow continue" with a folder or "flow start", names a tmp/flow step file, or when pair or deliver starts with no flow folder.
---

# Flow

Use a flow for any change that `pair` or `deliver` builds. A one-line change takes the fast lane.

## Entry and lanes

`/pair`, `/deliver`, and "flow start" all use this route. When `pair` or `deliver` starts with no
flow folder named, make the folder and do `01-frame.md` first. Then pick the lane from the change
and the repo's contracts, never from the missing folder. The fast lane skips `02-design.md`,
`03-contracts.md`, and `04-impact.md` with `skip <folder> <step> fast lane: <reason>` only when none
of them needs a new decision. Otherwise do them before build. The command keeps its own mode.
After "flow start", ask the user to choose `pair` or `deliver` in the reply that shows `01-frame.md`.

When a step ends and the next step is not yours, print `Next: <command>` as the last line of the
reply, for example `Next: flow continue tmp/flow/2026-10-05-login 06-review`.

## Start or continue

The user says "flow continue <folder>" in the repo. The folder is
`<repo-root>/tmp/flow/<YYYY-MM-DD>-<branch>/`, as the kit's `references/run-folder.md` says.
The profile stays private in `~/.flow/<repo>/profile.md`. `<repo>` is the name of the folder above
the git common dir (`git rev-parse --git-common-dir`), not the worktree folder name.

1. If `~/.flow/<repo>/profile.md` does not exist, stop. Suggest each domain of the catalog
   `~/.flow/domains.md` whose `Signals:` line matches a file in the repo. Draft the profile with
   `Domains:`, `Repo rules:` (files in the repo that hold its own rules), `Checks:` (the commands
   that close runs), and `Needs:` (the tools that a step uses). Show the draft, and write
   `~/.flow/<repo>/profile.md` only after the user confirms it. Tell the user that the new profile
   is not in version control yet.
2. Run the script from the repo root. `F=<skill-dir>/scripts/flow_run.py`.

   ```sh
   python3 $F start                          # makes the folder and step files, writes each step's skills into its file in full
   python3 $F status [<folder>]              # each step's status, which open step is ready, and which is stale
   python3 $F take <folder> <step> <agent>   # marks the step active and prints its file with its skills in full
   python3 $F done <folder> <step>           # sets the step done, only when every todo has its evidence
   python3 $F skip <folder> <step> <reason>  # skips 02-design, 03-contracts, or 04-impact, never another step
   ```

3. Read `01-frame.md`. If `Worktree:` or `Branch:` is not the current worktree and branch, stop and
   tell the user. If two open folders exist for one branch, stop and tell the user.
4. Pick up a step as `references/step-run.md` says, and take it with `take`. The step file holds the
   full text of the skills that the catalog gives the step (the `any` section, each profile domain,
   and the repo rules) under `## Rules for this step`, and a todo list. Apply the rules, check each
   todo with its evidence after the colon, write the result under `## Result`, and close the step
   with `done`. Never set a step done or skipped by hand. When a tool
   that the profile names is missing, set the status to `unavailable <tool>` and tell the user.
5. Write only your step's file. Line 1 is the status.

## Status line

The flow is a step run, so the kit's `references/step-run.md` owns the status line, the `Uses:`
line, pick-up, close, and when a step waits for the user. The step table below is the step graph that
the script reads. The revision of `05-build` is `HEAD`, so a new commit makes `06-review` stale.
`start` marks `05-build.md` with the `Revision: HEAD` line that step-run.md describes.

## Steps

`02-design` and `03-contracts` need only frame, so both run by the pick-up rule in
`references/step-run.md`; the frame names their shared assumptions.

| File | Needs | Holds |
|---|---|---|
| `01-frame.md` | none | `Worktree:`, `Branch:`, goal, user, out of scope, done-when, and shared assumptions for design and contracts |
| `02-design.md` | frame | UX flow and screens, or `skipped: no UI` |
| `03-contracts.md` | frame | APIs, data model, integrations, and the result of the `decisions` skill's check for the topic |
| `04-impact.md` | contracts | the existing features that change, or "independent", judged from the `## Callers` section that `take` writes with the places that name each code name in backticks in `03-contracts.md` |
| `05-build.md` | impact, and design unless skipped | the task-file path of the `pair` or `deliver` run that the user started, and `Base:`, the commit before its first commit |
| `06-review.md` | build | the `review-check` runs on `<Base>..<build revision>`, line 3 of the newest run's `03-verdict.md`, each `fix` row with its fix or the reason it stays, and one `Run: tmp/review-check/<run>` line per round, newest last |
| `07-close.md` | review with APPROVE | the done-when of `01-frame.md`, each with its evidence |

`take 07-close` reads line 3 of the last run's own `03-verdict.md` and refuses unless it is APPROVE
and the run's head is `HEAD`. A NEEDS DISCUSSION verdict closes only after the user's words go into
the run's `answers.md` as answers or `limit:` lines per review-check's Asks section, and `verdict`
runs again to APPROVE. A REQUEST CHANGES verdict goes to the user, who starts a `pair` or `deliver` run for the
findings. That run moves the build revision, so the review is stale and runs again on the new
range. Answer each `ask` in the run's `answers.md` per review-check's Asks section, and stop
and ask the user after three review rounds on this step. `deliver` takes `06-review` again for each
review round, so the warning counts real runs; the cap is a warning, not enforced.

At close, move the folder to `<repo-root>/tmp/flow/_closed/<folder>/`. Release is a separate action
that the user asks for.

## Owners

The flow owns the step list, the order, the status, the profiles, and the domain catalog.
`pair` or `deliver` owns the plan, the chunks, and the commits; the flow never starts `deliver`
by itself.
`review-check` owns the review and its verdict; the flow never fixes a finding itself. An `INCOMPLETE` verdict is no review.
`prove-it-works` owns the choice of check and the evidence. `sequence-verifiable-units` owns the
unit order. `decisions` owns the ADRs. The flow never pushes.

## Trial log

This skill is on trial. At the end of each step file, add two lines:

- `Skills read: <paths>` (the script writes it when you `take` a step)
- `Helped: yes | no, <one example>`

Also add a line for each stall, wrong skill, or repeated manual step. These lines decide if the
skill stays, changes, or is removed.
