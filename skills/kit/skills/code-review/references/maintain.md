# Maintaining check mode

Read this file only to add or change a rule, a lens, or a repo rules file, or to work on the
review scripts. A review run does not need it.

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

## Rules for each repo

`~/.review-check/rules/<repo>.md` holds what the team of that repo cares about, from its own past
reviews. `<repo>` is the folder name above the git common dir. It stays out of this public skill,
because it names internal code. This preset owns the format and the upkeep of those files. A rule
that the repo's own docs already state points to that doc and is not copied. Add a rule when a
review raises the same point on a third PR.

A rules file may have one line ``CI: `<check name>` runs `<command>`; config `<lint config>` ``.
`<check name>` is the GitHub check that `build` reads, `<command>` is what `build --run` runs from
the repo root, and the JSON lint config decides which `Check:` IDs the build owns. The command
must not write files, so check that its type checker sets `noEmit`.

## Script limits

Known limits, accepted by the user on 2026-10-09 (review run range-01cdecf-f715957-01): the
script assumes UTF-8 source and a UTF-8 host. `parse_diff`, `start`, and `verdict` still split
source on form feed and U+0085, the ctags and CI subprocess output decode by locale, a local run
reads a changed file as strict UTF-8, a non-ASCII file name needs `core.quotepath=false`, and a
closed pipe on stdout is not caught. Answers carry only from the deepest ancestor run that has an
`answers.md`.
