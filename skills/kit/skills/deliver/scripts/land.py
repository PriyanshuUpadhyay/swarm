#!/usr/bin/env python3
"""Check landing order without changing Git state.

Run: python3 land.py <decl.json> <worktree>... [--base <rev>]
Declaration: {"worktrees": {"/absolute/worktree": ["relative/file"]},
              "frozen_interfaces": ["relative/file"]}
Without --base, use each worktree's merge-base with the main checkout's HEAD.
"""
import argparse
import json
import os
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3] / "references"))
from step_run import plain  # noqa: E402


def git(worktree, *args):
    result = subprocess.run(["git", "-C", str(worktree), *args],
                            capture_output=True)
    if result.returncode:
        error = result.stderr.decode("utf-8", errors="backslashreplace").strip()
        raise ValueError(plain(f"Git failed in {worktree}: {error}"))
    return result.stdout.decode("utf-8")


def file_list(value, label):
    if not isinstance(value, list) or any(not isinstance(p, str) or not p for p in value):
        raise ValueError(f"{label} must be a list of file paths")
    return set(value)


def land(declaration, worktrees, base=None):
    payload = json.loads(Path(declaration).read_text(encoding="utf-8"))
    if not isinstance(payload, dict) or not isinstance(payload.get("worktrees"), dict):
        raise ValueError("declaration must map worktree paths to declared files in worktrees")
    declared = {Path(w).resolve(): file_list(files, w)
                for w, files in payload["worktrees"].items()}
    frozen = file_list(payload.get("frozen_interfaces"), "frozen_interfaces")
    worktrees = [Path(w).resolve() for w in worktrees]
    for worktree in declared:
        if worktree not in worktrees:
            raise ValueError(f"{worktree} is declared but missing from the command line")
    main_head = None
    if base is None:
        first = git(worktrees[0], "worktree", "list", "--porcelain", "-z").split("\0", 1)[0]
        main = first.removeprefix("worktree ")
        main_head = git(main, "rev-parse", "HEAD").strip()
    owners, order = {}, []
    for worktree in worktrees:
        if worktree not in declared:
            raise ValueError(f"{worktree} has no declared files")
        start = (git(worktree, "rev-parse", "--verify", "--end-of-options", f"{base}^{{commit}}")
                 if base is not None else git(worktree, "merge-base", "HEAD", main_head)).strip()
        changed = git(worktree, "diff", "--name-only", "--no-renames", "-z", start, "--")
        for file in changed.split("\0"):
            if not file:
                continue
            if file not in declared[worktree]:
                raise ValueError(f"{worktree} changed {file!r} outside its declared files")
            if file in frozen:
                raise ValueError(f"{worktree} changed frozen interface {file!r}")
            if file in owners:
                raise ValueError(f"{file!r} changed by both {owners[file]} and {worktree}")
            owners[file] = worktree
        commits = git(worktree, "log", "--oneline", f"{start}..HEAD", "--").rstrip("\n")
        order.append((worktree, commits))
    for index, (worktree, commits) in enumerate(order, 1):
        print(plain(f"{index}. {worktree}"))
        if commits:
            for line in commits.split("\n"):
                print(plain(line))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("declaration")
    parser.add_argument("worktrees", nargs="+")
    parser.add_argument("--base")
    args = parser.parse_args()
    try:
        land(args.declaration, args.worktrees, args.base)
    except BrokenPipeError:
        raise
    except (OSError, ValueError, UnicodeError) as error:
        print(plain(str(error)), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(errors="backslashreplace")
    try:
        code = main()
        sys.stdout.flush()
    except BrokenPipeError:
        fd = os.open(os.devnull, os.O_WRONLY)
        os.dup2(fd, sys.stdout.fileno())
        os.close(fd)
        code = 0
    sys.exit(code)
