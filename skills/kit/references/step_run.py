#!/usr/bin/env python3
"""Step-run files for any skill with a step table (| File | Needs | Holds |). See step-run.md.

  start <folder> <SKILL.md>    make one step file per table row, each with a todo and a result section
  status <folder>              print each step's status, whether it is ready, and whether it is stale
  take <folder> <step> <who> [--force]   claim the step and print its file; force takes a stale claim
  done <folder> <step>          set the step done, only when every todo is checked with evidence

take and done add one line to <folder>/events.log, so the path of a run stays readable after it ends.
A skill script with its own step graph or revision (flow) imports these functions and passes `need`
and `rev`.
"""

import datetime
import fcntl
import hashlib
import os
import re
import stat
import sys
import tempfile
from contextlib import contextmanager
from pathlib import Path

TODO_HEAD = "## Todo (check a box only with its evidence after the colon; `done` refuses an empty one)"
RESULT_HEAD = "## Result"


def plain(text, *, keep_newlines=False):
    text = text.replace("\u2028", "\n").replace("\u2029", "\n")
    return "".join(c for c in text if c == "\t" or (keep_newlines and c == "\n")
                   or ord(c) >= 32 and not 127 <= ord(c) <= 159)


def table(skill_md):
    """Rows of the first table whose header starts with | File | Needs | Holds |."""
    rows, inside = [], False
    for line in Path(skill_md).read_text(encoding="utf-8").splitlines():
        if re.match(r"\|\s*File\s*\|\s*Needs\s*\|\s*Holds\s*\|", line):
            inside = True
        elif inside and line.startswith("|") and not line.startswith("|---"):
            cells = [c.strip().strip("`") for c in line.strip("|").split("|")]
            rows.append((cells[0].removesuffix(".md"), cells[1], cells[2]))
        elif inside and not line.startswith("|"):
            break
    if not rows:
        raise SystemExit(f"no | File | Needs | Holds | table in {skill_md}")
    return rows


def need_files(need, names):
    """A Needs cell names steps by their word ("question", "local, web"); map each to its file."""
    return list(dict.fromkeys(n for word in re.split(r",|\band\b", need) if word.strip() and word.strip() != "none"
                              for n in names if n.split("-", 1)[1].startswith(word.strip().split()[0])))


def needs(folder, step):
    line = (Path(folder) / f"{step}.md").read_text(encoding="utf-8").splitlines()[1]
    return [n.strip().split("@")[0] for n in line.removeprefix("Uses:").split(",") if n.strip()]


def revision(folder, step):
    rest = (Path(folder) / f"{step}.md").read_text(encoding="utf-8").split("\n", 1)[1]
    return hashlib.sha1(rest.encode()).hexdigest()[:12]


def status_of(folder, step):
    return (Path(folder) / f"{step}.md").read_text(encoding="utf-8").splitlines()[0].removeprefix("Status: ")


def steps(folder):
    return sorted(p.stem for p in Path(folder).glob("[0-9][0-9]-*.md"))


def ready(folder, step, need=None):
    need = needs(folder, step) if need is None else need
    return all(status_of(folder, n).split()[0] in ("done", "skipped") for n in need)


def stale(folder, step, rev=revision):
    """The needed steps whose revision changed after this step took them."""
    line = (Path(folder) / f"{step}.md").read_text(encoding="utf-8").splitlines()[1]
    used = dict(u.strip().split("@", 1) for u in line.removeprefix("Uses:").split(",") if "@" in u)
    return [n for n, r in used.items() if rev(folder, n) != r]


def log(folder, step, event):
    with (Path(folder) / "events.log").open("a", encoding="utf-8") as f:
        f.write(f"{datetime.datetime.now().isoformat(timespec='seconds')}\t{step}\t{event}\n")


def write_step(file, text):
    fd, name = tempfile.mkstemp(dir=file.parent, prefix=f"{file.stem}.", suffix=".tmp")
    temporary = Path(name)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as stream:
            mode = stat.S_IMODE(file.stat().st_mode) if file.exists() else 0o644
            os.fchmod(stream.fileno(), mode)
            stream.write(text)
        os.replace(temporary, file)
    finally:
        temporary.unlink(missing_ok=True)


def start(folder, skill_md):
    folder = Path(folder)
    rows = table(skill_md)
    names = [r[0] for r in rows]
    folder.mkdir(parents=True, exist_ok=True)
    for name, need, holds in rows:
        file = folder / f"{name}.md"
        if file.exists():
            continue
        file.write_text(f"Status: open\nUses: {', '.join(need_files(need, names))}\n\n"
                        f"{TODO_HEAD}\n- [ ] {holds}: \n\n{RESULT_HEAD}\n", encoding="utf-8")
    print(folder)


def status(folder, need_of=None, rev=revision):
    for step in steps(folder):
        s = status_of(folder, step)
        old = stale(folder, step, rev) if s.split()[0] in ("active", "done") else []
        note = (f"  (stale: {', '.join(old)})" if old
                else "  (ready)" if s == "open" and ready(folder, step, need_of and need_of.get(step)) else "")
        print(f"{step}: {s}{note}")


@contextmanager
def step_lock(folder, step, command="take"):
    lock = Path(folder) / f"{step}.lock"
    fd = os.open(lock, os.O_CREAT | os.O_WRONLY, 0o600)
    try:
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise SystemExit(f"{step} is locked by a running {command}; wait for it") from None
        yield
    finally:
        os.close(fd)


def take(folder, step, who, need=None, rev=revision, force=False, on_claim=None):
    with step_lock(folder, step, command="take"):
        file = Path(folder) / f"{step}.md"
        lines = file.read_text(encoding="utf-8").split("\n")
        owner = lines[0].removeprefix("Status: ")
        if owner.startswith("active ") and owner != f"active {who}" and not force:
            raise SystemExit(f"{step} is active for {owner.removeprefix('active ')}; use --force if that run is gone")
        need = needs(folder, step) if need is None else need
        if not ready(folder, step, need):
            raise SystemExit(f"{step} is not ready; it needs {', '.join(need)}")
        uses = ", ".join(f"{n}@{rev(folder, n)}" for n in need)
        text = "\n".join([f"Status: active {who}", f"Uses: {uses}", *lines[2:]])
        write_step(file, text)
        if on_claim is not None:
            try:
                on_claim(Path(folder))
            except BaseException as error:
                message = plain(str(error), keep_newlines=True).replace("\n", " ").replace("\t", " ")
                log(folder, step, f"take {who} failed: {message}")
                raise
            text = file.read_text(encoding="utf-8")
    log(folder, step, f"take {who}")
    print(text)


def done(folder, step, rev=None):
    with step_lock(folder, step, command="done"):
        file = Path(folder) / f"{step}.md"
        text = file.read_text(encoding="utf-8")
        todo = text.split(TODO_HEAD, 1)[1].split(RESULT_HEAD, 1)[0] if TODO_HEAD in text else ""
        open_items = [l for l in todo.splitlines() if l.startswith("- [ ]")]
        empty = [l for l in todo.splitlines() if l.startswith("- [x]") and not l.split(":", 1)[-1].strip()]
        if open_items or empty:
            raise SystemExit(f"{step} is not done:\n" + "\n".join(open_items + [f"no evidence: {l}" for l in empty]))
        rev = rev or revision(folder, step)
        write_step(file, f"Status: done {rev}\n" + text.split("\n", 1)[1])
    log(folder, step, f"done {rev}")
    print(f"{step}: done {rev}")


if __name__ == "__main__":
    if hasattr(sys.stdout, "reconfigure"):
        # Escape non-ASCII output when the console uses an ASCII encoding.
        sys.stdout.reconfigure(errors="backslashreplace")
    commands = {"start": start, "status": status, "take": take, "done": done}
    arity = {"start": (2, 2), "status": (1, 1), "take": (3, 3), "done": (2, 2)}
    if len(sys.argv) < 2 or sys.argv[1] not in commands:
        raise SystemExit(__doc__)
    try:
        args = sys.argv[2:]
        command = sys.argv[1]
        force = command == "take" and "--force" in args
        if command == "take":
            args = [arg for arg in args if arg != "--force"]
        if command == "done" and "--force" in args:
            raise SystemExit("done <folder> <step> accepts no --force")
        minimum, maximum = arity[command]
        if len(args) < minimum or len(args) > maximum:
            raise SystemExit(next(line.strip() for line in __doc__.splitlines()
                                  if line.strip().split()[:1] == [command]))
        commands[command](*args, **({"force": force} if command == "take" else {}))
        sys.stdout.flush()
    except BrokenPipeError:
        fd = os.open(os.devnull, os.O_WRONLY)
        # Python flushes stdout at exit; devnull prevents another error from the closed pipe.
        os.dup2(fd, sys.stdout.fileno())
        os.close(fd)
        sys.exit(0)
