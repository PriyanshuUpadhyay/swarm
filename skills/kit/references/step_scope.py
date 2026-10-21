"""The write scope of a step: `take` records the work tree, and `done` refuses a change outside the Writes of every step beside it.

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


def write_paths(cell):
    """[] for `none`, else the comma-separated paths with their backticks removed. Plan cells and step files both
    come through here, so the Writes grammar has one owner."""
    paths = [p.strip().strip("`").strip() for p in cell.split(",")]
    paths = [p for p in paths if p]
    if "none" in paths and paths != ["none"]:
        raise SystemExit(f"Writes `none` stands alone, not beside paths: {cell}")
    return [] if paths == ["none"] else paths


def writes(text):
    """The step's Writes paths ([] for `none`), or None when the step file has no Writes line."""
    value = header(text, "Writes")
    return None if value is None else write_paths(value)


def git(root, *args, env=None):
    try:
        return subprocess.run(["git", "-C", str(root), *args], capture_output=True, text=True, encoding="utf-8",
                              errors="replace", env=env)
    except OSError as error:  # no git on PATH: the caller reads a failed run
        return subprocess.CompletedProcess(args, 127, "", str(error))


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
    """(tree, steps done since) for the step's newest take, or (None, set()) when that take recorded no tree."""
    events = Path(folder) / "events.log"
    lines = events.read_text(encoding="utf-8").splitlines() if events.exists() else []
    rows = [line.split("\t") for line in lines]
    found, since = None, set()
    for row in rows:
        if len(row) > 2 and row[1] == step:
            if row[2].startswith("take "):
                found, since = None, set()
            elif row[2].startswith("base "):
                found = row[2].removeprefix("base ")
        elif len(row) > 2 and row[2].startswith("done "):
            since.add(row[1])
    return found, since


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
    start, since = base(folder, step)
    now = work_tree(folder)
    if start is None or now is None:
        print(f"not run: scope: {'no git work tree' if now is None else 'no base recorded at take'}")
        return
    root, tree = now
    allowed = list(own)
    # A step that ran beside this one counts too, also when it was done first: its files cannot be told apart.
    for other in sorted(Path(folder).glob("[0-9][0-9]-*.md")):
        if other.stem != step:
            body = other.read_text(encoding="utf-8")
            if body.startswith("Status: active ") or other.stem in since:
                allowed += writes(body) or []
    run = Path(folder).resolve()
    if run.is_relative_to(root.resolve()):
        allowed.append(run.relative_to(root.resolve()).as_posix())
    diff = git(root, "diff", "-z", "--name-only", "--no-renames", start, tree)
    if diff.returncode:  # a pruned base tree, say; refuse, so a failed diff never passes the check
        raise SystemExit(f"{step} is not done: scope check failed: {diff.stderr.strip()}; take the step again")
    changed = [p for p in diff.stdout.split("\0") if p]
    if outside := next((p for p in changed if not any(inside(p, s) for s in allowed)), None):
        raise SystemExit(f"{step} is not done: it changed {outside}, outside the Writes of every step that ran beside it "
                         f"({', '.join(own) or 'none'} for {step}); undo it, or ask the owner to change the plan")


def blocks(folder, step, text):
    """The step's blocks, from its plan row, its `Skills read:` line, and implement's `Plan blocks:` line."""
    from step_checks import plan_blocks
    import re
    read = next((l for l in text.splitlines() if l.startswith("Skills read: ")), "")
    labels = header(text, "Plan blocks") or ""
    return (plan_blocks(folder, step) | set(re.findall(r"\bblock:([\w.-]+)@", read))
            | {b.strip() for b in labels.split(",") if b.strip()})


def earlier(folder, step):
    """Every step that the step reaches back to through the `Uses:` lines of the step files."""
    seen, todo = set(), [step]
    while todo:
        line = (Path(folder) / f"{todo.pop()}.md").read_text(encoding="utf-8").splitlines()[1]
        for name in (n.strip().split("@")[0] for n in line.removeprefix("Uses:").split(",") if n.strip()):
            if name not in seen:
                seen.add(name)
                todo.append(name)
    return seen


def check_reviewer(folder, step, who):
    """SystemExit when the step has change-review and `who` took a writing step that it covers."""
    text = (Path(folder) / f"{step}.md").read_text(encoding="utf-8")
    if "change-review" not in blocks(folder, step, text):
        return
    events = Path(folder) / "events.log"
    rows = [l.split("\t") for l in (events.read_text(encoding="utf-8").splitlines() if events.exists() else [])]
    for name in sorted(earlier(folder, step)):
        if writes((Path(folder) / f"{name}.md").read_text(encoding="utf-8")) and \
                any(len(r) > 2 and r[1] == name and r[2] == f"take {who}" for r in rows):
            raise SystemExit(f"{step} covers {name}, which {who} took; a new seat takes the review")
