---
name: skill-eval
description: "Test whether a skill helps. Check that cold models understand its name, run real tasks with and without it in panes, grade them blind, then keep, fix, or cut it. Use when you add, change, rename, or doubt a skill."
---

# Skill eval

Example. Initial state: the owner wants a new skill `pair-programming`.
Action: three cold models explain the name, then two real tasks run in four panes, each task once
with the skill and once without it, and a blind grader compares each pair.
Result: the name passes, the skill wins one task and ties the other on both providers, so it is
kept, and one row per task goes into `evals/results.md`.

The kit root, with its `blocks/` and `references/` folders, is two folders up from this skill's
folder. This skill's own `references/` folder holds the grader prompts.

A skill earns its place only when runs with it beat runs without it. Before you build or change a
skill, check that the same failure happened in at least two real runs.

## Inputs

- The skill folder under test, which holds `SKILL.md`.
- The baseline. A new skill runs against no skill. A changed skill runs against a copy of its last
  committed version, made with `git show HEAD:<path>` into the scratch folder.
- The cases file, `<skill>/evals/cases.md`. Step 2 writes it when it is missing.

Scratch work goes in `~/.swarm/ws/skill-eval/<skill>-<YYYY-MM-DD>/`. Each pane gets its own empty
`mktemp -d` folder as its working folder, so no run edits a repository.

An eval spans many turns, so keep a task file as `<kit root>/blocks/task-file/SKILL.md` says, at
`tmp/skill-eval/<YYYY-MM-DD>-<skill>.md`. Besides the block's fields, it holds the scratch path, the
round, the pane and grader names with their ask numbers, the grades received so far, and each
result after unblinding.

## Steps

### 1. Name check

Open three panes in one empty folder with no skill or repository files. Use role `chat` with
`--provider claude`, role `chat` with `--provider codex`, and role `council.gemini`. Send each the
same ask, with the skill name and any names you compare it with:

> Answer from your own knowledge only. Do not read files, search, or load any skill. A
> coding-agent plugin has skills with the names below. For each name, write one line that says
> what you expect the skill to do. If the name gives you no clear meaning, write "unsure" and your
> best guess.

A name passes when all three lines describe the skill's job and none says "unsure". When a name
fails, test two or three known names for the same job, and stop until the owner picks one. Close
the panes and remove the folder.

### 2. Cases

Write two or three cases into `evals/cases.md`, each in this shape:

```markdown
## case: <short-name>
Source: <run file and line, or "owner, <date>: <quote>">
Task: <the task as a user would type it>
Files: <none, or the files to copy into each pane's folder>
Check (script): <a command that exits 0 on success, or none>
Expect:
- <one fact a reader can see in the output>
```

- Take each case from a real failure, in this order: a saved step line `Helped: no`, a wrong skill
  or wrong route in a Trial log, a correction the owner made in chat, and last the skill's own
  example.
- Write the task as a user would. Never name the skill or tell the agent to use it.
- Give each expectation one fact that a wrong output cannot meet by chance. A script check
  decides before any model judge.
- Show the cases to the owner before the first run, because the cases decide the verdict.

### 3. Runs

For each case, run the skill side and the baseline side on two providers, `code.routine` with
`--provider claude` and with `--provider codex`. Tell the owner the pane count (cases × 2 × 2)
before launch, then launch all panes of a case at once. Follow `<kit root>/blocks/run-workers/SKILL.md`.

Both sides get the same brief except for one line:

- Skill side: "Before you start, read `<path>/SKILL.md` and follow it."
- Baseline, new skill: "Do not load or read the skill `<name>`."
- Baseline, changed skill: "Before you start, read `<snapshot>/SKILL.md` and follow it."

Each pane works in its own folder and writes `notes.md` there with the steps it took and the files
it read.

### 4. Blind grade

1. Run each script check on both folders, and record the exit codes.
2. Flip a coin for each pair with `python3 -c 'import random; print(random.choice("AB"))'`. That
   letter marks the skill side. Write the key in the scratch folder and never send it.
3. Copy each pair into a new `mktemp -d` folder as `A/` and `B/`, because a path under the
   scratch folder names the skill. In each `notes.md`, replace the skill name, each skill,
   preset, or brief file, and each pane folder path with a neutral phrase such as "the guide"
   or "the working folder". Keep the rest of the line, because an expectation can depend on it.
   Remove only the sentence that says the skill was read or not read. Then search both copies
   for the skill name, `skill`, and `base`. In the first run, notes showed which side read the
   skill and a folder path named it. In a later run, removing whole lines deleted the evidence
   that one expectation checked.
4. Open one `checker` pane per pair, on a provider other than the one that ran the pair. Send it
   `references/grader.md`, `references/comparator.md`, the task, the expectations, the script
   results, and the two folders as A and B. Tell it that lines were removed from `notes.md`.
5. The grader must not learn the skill name, which side is new, or any target. A judge that knows
   the goal scores toward it.

The grader returns a pass or fail for each expectation with a quote, a winner or a tie, and any
weak expectation it found. Use no 1 to 10 scores, because the same output gets different scores
in a different context.

### 5. Verdict

Unblind with the key, then score each case per provider as win, tie, or loss for the skill.

- **Keep** when the skill wins at least one result and loses none, across all cases and
  providers. With fewer than three cases, call it a weak keep and add cases.
- **Fix** when it loses a result. That case is the fix target. Rerun all cases after the fix.
- **Cut** when it wins no result. The skill adds nothing, so propose removal.
- **Split** when one provider wins and the other loses on a case. Rerun that case once. If it
  stays split, report the skill as dependent on the model. A win and a tie are not a split.
- When the grader names a weak expectation, fix the case before the next run.
- A baseline `notes.md` that shows the skill was read voids that pair. Rerun it.

A skill installed from the web is never edited. When it fails a case, give the owner a choice
between removing it and replacing it with a skill of your own.

### 6. Shrink

After a keep, cut text from the skill and rerun the cases against the kept version. Keep a cut
only when no case gets worse. Stop when a cut makes a case lose.

### 7. Record

Add one row per case and provider to `<skill>/evals/results.md`:

```markdown
| date | skill commit | baseline | case | provider | result | grader note |
```

Close every pane, then remove the scratch folder and the pane folders.

## Reply

Open with the first screen from `<kit root>/references/plan-layout.md`. Give the verdict, a
table of case, provider, and result, and each failed expectation with its quote.
