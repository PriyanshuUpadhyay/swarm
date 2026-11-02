#!/usr/bin/env python3
"""Say whether a built change still fits the small route, from the real diff.

  size_gate.py <base>    run in the repo after the build; <base> is the commit before the change

It counts the work tree against <base>, so commits, edits, and new files that git does not ignore all count.
It prints `small: <n> files, <m> lines` and exits 0, or `raise: <reason>` and exits 1. A raise sends the
change to the full implement route. Lines in test files do not count toward the line limit, because good
tests for a 40-line change can be 130 lines (trial 6: two such changes lost the small route); they still
count as files. The limits are fixed here, because a limit that a prompt or a repo file
can widen does not hold (trial 5, kit-right-size council 2026-11-02).
"""

import fnmatch
import re
import subprocess
import sys
from pathlib import Path

MAX_FILES = 3
MAX_LINES = 100
# Test lines do not count toward MAX_LINES, but a huge fixture still leaves the small route (owner, round 5).
MAX_TEST_LINES = 400
# A change to stored data or a wire format needs the contracts step, whatever its size.
SCHEMA = ("*.sql", "*migration*", "*schema*", "*.proto", "*.graphql")
TESTS = ("test_*", "*_test.*", "*.test.*", "*.spec.*", "*_spec.*", "*Test.*", "*Tests.*")
TEST_DIRS = {"test", "tests", "__tests__", "testdata"}  # not spec: many repos keep API specs there


def is_test(path):
    parts = path.split("/")
    return bool(TEST_DIRS & {p.lower() for p in parts[:-1]}) or any(fnmatch.fnmatchcase(parts[-1], g) for g in TESTS)


def git(*args):
    out = subprocess.run(["git", *args], capture_output=True, text=True, encoding="utf-8", errors="replace")
    if out.returncode:
        raise SystemExit(f"git {' '.join(args)} failed: {out.stderr.strip()}")
    return out.stdout


def lines_of(path):
    """Lines in an untracked file, read in chunks. Like git, a file with a NUL byte in its first 8000 bytes is
    binary and counts 0, and a symlink counts 1 line, its target."""
    path = Path(path)
    if path.is_symlink():
        return 1
    count, last = 0, b"\n"
    with path.open("rb") as file:
        head = file.read(8000)
        if b"\0" in head:
            return 0
        chunk = head
        while chunk:
            count, last = count + chunk.count(b"\n"), chunk[-1:]
            chunk = file.read(1 << 20)
    return count + (last != b"\n")


def measure(base):
    """(paths, changed non-test lines, test lines) of the work tree against base; a binary file counts as 0.

    SystemExit when base is not a commit; `raise:` text in the second slot when a new file cannot be read."""
    if base.startswith("-"):
        raise SystemExit(f"{base!r} is not a commit; a base never starts with -")
    base = git("rev-parse", "--verify", "--quiet", f"{base}^{{commit}}").strip()
    changed = {}
    # -z keeps a path with non-ASCII bytes unquoted, so the schema globs see its real name.
    for entry in git("diff", "--numstat", "-z", "--no-renames", base, "--").split("\0"):
        if entry:
            added, removed, path = entry.split("\t", 2)
            changed[path] = (int(added) if added.isdigit() else 0) + (int(removed) if removed.isdigit() else 0)
    for path in git("ls-files", "--others", "--exclude-standard", "-z").split("\0"):
        # A test run leaves bytecode caches; in a repo with no .gitignore they are not part of the change (trial 6).
        if path and not ("__pycache__" in path.split("/")[:-1] or path.endswith((".pyc", ".pyo"))):
            try:
                changed[path] = lines_of(path)
            except OSError as error:
                return sorted(changed), f"{path} cannot be read ({error.strerror}), so its size is unknown", 0
    tests = sum(n for p, n in changed.items() if is_test(p))
    return sorted(changed), sum(changed.values()) - tests, tests


def main(argv):
    if len(argv) != 1:
        raise SystemExit("usage: size_gate.py <base>")
    paths, lines, tests = measure(argv[0])
    schema = next((p for p in paths if any(fnmatch.fnmatch(p.lower(), g) for g in SCHEMA)), None)
    if isinstance(lines, str):
        reason = lines
    elif schema:
        reason = f"{schema} is a schema or migration path"
    elif len(paths) > MAX_FILES:
        reason = f"{len(paths)} files changed (small allows {MAX_FILES})"
    elif lines > MAX_LINES:
        reason = f"{lines} non-test lines changed (small allows {MAX_LINES})"
    elif tests > MAX_TEST_LINES:
        reason = f"{tests} test lines changed (small allows {MAX_TEST_LINES})"
    else:
        print(f"small: {len(paths)} files, {lines} lines" + (f" (+{tests} test lines)" if tests else ""))
        return 0
    # -z gives raw names, so a control byte in a path is shown escaped, not sent to the terminal.
    print("raise: " + re.sub(r"[\x00-\x1f\x7f-\x9f]", lambda m: f"\\x{ord(m[0]):02x}", reason))
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
