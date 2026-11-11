# review mode: check

The `code-review` skill owns the mode choice. This file owns the review gate, its script, and its verdict.

Example. An `auto` build finishes on `feat-x` and runs a check of `a1b2c3d..HEAD`. The script finds 14
units in 6 files. For `index.ts` `hunk 2365-2380` it lists 23 checks: the lens rules whose `Files:`
glob and `Applies:` regex match the hunk, among them `C-7` (magic number), the TypeScript and
Cloudflare rules that match, the repo's `T-` rules, and `REF`, with the 3 places that call the
changed method. `start` prints the seats and their check counts. `verdict` prints
`INCOMPLETE (14 of 14 units, 301 of 305 checks ...)` and names the four missing IDs. The seats
answer them, and the result is `REQUEST CHANGES`, because `C-7` found `bytes / 32000` next to a
helper that already does that math.

## Preset

Stop: owner
Offers: prove-it-works, call-tree, headless-shot, lean-review

| File | Needs | Holds | Block | Why |
|---|---|---|---|---|
| 01-units | none | the run folder that `review_check.py start` printed, and its seat map | worker-brief | each seat needs a brief from the seat map |
| 02-seats | units | each seat's `02-review-<seat>.md` | run-workers, when-to-extract | new seats answer every unit-rule pair |
| 03-verdict | seats | line 3 of `03-verdict.md`, then each kept row | voice-reply | the owner reads the script's verdict |

## Contract

For a gate to new seats and the `implement` handoff from a leaf, apply `blocks/worker-brief/SKILL.md`; this preset names no default that replaces fresh seats.


- The review is report-only. Never edit, commit, stash, switch, or reset the user's checkout.
- Only `verdict` writes the verdict. A reviewer never writes `03-verdict.md` or states a verdict that
  the script did not print.
- Every unit gets at least one row, and every rule ID in its checklist gets a result. A missing
  lens, rules file, or ctags language never skips a unit.
- The session that wrote the change never fills the checklist. In `implement`, in both modes,
  the rows come from new seats.
- Judge the change on its own merits. Messy code around it does not excuse a new defect.
- No style nits that a linter or formatter owns.
- Seats never judge what the build gate owns, which is compile errors such as a missing import and
  the IDs in a unit's `tool` list. Missing or stale build evidence gives INCOMPLETE, and those IDs
  never go back to a seat.

## Run

```sh
C=<kit root>/skills/code-review/scripts/review_check.py
python3 $C start <base>..<head> --parent-loop <task-key> # or `local`; --patch FILE for a part
python3 $C start <base>..<head> --parent-loop <task-key> --continue <user-file>
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
is `range-<base7>-<head7>-NN` or `local-NN`, as the kit's `references/run-folder.md` describes. The kit
root is two folders up from the `code-review` skill's folder; a bare `references/` path below is that skill's own.

| File | Who | Holds |
|---|---|---|
| `01-units.md` | script | target, rules file or `none`, and every unit with its file, symbol, range, and count of checks |
| `checklist.json` | script | unit entries hold `rules`, build-gate IDs under `tool`, references under `refs`, and one owner seat for each rule under `owners`; `seats` maps each seat name to its unit IDs, check count, and route |
| `target.json` | script | base, head, the repo's `CI:` line, parent-loop key, round, and consumed continuation |
| `snapshot.json` | script | open issue identities and check results from `verdict`, for the loop guard in `review_check.py start` |
| `<git-common-dir>/review-check/<key-hash>.json` | script | shared round ledger for `review_check.py start` across linked worktrees |
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
`rules.md` in the run folder holds the text of each rule ID in the checklist, so read it rather
than the lens files, whose `Source:` citations are for rule authors. A rule that cites a kit block or a standards row reads that target. The `repo`
seat also reads the repo's own convention docs and the `review` paths of each domain in
`~/.flow/domains.md` whose `Signals:` line matches the file. A defect that no rule names is still a finding, with rule `-`.

When `~/.review-walk/taste.md` exists, `start` loads its `TASTE-` rules with the repo rules, so
each matching unit's checklist holds them. They say what the owner flags. A copy that the change
adds of a type, schema, or helper that already exists is a `fix`, because its author would fix it.

`REF` means the change can break a place that names its symbol. Check each listed place against
the new signature, return shape, and meaning. For a removed symbol, each listed place is a
leftover reference.

Write each row as `| unit | rule | file:line | quote | kind | problem | proof |`.

- `rule` is one ID, a comma list of IDs for a `pass` or an `n/a` row, or `-`.
- `kind` is `pass` (the unit follows the rule), `n/a` (the rule cannot apply, for example "no SQL
  in this hunk"), `fix` (a proved defect or a rule break), `ask` (a question, or a defect that is
  not proved), or `note` (optional cleanup). An unproved defect is an `ask`, never a `fix`; start
  its problem with "potential issue, not confirmed:" and name the input that would trigger it. A test
  that a run shows passes with and without its change is proved, so it is a `fix` (`L-14`), never an `ask`. A `fix` also needs a defect that this change
  introduced and that its author would fix; an older defect or a matter of taste is a `note`, unless
  the change says it fixes or bounds that defect (`L-15`). A `TASTE-` row that the change breaks is
  the owner's rule, not a matter of taste, so it is a `fix` that asks for the remedy the row names.
  A `note` that names a crash on a line the change added counts as a `fix`; `verdict` changes its
  kind, and only an owner's `limit:` answer closes it. When the change did not cause the crash,
  open the problem with `older defect` or `not from this change`.
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
Anyone may turn an `ask` into a `fix` with a failing test, when the ask is a behavior bug that a
real caller can hit, or a guard on auth, data loss, a trust boundary, or the owner gate. Write the
answer as `fix: <test> fails at <head7>`. For any other ask, follow the test rule in the kit's
`blocks/prove-it-works/SKILL.md`: fix the code without a new test, or leave a note. A seat writes a
`note` for a test that breaks that rule, and does not ask for more edge-case tests.

The user can accept a `fix` as a known limit. Write this line only with the user's words:

```text
- <file> `<line>`: limit: <the user's words>
```

`<line>` is the whole trimmed source line, as in an ask's answer. A new `start` of the same range
does not replace the answer that an open ask needs.

After answering, run `verdict <run>` again on the same run, with no new range. `start` refuses a
new range while an ancestor run still waits for a verdict or an answer, and prints why. Answers
carry from the deepest ancestor run. `answers-before.md` is history only; copy an answer into
`answers.md` to use it in this run.

To add or change a rule, a lens, or a repo rules file, read `references/maintain.md`.

## Seats

Use every seat in `checklist.json` on its listed route, as
[orchestration-check.json](../orchestration-check.json) declares. Each seat answers only the unit-rule pairs
whose `owners` entry names it, never an ID under `tool`. A `pass` or `n/a` row may list many IDs
with one quote and one proof, so write one row for each group of IDs.

Dispatch and gap follow-ups follow [the kit's fan-out.md](../../../references/fan-out.md).

`start` prints the seats. One shard keeps the aspect name; more shards use `<aspect>-N`. Each
seat writes `02-review-<seat>.md`. Before it reports, the seat runs
`python3 <kit root>/skills/code-review/scripts/review_check.py lint <run> 02-review-<seat>.md`, which checks
only that file's rows (cells, IDs, quotes, and proofs), and fixes each row that it names.

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

## Refute findings

Example. The `lang` seat writes a `fix` for `util.py:2` `return 2`, because a caller expects 1. A
new `refute` seat on the other provider gets only that row, the diff, and `head/`. It finds no
caller that reads the value, so it writes `| util.py:2 | \`return 2\` | refuted | rg finds no use of
helper |`. `verdict` then shows the row as `ask` with `refuted:` in front of its problem, and the
owner decides.

When the first `verdict` shows a `fix` row, open one new seat on `review.deep`, on a provider
that wrote no seat file. Give it each `fix` row with its unit, and no other row. It tries to
disprove each one from the code, and writes one row for each in `02-refute.md`:
`| file:line | quote | kept or refuted | proof |`. The file and quote are copied from the `fix` row. A
`refuted` proof names the input, caller, or line that shows the row is wrong. Then run `verdict`
again. A refuted `fix` becomes an `ask`, never a pass, so only the owner can close it.

## Callers

`implement` step 06 (both modes), the `change-review` block, and walk mode step 03 read line 3 of `03-verdict.md`. A caller treats
`INCOMPLETE`, or a verdict file older than its range, as no review.

## Output

In chat, give line 3 of `03-verdict.md`, then each `fix`, `ask`, `note`, and `limit` row with its
`file:line`, then one line for each changed file that says what was checked and how, such as a
test run or a probe input, then the run folder.
