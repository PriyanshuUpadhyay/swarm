"""The write scope of a step: `take` records the work tree, and `done` refuses a change outside the Writes of every step beside it.

A step file from a plan with a Writes column has a `Writes:` line under `Uses:` (see step_run.start). A step file
with no such line, or a run outside git, has no scope check.
"""

import hashlib
import os
import shutil
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
    if not paths:
        raise SystemExit("a Writes cell is empty; write `none` for a step that writes only its run folder")
    if "none" in paths and paths != ["none"]:
        raise SystemExit(f"Writes `none` stands alone, not beside paths: {cell}")
    return [] if paths == ["none"] else paths


def writes(text):
    """The step's Writes paths ([] for `none`), or None when the step file has no Writes line."""
    value = header(text, "Writes")
    return None if value is None else write_paths(value)


# `git rev-parse --local-env-vars`: the variables that tie git to one repo. A caller's GIT_DIR or GIT_WORK_TREE would
# point git at another repo; the user's own settings, such as GIT_CONFIG_GLOBAL, stay.
LOCAL_ENV = {"GIT_ALTERNATE_OBJECT_DIRECTORIES", "GIT_CONFIG", "GIT_CONFIG_PARAMETERS", "GIT_CONFIG_COUNT",
             "GIT_OBJECT_DIRECTORY", "GIT_DIR", "GIT_WORK_TREE", "GIT_IMPLICIT_WORK_TREE", "GIT_GRAFT_FILE",
             "GIT_INDEX_FILE", "GIT_NO_REPLACE_OBJECTS", "GIT_REPLACE_REF_BASE", "GIT_PREFIX", "GIT_SHALLOW_FILE",
             "GIT_COMMON_DIR"}


def clean_env():
    """os.environ with no repo-local GIT_ variable, and no GIT_CONFIG_KEY_n or GIT_CONFIG_VALUE_n of a dropped count.
    step_checks.work_root uses it too, so both find the same repo."""
    return {k: v for k, v in os.environ.items()
            if k not in LOCAL_ENV and not k.startswith(("GIT_CONFIG_KEY_", "GIT_CONFIG_VALUE_"))}


def git(root, *args, env=None):
    env = clean_env() if env is None else env
    try:
        return subprocess.run(["git", "-C", str(root), *args], capture_output=True, text=True, encoding="utf-8",
                              errors="replace", env=env)
    except OSError as error:  # no git on PATH: the caller reads a failed run
        return subprocess.CompletedProcess(args, 127, "", str(error))


def work_tree(folder):
    """(root, tree id) of the work tree as it is now, untracked files included, or None outside git. SystemExit
    when git fails inside a repo, so a failure never reads as "no git".

    A copy of the index takes the files, so the real index and the stash stay as they are. The copy keeps the
    index's stat cache and its modify time: git's racy-clean test compares a file's time with the index file's, so
    a fresh time would let a same-size edit in that second look unchanged.
    """
    root, _ = work_root(folder)
    env = clean_env()
    here = Path(folder).resolve()  # not root: work_root already took git's answer, which may skip a broken repo
    nearest = next((p for p in (here, *here.parents) if (p / ".git").exists()), None)
    found = git(root, "rev-parse", "--show-toplevel", env=env)
    # A .git that git cannot read (a broken HEAD, a dubious owner) refuses. So does one that git passes over for an
    # outer repo, as it does for a broken nested repo. A missing git binary (127) stays a printed skip.
    if found.returncode == 127 or (found.returncode and nearest is None):
        return None
    if found.returncode:
        raise SystemExit(f"scope check failed: git rev-parse: {found.stderr.strip()}")
    if nearest is not None and not Path(found.stdout.strip()).samefile(nearest):  # samefile: a case-blind disk
        raise SystemExit(f"scope check failed: git reads {found.stdout.strip()}, not the repo at {nearest}")
    index = Path(git(root, "rev-parse", "--path-format=absolute", "--git-path", "index", env=env).stdout.strip())
    with tempfile.TemporaryDirectory() as tmp:
        copy = Path(tmp, "index")
        if index.is_file():
            shutil.copy2(index, copy)
        env["GIT_INDEX_FILE"] = str(copy)
        for args in (("add", "-A"), ("write-tree",)):
            if (out := git(root, *args, env=env)).returncode:
                raise SystemExit(f"scope check failed: git {' '.join(args)}: {out.stderr.strip()}")
    return root, out.stdout.strip()


def config_hash(root):
    """A hash of the repo's .blocks.toml, or `none`. take records it, because git add skips an ignored file."""
    file = Path(root) / ".blocks.toml"
    try:
        return hashlib.sha256(file.read_bytes()).hexdigest()[:16] if file.is_file() else "none"
    except OSError as error:
        raise SystemExit(f"scope check failed: {file} cannot be read: {error.strerror}") from None


def events(folder):
    file = Path(folder) / "events.log"
    return file.read_text(encoding="utf-8").splitlines() if file.exists() else []


def snapshot(folder):
    """`<tree> <config hash> <events>` for take's base line, or None outside git. <events> is the events.log line
    count read before the tree, so a step done while the tree is read still counts as one that ran beside it."""
    seen = len(events(folder))
    now = work_tree(folder)
    return now and f"{now[1]} {config_hash(now[0])} {seen}"


def base(folder, step):
    """`<tree> <config hash> <events>` that the step's newest take recorded, or None."""
    found = None
    for row in (line.split("\t") for line in events(folder)):
        if len(row) > 2 and row[1] == step:
            if row[2].startswith("take "):
                found = None
            elif row[2].startswith("base "):
                found = row[2].removeprefix("base ")
    return found


def done_before_take(folder, step):
    """The steps whose newest event, a done line, comes before the step's newest base read: they did not run beside
    it. A later take of such a step makes it run beside it again.

    The read point is the events count in the step's `base` line, which only a take that claimed the step writes;
    the agent name in a `take` line is free text, so it cannot tell a failed take apart.
    """
    parts = (base(folder, step) or "").split(" ")
    seen = int(parts[2]) if len(parts) > 2 and parts[2].isdigit() else 0
    last = {}
    for i, row in enumerate(line.split("\t") for line in events(folder)):
        if len(row) > 2 and row[1] != step and row[2].startswith(("take ", "done ")):
            last[row[1]] = (i, row[2].startswith("done "))
    return {name for name, (i, done) in last.items() if done and i < seen}


def inside(path, scope):
    """True when the path is the scope path or under that folder, compared by path parts."""
    path, scope = PurePosixPath(path).parts, PurePosixPath(scope).parts
    return path[:len(scope)] == scope


def check(folder, step, text):
    """SystemExit when the step changed `.blocks.toml`, or a path outside its own Writes, the Writes of each step
    that can run beside it (active or done, and not one it needs), and the run folder; a printed `not run: scope:`
    line when the check cannot run."""
    own = writes(text)
    if own is None:
        return
    start, config = ((base(folder, step) or "").split(" ") + [None, None])[:2]
    start = start or None
    now = work_tree(folder)
    if now is None:
        print("not run: scope: no git work tree")
        return
    if start is None:  # take records one in a git repo, so its loss must not skip the check
        raise SystemExit(f"{step} is not done: its take recorded no base; take it again")
    if config is None:  # a take by an older kit logged the tree alone
        raise SystemExit(f"{step} is not done: its take recorded no .blocks.toml hash; take it again")
    root, tree = now
    allowed = list(own)
    # A step that ran beside this one counts too, also when it was done first: its files cannot be told apart.
    # Its status says so, so the gap between done's file write and its log line does not matter. A step done
    # before this take did not run beside it.
    needed = earlier(folder, step) | done_before_take(folder, step)
    for other in sorted(Path(folder).glob("[0-9][0-9]-*.md")):
        if other.stem != step and other.stem not in needed:
            body = other.read_text(encoding="utf-8")
            if body.startswith(("Status: active ", "Status: done ")):
                allowed += writes(body) or []
    run = Path(folder).resolve()
    if run.is_relative_to(root.resolve()):
        allowed.append(run.relative_to(root.resolve()).as_posix())
    diff = git(root, "diff", "-z", "--name-only", "--no-renames", start, tree)
    if diff.returncode:  # a pruned base tree, say; refuse, so a failed diff never passes the check
        raise SystemExit(f"{step} is not done: scope check failed: {diff.stderr.strip()}; take the step again")
    changed = [p for p in diff.stdout.split("\0") if p]
    if ".blocks.toml" in changed or config != config_hash(root):  # its [off] table would turn off this step's checks
        raise SystemExit(f"{step} is not done: it changed .blocks.toml, which only the owner changes; undo it")
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
    rows = [l.split("\t") for l in events(folder)]
    for name in sorted(earlier(folder, step)):
        if writes((Path(folder) / f"{name}.md").read_text(encoding="utf-8")) and \
                any(len(r) > 2 and r[1] == name and r[2] == f"take {who}" for r in rows):
            raise SystemExit(f"{step} covers {name}, which {who} took; a new seat takes the review")
