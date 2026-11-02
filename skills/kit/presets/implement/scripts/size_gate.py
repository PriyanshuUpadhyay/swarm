#!/usr/bin/env python3
"""Say whether a built change still fits the small route, from the real diff.

  size_gate.py <base>    run in the repo after the build; <base> is the commit before the change

It counts the work tree against <base>, so commits, edits, and new files that git does not ignore all count.
It prints `small: <n> files, <m> lines` and exits 0, or `raise: <reason>` and exits 1. A raise sends the
change to the full implement route. The limits are fixed here, because a limit that a prompt or a repo file
can widen does not hold (trial 5, kit-right-size council 2026-11-02).
"""

import fnmatch
import subprocess
import sys

MAX_FILES = 3
MAX_LINES = 100
# A change to stored data or a wire format needs the contracts step, whatever its size.
SCHEMA = ("*.sql", "*migration*", "*schema*", "*.proto", "*.graphql")


def git(*args):
    out = subprocess.run(["git", *args], capture_output=True, text=True, encoding="utf-8", errors="replace")
    if out.returncode:
        raise SystemExit(f"git {' '.join(args)} failed: {out.stderr.strip()}")
    return out.stdout


def measure(base):
    """(paths, changed lines) of the work tree against base; a binary file counts as 0 lines."""
    changed = {}
    for line in git("diff", "--numstat", "--no-renames", base, "--").splitlines():
        added, removed, path = line.split("\t", 2)
        changed[path] = (int(added) if added.isdigit() else 0) + (int(removed) if removed.isdigit() else 0)
    for path in git("ls-files", "--others", "--exclude-standard", "-z").split("\0"):
        if path:
            try:
                with open(path, encoding="utf-8", errors="replace") as file:
                    changed[path] = sum(1 for _ in file)
            except OSError:
                changed[path] = 0
    return sorted(changed), sum(changed.values())


def main(argv):
    if len(argv) != 1:
        raise SystemExit("usage: size_gate.py <base>")
    paths, lines = measure(argv[0])
    schema = next((p for p in paths if any(fnmatch.fnmatch(p.lower(), g) for g in SCHEMA)), None)
    if schema:
        reason = f"{schema} is a schema or migration path"
    elif len(paths) > MAX_FILES:
        reason = f"{len(paths)} files changed (small allows {MAX_FILES})"
    elif lines > MAX_LINES:
        reason = f"{lines} lines changed (small allows {MAX_LINES})"
    else:
        print(f"small: {len(paths)} files, {lines} lines")
        return 0
    print(f"raise: {reason}")
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
