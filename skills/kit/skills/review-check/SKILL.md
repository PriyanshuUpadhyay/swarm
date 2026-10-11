---
name: review-check
description: Judge a commit range or uncommitted changes with a script-enforced verdict. A script splits the diff into units (changed functions, hunks, removed code), a checklist gives each unit every rule whose file glob and regex match it plus a reference check, seats answer each rule with a quoted row, and the script gives APPROVE, REQUEST CHANGES, or NEEDS DISCUSSION only when every rule of every unit has a result. Use when flow, deliver, or review-walk needs a review verdict, or for "check this range". Not for a GitHub PR walk with comments in your voice, which review-walk owns.
---

# review-check

Example. deliver finishes on `feat-x` and runs `review-check a1b2c3d..HEAD`. The script finds 14
units in 6 files. For `index.ts` `hunk 2365-2380` it lists 23 checks: the lens rules whose `Files:`
glob and `Applies:` regex match the hunk, among them `C-7` (magic number), the TypeScript and
Cloudflare rules that match, the repo's `T-` rules, and `REF`, with the 3 places that call the
changed method. `start` prints the seats and their check counts. `verdict` prints
`INCOMPLETE (14 of 14 units, 301 of 305 checks ...)` and names the four missing IDs. The seats
answer them, and the result is `REQUEST CHANGES`, because `C-7` found `bytes / 32000` next to a
helper that already does that math.

## Contract

- The review is report-only. Never edit, commit, stash, switch, or reset the user's checkout.
- Only `verdict` writes the verdict. A reviewer never writes `03-verdict.md` or states a verdict that
  the script did not print.
- Every unit gets at least one row, and every rule ID in its checklist gets a result. A missing
  lens, rules file, or ctags language never skips a unit.
- The session that wrote the change never fills the checklist. In `flow`, `deliver`, and `pair`,
  the rows come from new seats.
- Judge the change on its own merits. Messy code around it does not excuse a new defect.
- No style nits that a linter or formatter owns.
- Seats never judge what the build gate owns, which is compile errors such as a missing import and
  the IDs in a unit's `tool` list. Missing or stale build evidence gives INCOMPLETE, and those IDs
  never go back to a seat.

## Run

```sh
C=<skill-dir>/scripts/review_check.py
python3 $C start <base>..<head>     # or `local` for uncommitted changes; --patch FILE for a part
python3 $C build <run>              # when 01-units.md has a Build line: the CI check for the head
python3 $C build <run> --run        # or run the CI command in the frozen verify worktree
python3 $C verdict <run>            # exits 1 with the gaps, or writes 03-verdict.md
```

Use `build <run>` for a pushed head. Use `--run` for an unpushed head or `local`. A range build
uses a persistent detached worktree at `<repo>/tmp/review-check/verify`, checked out at the
reviewed head. The live checkout can move while the build runs. `build --run` resets and cleans
the scratch verify worktree before checkout; ignored files stay. A `local` build uses the live tree and prints that limit.
If CI needs untracked files (`node_modules`, `.env`, generated code), use `build <run>` or install them in `tmp/review-check/verify` once; only ignored files stay between builds.
Build may run while seats work, because seats never judge build-gate IDs.
`build --run --force` is refused; wait for the running build to release its lock.
A CI descendant that starts its own session escapes the group kill and must be stopped by hand.

Run it from the repo root. The run folder is `<repo-root>/tmp/review-check/<run>/`, where `<run>`
is `range-<base7>-<head7>-NN` or `local-NN`, as `references/run-folder.md` describes.

| File | Who | Holds |
|---|---|---|
| `01-units.md` | script | target, rules file or `none`, and every unit with its file, symbol, range, and count of checks |
| `checklist.json` | script | unit entries hold `rules`, build-gate IDs under `tool`, references under `refs`, and one owner seat for each rule under `owners`; `seats` maps each seat name to its unit IDs, check count, and route |
| `target.json` | script | base, head, and the repo's `CI:` line |
| `build.json` | script | the CI result for the head, or the `--run` result for the tree |
| `02-review-<seat>.md` | seat | results for the unit-rule pairs it owns, checked by `verdict <run>` |
| `answers.md` | chair or user | answers for this run, with a file and a whole trimmed source line |
| `answers-before.md` | script | answers from a prior run, for history only |
| `03-verdict.md` | script | line 3 is `Verdict: <word> (<n> of <m> units, <c> of <t> checks; <fix> fix, <ask> ask, <note> note, <answered> answered, <limit> limit; rules: <file>; build: pass\|fail\|none)`, then each gap or kept row |

`head/` holds the new version of each changed file. Read full functions there, not in a checkout
that can move.

## Review

For each unit, read its full range in `head/`, the diff hunks, and every place that
`checklist.json` lists under `refs`. Then check the unit against each rule ID in its checklist.
The rules live in `references/lens.md`, `references/lens-<name>.md`, and the repo's rules file that
`01-units.md` names. Also read `minimize-reader-load`, `engineering-standards`, the repo's own
convention docs, and the `review` paths of each domain in `~/.flow/domains.md` whose `Signals:`
line matches the file. A defect that no rule names is still a finding, with rule `-`.

`REF` means the change can break a place that names its symbol. Check each listed place against
the new signature, return shape, and meaning. For a removed symbol, each listed place is a
leftover reference.

Write each row as `| unit | rule | file:line | quote | kind | problem | proof |`.

- `rule` is one ID, a comma list of IDs for a `pass` or an `n/a` row, or `-`.
- `kind` is `pass` (the unit follows the rule), `n/a` (the rule cannot apply, for example "no SQL
  in this hunk"), `fix` (a proved defect or a rule break), `ask` (a question, or a defect that is
  not proved), or `note` (optional cleanup). An unproved defect is an `ask`, never a `fix`. Start
  its problem with "potential issue, not confirmed:" and name the input that would trigger it.
  The script sets `answered` when an answer closes an `ask`, or `limit` when the user accepts a
  `fix` as a known limit; a seat never writes those kinds.
- `quote` is an exact part of one line. A `pass` or `n/a` quotes a line of its unit. Other kinds
  quote the new line that `file:line` names, or a line the diff removed from that file.
- `proof` names the input, caller, or rule text that triggers the problem. For a `pass`, it names
  the case that was checked, for example "empty list returns before the loop". For an `n/a`, it
  says why the rule cannot apply. A bare "ok" fails the gate in every kind of row.
- Escape a `|` inside a cell as `\|`.

`verdict` owns the verdict word and prints it.

## Asks

An `ask` is answered. Never change code for an `ask` inside a review loop. Write one answer per
line in `<run>/answers.md` with this shape:

```text
- <file> `<whole trimmed source line>`: <answer>
```

The user closes an `ask`. The chair may close one only by citing a record the user approved,
such as the frame's done-when, contracts, an ADR, or an earlier user answer.
Anyone may turn an `ask` into a `fix` with a failing test. Write the answer as
`fix: <test> fails at <head7>`.

The user can accept a `fix` as a known limit. Write this line only with the user's words:

```text
- <file> `<line>`: limit: <the user's words>
```

`<line>` is the whole trimmed source line, as in an ask's answer. A new `start` of the same range
does not replace the answer that an open ask needs.

After answering, run `verdict <run>` again on the same run, with no new range.
`start` refuses a new range, or `local`, when the newest run for any ancestor head has no verdict,
an INCOMPLETE verdict, or 0 fix and an open ask (same head too for `local`).
Answers carry from the deepest ancestor run, newest first on a tie.
`--patch` runs never block `start` or carry answers; runs whose heads are no longer ancestors after
a rebase or amend are skipped.
`answers-before.md` is history only; copy an answer into `answers.md` to use it in this run.

Known limits, accepted by the user on 2026-10-09 (review run range-01cdecf-f715957-01): the
script assumes UTF-8 source and a UTF-8 host. `parse_diff`, `start`, and `verdict` still split
source on form feed and U+0085, the ctags and CI subprocess output decode by locale, a local run
reads a changed file as strict UTF-8, a non-ASCII file name needs `core.quotepath=false`, and a
closed pipe on stdout is not caught. Answers carry only from the deepest ancestor run that has an
`answers.md`.

## Rule files

Each rule is one line: ``- `ID` flag → ask. Files: `<glob>, <glob>`. Applies: `<regex>`. Source: …``.
`Files:` is optional and falls back to the file's own `Files:` header line. `Applies:` is one
Python regex on the unit's code, new and removed; with none, the rule applies to every unit of a
matching file. A regex must never skip a unit that can break the rule, so leave it out when in
doubt. `Scope: added` matches `Applies:` against the unit's added lines only. Use it only when the
token on the added line is the defect itself, never for a rule about callers, aliases, or data
flow. Add a `lens-<name>.md` for a new language or platform in the same shape.

`Check: <tool>:<rule>` names the lint rule that proves the whole rule text. The build gate owns
the ID for a unit only when the repo's `CI:` line names a lint config that runs that rule at
error level, with no options, on the unit's file, and not as a type-aware rule on a JS file. Else
the seat owns it. A rule that a tool covers only in part splits into two IDs, one with `Check:`
and one for the seat, as TS-1 and TS-33 do. Every `Check:` needs `fixtures/<tool>/<rule>/` with
`bad`, `good`, and `exception` files, where each `// expect` line of `bad` is a hit and the other
files have none. `scripts/test_review_check.py` runs them with the oxlint on PATH, so run it with
`PATH=<repo>/node_modules/.bin:$PATH` to test the repo's own version before you add a `Check:`.

## Seats

Use every seat in `checklist.json` on its listed route, as
[orchestration.json](orchestration.json) declares. Each seat answers only the unit-rule pairs
whose `owners` entry names it, never an ID under `tool`. A `pass` or `n/a` row may list many IDs
with one quote and one proof, so write one row for each group of IDs.

Dispatch and gap follow-ups follow [references/fan-out.md](../../references/fan-out.md).

`start` splits each `review.check` aspect by assigned unit-rule pairs, with one shard per
150 checks rounded up, at most three. Units stay whole and in order. The split balances check
counts. One shard keeps the aspect name; more shards use `<aspect>-1`, `<aspect>-2`, and
`<aspect>-3`. Each seat writes `02-review-<seat>.md`.

| Seat | Route | IDs | Writes |
|---|---|---|---|
| lens | `review.check` | `C-`, `L-` | `02-review-lens.md` |
| lang | `review.check` | the IDs from `lens-<name>.md` | `02-review-lang.md` |
| repo | `review.check` | the IDs from the repo's rules file | `02-review-repo.md` |
| refs | `review.deep` | `REF`, and any finding it meets | `02-review-refs.md` |

REF stays on `review.deep`, because it judges a changed signature, return shape, or meaning at
each caller, and it must search past the first 20 references on its own.

`verdict` reads each named seat file in the map and checks every result against its owner.
It lists missing results together for each seat. One review session writes the named seat files.
The chair uses `02-review.md` for findings without an owner; `verdict <run>` checks ownership.
Older runs without a seat map still use `02-review*.md`.

## Rules for each repo

`~/.review-check/rules/<repo>.md` holds what the team of that repo cares about, from its own past
reviews. `<repo>` is the folder name above the git common dir. It stays out of this public skill,
because it names internal code. This skill owns the format and the upkeep of those files. A rule
that the repo's own docs already state points to that doc and is not copied. Add a rule when a
review raises the same point on a third PR.

A rules file may have one line ``CI: `<check name>` runs `<command>`; config `<lint config>` ``.
`<check name>` is the GitHub check that `build` reads, `<command>` is what `build --run` runs from
the repo root, and the JSON lint config decides which `Check:` IDs the build owns. The command
must not write files, so check that its type checker sets `noEmit`.

## Callers

`flow` step 06, `deliver`, and `review-walk` step 03 read line 3 of `03-verdict.md`. A caller treats
`INCOMPLETE`, or a verdict file older than its range, as no review.

## Output

In chat, give line 3 of `03-verdict.md`, then each `fix`, `ask`, `note`, and `limit` row with its
`file:line`, then the run folder.
