#!/usr/bin/env python3
"""Deterministic parts of a review-check run. The script owns the units and the verdict; the reviewer
only writes rows into 02-review.md.

Every rule in references/lens*.md and in the repo's rules file has an ID, a `Files:` glob, and an
optional `Applies:` regex. `start` writes checklist.json: for each unit, the ID of every rule whose
glob matches the unit's file and whose regex matches the unit's code, plus `REF` when a symbol the
unit defines or removes has references elsewhere. `verdict` refuses until each ID has a result.

A rule with `Check: <tool>:<rule>` is owned by the build gate, not a seat, when the repo rules file
has a `CI:` line and the repo's lint config runs that rule at error level, with default options, on
the unit's file. `build` records the CI result for the run's head, and `verdict` counts those IDs
only from it.

  start <local|base..head> [--parent-loop KEY] [--continue FILE] [--patch FILE]
                                            make <repo>/tmp/review-check/<run>/ with 01-units.md
  build <run> [--run]                        record CI for the head, or run its command in the verify worktree
  verdict <run>                              gate the rows of 02-review*.md and write 03-verdict.md

`--patch` reviews that patch instead of the whole range, for example review-walk's diff since view.
`verdict` exits 1 when the gate fails, so a caller cannot read an APPROVE that no one earned.

`start` owns the loop stop. A parent-loop key has three rounds, across fixes and re-plans.
Its ledger and start lock live under git-common-dir/review-check, shared by linked worktrees.
When that folder is not writable, start still reads the ledger, but it counts and locks in the
worktree. Those read-only rounds stay in that worktree, so a later writable start elsewhere does not
see them; only a writable start in the same worktree merges them.
Without --parent-loop, the key is branch:<current branch>; detached HEAD needs an explicit key.
Two complete, certain snapshots with the same open issues and check results stop the next round.
Identity uses file, symbol, rule, and whole source evidence, without line numbers. A rule `-` adds
the trimmed problem text to its identity. Uncertain matches and broken snapshots
use the cap. One user line `continue: <key>` in --continue FILE permits one extra round, once.
A refusal preserves the verdict. An answer-only verdict spends no round, and APPROVE can close
on the last allowed round. --patch starts do not spend the parent-loop budget.
"""
import fcntl, fnmatch, hashlib, json, os, re, shutil, signal, subprocess, sys, tempfile, time, urllib.parse
from contextlib import contextmanager, nullcontext
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
sys.path.insert(0, str(Path(__file__).resolve().parents[3] / "references"))
import step_run  # noqa: E402
from review_check_start import start_lock, start, start_locked
from review_check_rules import expand, glob_match, globs_of, load_rules, ci_of, lint_config, lint_name, tool_owned, seat_map
from review_check_units import git, repo_name, home, family, references, revision, local_patch, parse_diff, functions, brace_end, units_of, tree_digest, stale_verdict
from review_check_build import verify_tree, build_signals, build
from review_check_verdict import review_files, rows, answers, verdict

KINDS = ("pass", "fix", "ask", "note", "n/a")
RULE = re.compile(r"^- `([A-Z][A-Z0-9-]*-\d+)` (.*)$")
REFS_CAP = 20  # a symbol with more references than this gets the first 20; the seat searches the rest
ROUND_CAP = 3
START_LOCK_WAIT = 10
BUILD_TIMEOUT = 3600
CLEANUP_SECONDS = 10
FUNC_KINDS = {"function", "method", "subroutine", "func", "procedure"}
PLAIN = ("--no-ext-diff", "--no-color", "--src-prefix=a/", "--dst-prefix=b/")  # user config may change these
GENERIC = {"", "n/a", "na", "none", "ok", "okay", "fine", "good", "looks fine", "looks good", "checked",
           "verified", "tested", "no issues", "lgtm", "-"}


# Names other files can reach: exported bindings, top-level functions and types, and class members with
# a modifier. A plain `const x` inside a function is local, so it gets no REF.
DEFINES = re.compile(r"(?:\bexport\s+(?:default\s+)?(?:async\s+)?(?:const|let|var|function|class|type|interface|enum)\s+"
                     r"|^(?:async\s+)?(?:def|function|func|class|struct|enum|protocol|interface|type)\s+"
                     r"|^\s+(?:def|func)\s+"
                     r"|^\s*(?:(?:public|private|protected|static|async|override|readonly)\s+)+)([A-Za-z_]\w{2,})")


FAMILY = {ext: fam for fam, exts in {
    "js": "ts tsx js jsx mjs cjs mts cts vue svelte", "py": "py pyi", "go": "go", "swift": "swift m h",
    "rs": "rs", "jvm": "java kt", "rb": "rb", "sql": "sql"}.items() for ext in exts.split()}


if __name__ == "__main__":
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(errors="backslashreplace")
    commands = {"start": start, "build": build, "verdict": verdict}
    if len(sys.argv) < 3 or sys.argv[1] not in commands:
        raise SystemExit(__doc__)
    sys.exit(commands[sys.argv[1]](*sys.argv[2:]))
