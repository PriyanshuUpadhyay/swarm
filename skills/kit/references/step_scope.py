"""The write scope of a step: `take` records the work tree, and `done` refuses a change outside every active Writes.

A step file from a plan with a Writes column has a `Writes:` line under `Uses:` (see step_run.start). A step file
with no such line, or a run outside git, has no scope check.
"""

import os
import subprocess
import tempfile
from pathlib import Path, PurePosixPath

from step_checks import work_root


def header(text, key):
    """The value of a `key: value` line in the step file's head (the lines before the first blank line), or None."""
    for line in text.split("\n\n", 1)[0].splitlines()[2:]:
        if line.startswith(f"{key}: "):
            return line.removeprefix(f"{key}: ").strip()
    return None


def writes(text):
    """The step's Writes paths ([] for `none`), or None when the step file has no Writes line."""
    value = header(text, "Writes")
    if value is None:
        return None
    paths = [p.strip() for p in value.split(",") if p.strip()]
    return [] if paths == ["none"] else paths


def git(root, *args, env=None):
    return subprocess.run(["git", "-C", str(root), *args], capture_output=True, text=True, encoding="utf-8",
                          errors="replace", env=env)


def work_tree(folder):
    """(root, tree id) of the work tree as it is now, untracked files included, or None outside git.

    A copy of the index takes the files, so the real index and the stash stay as they are, and the copy keeps
    the index's stat cache, so an unchanged file is not read again.
    """
    root, _ = work_root(folder)
    env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
    if git(root, "rev-parse", "--show-toplevel", env=env).returncode:
        return None
    index = Path(git(root, "rev-parse", "--path-format=absolute", "--git-path", "index", env=env).stdout.strip())
    with tempfile.TemporaryDirectory() as tmp:
        copy = Path(tmp, "index")
        if index.is_file():
            copy.write_bytes(index.read_bytes())
        env["GIT_INDEX_FILE"] = str(copy)
        if git(root, "add", "-A", env=env).returncode:
            return None
        tree = git(root, "write-tree", env=env)
    return (root, tree.stdout.strip()) if tree.returncode == 0 else None


def base(folder, step):
    """The tree that the step's newest take recorded, or None."""
    events = Path(folder) / "events.log"
    lines = events.read_text(encoding="utf-8").splitlines() if events.exists() else []
    rows = [line.split("\t") for line in lines]
    found = None
    for row in rows:
        if len(row) > 2 and row[1] == step:
            if row[2].startswith("take "):
                found = None
            elif row[2].startswith("base "):
                found = row[2].removeprefix("base ")
    return found


def inside(path, scope):
    """True when the path is the scope path or under that folder, compared by path parts."""
    path, scope = PurePosixPath(path).parts, PurePosixPath(scope).parts
    return path[:len(scope)] == scope


def check(folder, step, text):
    """SystemExit when the step changed a path outside its own Writes, the Writes of every other active step, and
    the run folder; a printed `not run: scope:` line when the check cannot run."""
    own = writes(text)
    if own is None:
        return
    start = base(folder, step)
    now = work_tree(folder)
    if start is None or now is None:
        print(f"not run: scope: {'no git work tree' if now is None else 'no base recorded at take'}")
        return
    root, tree = now
    allowed = list(own)
    for other in sorted(Path(folder).glob("[0-9][0-9]-*.md")):
        if other.stem != step:
            body = other.read_text(encoding="utf-8")
            if body.startswith("Status: active "):
                allowed += writes(body) or []
    run = Path(folder).resolve()
    if run.is_relative_to(root.resolve()):
        allowed.append(run.relative_to(root.resolve()).as_posix())
    changed = git(root, "diff", "--name-only", "--no-renames", start, tree).stdout.splitlines()
    if outside := next((p for p in changed if not any(inside(p, s) for s in allowed)), None):
        raise SystemExit(f"{step} is not done: it changed {outside}, outside every active step's Writes "
                         f"({', '.join(own) or 'none'} for {step}); undo it, or ask the owner to change the plan")
