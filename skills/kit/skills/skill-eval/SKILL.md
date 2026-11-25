---
name: skill-eval
description: Test whether a skill helps, then keep, fix, or cut it. Use when you add, change, rename, or doubt a skill.
---

# Skill eval

Example. Initial state: the owner wants a new skill `pair-programming`.
Action: three cold models, one per family, explain the name. Then two real tasks run on the Claude,
GPT, and Gemini families, with the skill, without it, and with a same-length filler text, three
runs each, and a blind grader compares each pair.
Result: the name passes, the skill wins one task and ties the other on all three families, so it
is kept, and one row per task and family goes into `evals/results.md`.

The kit root, with its `blocks/` and `references/` folders, is two folders up from this skill's
folder. This skill's own `references/` folder holds the grader prompts.

A skill earns its place only when runs with it beat runs without it. Before you build or change a
skill, check that the same failure happened in at least two real runs.

## Families

The kit must work on each model family the owner uses, so every step runs on all three. Resolve each
role with `swarm roles get <role>` and record the routed model and effort; when a role resolves to
another family, use its next fallback of the right family or tell the owner.

| Family | Role |
|---|---|
| Claude | `code.routine` |
| GPT | `code.complex` |
| Gemini | `council.gemini` |

## Inputs

- The skill folder under test, which holds `SKILL.md`.
- The baseline. A new skill runs against no skill. A changed skill runs against a copy of its last
  committed version, made with `git show HEAD:<path>` into the scratch folder.
- The cases file, `<skill>/evals/cases.md`. Step 2 writes it when it is missing.

Scratch work goes in `<repo-root>/tmp/skill-eval/<skill>-<YYYY-MM-DD>/`, as
`references/run-folder.md` says. Each pane gets its own empty
`mktemp -d` folder as its working folder, so no run edits a repository.

An eval spans many turns, so keep a task file as `<kit root>/blocks/task-file/SKILL.md` says, at
`tmp/skill-eval/<YYYY-MM-DD>-<skill>.md`. Besides the block's fields, it holds the scratch path, the
round, the pane and grader names with their ask numbers, the grades received so far, and each
result after unblinding.

## Steps

### 1. Name check

Open one pane per family in one empty folder with no skill or repository files. Send each the
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

- Take each case from a real failure, in this order: a stall or a wrong skill or
  wrong route in a Trial log, a correction the owner made in chat, and last the skill's own
  example.
- Write the task as a user would. Leave the skill name out and do not tell the agent to use it, because naming it would void the load test.
- Give each expectation one fact that a wrong output cannot meet by chance. A script check
  decides before any model judge.
- Show the cases to the owner before the first run, because the cases decide the verdict.

### 3. Runs

For each case, run three arms on each family, three runs per arm: the skill, the baseline, and a
placebo. Effects differ between runs and between model families, so one run per cell cannot show a
win. Tell the owner the pane count (cases × 3 arms × 3 families × 3 runs) before launch, and launch
in waves within the seat caps of `<kit root>/references/fan-out.md`. Follow
`<kit root>/blocks/run-workers/SKILL.md`.

All arms get the same brief except for one line:

- Skill: "Before you start, read `<path>/SKILL.md` and follow it."
- Baseline, new skill: "Do not load or read the skill `<name>`."
- Baseline, changed skill: "Before you start, read `<snapshot>/SKILL.md` and follow it."
- Placebo: "Before you start, read `<placebo>/SKILL.md` and follow it." The placebo has about the
  same length as the skill and neutral advice that does not touch the task, so a win over it comes
  from the skill's content, not from its length.

Each pane works in its own folder and writes `notes.md` there with the steps it took and the files
it read.

### 3b. Trigger

A skill that never loads helps no one, and loading fails apart from following. Each family loads the
version under test with its live login, because a temporary home has no login:

- Start a `pi` seat with `swarm spawn <id> <role> -- pi --model <model> --thinking <effort> --no-skills
  --skill <worktree>/skills/kit/skills`, with the model and effort of its role. `swarm launch` refuses
  `--no-skills`, because the role owns the skill flags. The seat loads only the worktree's skills,
  `swarm-voice` included, and the live plugin stays unchanged.
- A CLI with no such flag, such as `agy`, needs the version under test in its live home. Ask the
  owner first, then install it, and put the live plugin back after the run.

On each family, with no hint, send three requests: one that names the skill, one that needs it
but does not name it, and one nearby request that must not load it. Record from `notes.md` whether
the skill was read. A miss on the unnamed request or a load on the nearby one is a trigger
failure; fix the description first.

### 4. Blind grade

1. Run each script check on both folders, and record the exit codes.
2. Flip a coin for each pair with `python3 -c 'import random; print(random.choice("AB"))'`. That
   letter marks the skill side. Write the key in the scratch folder and keep it from the grader.
3. Copy each pair into a new `mktemp -d` folder as `A/` and `B/`, because a path under the
   scratch folder names the skill. In each `notes.md`, replace the skill name, each skill,
   preset, or brief file, and each pane folder path with a neutral phrase such as "the guide"
   or "the working folder". Keep the rest of the line, because an expectation can depend on it.
   Remove only the sentence that says the skill was read or not read. Then search both copies
   for the skill name, `skill`, and `base`. In the first run, notes showed which side read the
   skill and a folder path named it. In a later run, removing whole lines deleted the evidence
   that one expectation checked.
4. Open one grader pane per pair, on a family other than the one that ran the pair (for example
   `checker` for a GPT or Gemini pair, `review.hunter` for a Claude pair). Send it
   `references/grader.md`, `references/comparator.md`, the task, the expectations, the script
   results, and the two folders as A and B. Tell it that lines were removed from `notes.md`.
5. The grader must not learn the skill name, which side is new, or any target. A judge that knows
   the goal scores toward it.

The grader returns a pass or fail for each expectation with a quote, a winner or a tie, and any
weak expectation it found. Use no 1 to 10 scores, because the same output gets different scores
in a different context.

### 5. Verdict

Unblind with the key, then score each case per family as win, tie, or loss for the skill against
the baseline and against the placebo. A result is the majority of its three runs; a win, a tie, and a loss count as a tie.

- **Keep** when the skill wins at least one result and loses none, across all cases and
  families, and does not lose to the placebo. With fewer than three cases, call it a weak keep and add cases.
- **Fix** when it loses a result. That case is the fix target. Rerun all cases after the fix.
- **Cut** when it wins no result. The skill adds nothing, so propose removal.
- **Split** when one family wins and another loses on a case. Rerun that case once. If it
  stays split, report the skill as dependent on the model. A win and a tie are not a split.
- When the grader names a weak expectation, fix the case before the next run.
- A baseline `notes.md` that shows the skill was read voids that pair. Rerun it.

Leave a skill installed from the web unedited, because the owner does not own it. When it fails a case, give the owner a choice
between removing it and replacing it with a skill of your own.

### 6. Shrink

After a keep, cut text from the skill and rerun the cases against the kept version. Keep a cut
only when no case gets worse. Stop when a cut makes a case lose.

### 7. Record

Add one row per case and family to `<skill>/evals/results.md`, with the routed model and effort,
so a later model change is visible:

```markdown
| date | skill commit | baseline | case | family | model | effort | trigger | vs baseline | vs placebo | grader note |
```

Close every pane, then remove the scratch folder and the pane folders.

## Reply

Follow the loaded session reply rules. Give the verdict, each case's family and result, and
each failed expectation with its quote. Use `<kit root>/references/explain-formats.md` when a
table helps compare the case results.
