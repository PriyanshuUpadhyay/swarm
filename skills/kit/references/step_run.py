#!/usr/bin/env python3
"""Step-run files for any skill with a step table (| File | Needs | Holds |). See step-run.md.

  start <folder> <SKILL.md>    make one step file per table row, each with a todo and a result section
  status <folder>              print each step's status, whether it is ready, and whether it is stale
  take <folder> <step> <who> [--force]   claim the step and print its file; force takes a stale claim
  done <folder> <step> <n>      set the step done, only for take's claim #<n>, when every todo is checked
                                with evidence, no input is stale, and each runnable script check passes
  skip <folder> <step> <reason> skip an open or active step, with the reason
  block <folder> <step> <reason> block an open or active step, with the reason
  amend <folder> <row> [--before <step>]  add one `Writes: none` step to a plan run, within plan.md's Amend: N
                                (step_amend.py)

take, done, skip, block, and amend add one line to <folder>/events.log, and done adds one more for each check it runs,
so the path of a run stays readable after it ends.
A skill script with its own step graph or revision (implement) imports these functions and passes `need`
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

sys.path.insert(0, str(Path(__file__).resolve().parent))
from step_checks import run_checks, work_root  # noqa: E402
import step_scope  # noqa: E402

TODO_HEAD = "## Todo (check a box only with its evidence after the colon; `done` refuses an empty one)"
# Parse by this prefix, so a step file written under an older head wording still has its todos checked.
TODO_MARK = "## Todo ("
RESULT_HEAD = "## Result"


def kit_path(path):
    """Display paths in the installed kit through its stable link; keep source checkout paths."""
    root = Path(__file__).resolve().parents[1]
    installed = Path.home() / ".swarm/kit"
    if installed.resolve() == root:
        try:
            relative = Path(path).expanduser().resolve().relative_to(root)
        except ValueError:
            pass
        else:
            return str(Path("~/.swarm/kit") / relative)
    return str(path)


def todo_scan(text, ends=(RESULT_HEAD,)):
    """(head, end, cut) of the step's own todo list. Walk the TODO_MARK lines in order, each up to its next
    `## ` line. The first one ended by a line in `ends` is the list. One with no `- [` item is rules text and
    is skipped. One with an item but another end gives (its start, -1, that line), since a `## ` line pasted
    into the evidence would hand the list to a later head. No list at all gives (-1, -1, None). done
    refuses an end of -1; the head still marks where the rules text stops."""
    for head in re.finditer(rf"^(?={re.escape(TODO_MARK)}.*\n(?:(?!## ).*\n)*(## .*)$)", text, re.M):
        if head[1] in ends:
            return head.start(), head.start(1), None
        if re.search(r"^- \[", text[head.start():head.start(1)], re.M):
            return head.start(), -1, head[1]
    return -1, -1, None


def plain(text, *, keep_newlines=False):
    text = text.replace("\u2028", "\n").replace("\u2029", "\n")
    return "".join(c for c in text if c == "\t" or (keep_newlines and c == "\n")
                   or ord(c) >= 32 and not 127 <= ord(c) <= 159)


def table(skill_md):
    """Rows (name, needs, holds, blocks, writes, optional) of the first table whose header starts with
    | File | Needs | Holds |. blocks is None unless the table has `Blocks`, or `Block` beside `Writes` (a plan);
    writes is None with no Writes column, else the paths ([] for `none`); optional is a File cell ending `(optional)`."""
    rows, inside = [], False
    for line in Path(skill_md).read_text(encoding="utf-8").splitlines():
        if re.match(r"\|\s*File\s*\|\s*Needs\s*\|\s*Holds\s*\|", line):
            inside = True
            columns = [c.strip() for c in line.strip().strip("|").split("|")]
            at = {c: i for i, c in enumerate(columns)}
            block_at = at.get("Blocks", at.get("Block") if "Writes" in at else None)
        elif inside and line.startswith("|") and not line.startswith("|---"):
            cells = [c.strip() for c in line.strip().strip("|").split("|")]
            cell = lambda i: cells[i].strip("`") if i is not None and i < len(cells) else ""
            blocks = [b.strip() for b in cell(block_at).split(",") if b.strip()] if block_at is not None else None
            writes = None
            if "Writes" in at:
                writes = step_scope.write_paths(cell(at["Writes"]))
            name = cells[0]
            optional = bool(re.search(r"\s*\(optional\)$", name))
            name = re.sub(r"\s*\(optional\)$", "", name).strip("`")
            rows.append((name.removesuffix(".md"), cell(1), cell(2), blocks, writes, optional))
        elif inside and not line.startswith("|"):
            break
    if not rows:
        raise SystemExit(f"no | File | Needs | Holds | table in {skill_md}")
    return rows


def need_files(need, names):
    """A Needs cell names steps by their word ("question", "local, web"); map each to its file."""
    found = []
    for word in re.split(r",|\band\b", need):
        word = word.strip()
        if word and word != "none":
            first = word.split()[0]
            exact = [n for n in names if n.split("-", 1)[1] == first]  # an exact word wins over a prefix
            found += exact or [n for n in names if n.split("-", 1)[1].startswith(first)]
    return list(dict.fromkeys(found))


def check_needs(rows):
    """Each step's needed step files, or SystemExit naming the step and the word of the first defect."""
    names = [r[0] for r in rows]
    uses, word_of = {}, {}
    for name, need, _ in rows:
        uses[name] = []
        for word in (w.strip() for w in re.split(r",|\band\b", need)):
            if not word or word == "none":
                continue
            found = need_files(word, names)
            if not found:
                raise SystemExit(f"step {name} needs {word!r}, which no step gives")
            if len(found) > 1:
                raise SystemExit(f"step {name} needs {word!r}, which fits {', '.join(found)}; name one")
            if found[0] not in uses[name]:
                uses[name] += found
            word_of[name, found[0]] = word
    done, path = set(), []

    def walk(name):
        if name in path:
            ring = path[path.index(name):] + [name]
            raise SystemExit("cycle: " + " -> ".join(f"{a} needs {word_of[a, b]!r}" for a, b in zip(ring, ring[1:])) + f" -> {name}")
        if name not in done:
            path.append(name)
            for n in uses[name]:
                walk(n)
            path.pop()
            done.add(name)

    for name in names:
        walk(name)
    return uses


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


def claim_of(folder, step):
    """The number of the step's newest take, from events.log."""
    events = Path(folder) / "events.log"
    lines = events.read_text(encoding="utf-8").splitlines() if events.exists() else []
    rows = (line.split("\t") for line in lines)
    return sum(1 for r in rows if len(r) > 2 and r[1] == step and r[2].startswith("take "))


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
    selected = {name: ids for name, _, _, ids, *_ in rows if ids is not None}
    blocks = {}
    if selected and not all((folder / f"{name}.md").exists() for name in names):
        sys.path.insert(0, str(Path(__file__).resolve().parent))
        import block_resolver
        root, piece = work_root(folder)
        site = str(piece.relative_to(root))
        blocks, _, _ = block_resolver.resolve_blocks(
            root, selected, "default", blocks_dir=Path(__file__).resolve().parents[1] / "blocks",
            params_override={"site": site, "shots": str(Path(site) / "shots")}, judge_todos=True)
    uses = check_needs([r[:3] for r in rows])
    folder.mkdir(parents=True, exist_ok=True)
    for name, need, holds, _, writes, optional in rows:
        file = folder / f"{name}.md"
        if file.exists():
            continue
        read, rules, todos = blocks.get(name, ([], "", []))
        if any(token in rules + "\n".join(todos) for token in ("blocks/", "the kit's `references/", "<kit root>")):
            rules = f"Kit blocks (`blocks/<id>/`) and the kit's `references/` are under {kit_path(Path(__file__).resolve().parents[1])} (the kit root).\n{rules}"
        extra = (f"Skills read: {', '.join(read)}\n\n## Rules for this step\n{rules}\n" if read else "")
        scope = "" if writes is None else f"Writes: {', '.join(writes) or 'none'}\n" + ("Optional: yes\n" if optional else "")
        file.write_text(f"Status: open\nUses: {', '.join(uses[name])}\n{scope}\n{extra}"
                        f"{TODO_HEAD}\n" + "".join(f"- [ ] {item}: \n" for item in [holds] + todos)
                        + f"\n{RESULT_HEAD}\n", encoding="utf-8")
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
    # events.log keeps one tab-split line per event, so a name with a tab or newline could forge a take.
    if not who.strip() or plain(who) != who or "\t" in who:
        raise SystemExit("take needs an agent name with no tab, newline, or other control character")
    with step_lock(folder, step, command="take"):
        file = Path(folder) / f"{step}.md"
        lines = file.read_text(encoding="utf-8").split("\n")
        owner = lines[0].removeprefix("Status: ")
        # The claim number is last, so a name that holds " #" stays whole.
        holder = re.sub(r" #\d+$", "", owner.removeprefix("active "))
        number = re.search(r" #(\d+)$", owner) if owner.startswith("active ") else None
        if owner.startswith("active ") and holder != who and not force:
            raise SystemExit(f"{step} is active for {holder}; use --force if that run is gone")
        need = needs(folder, step) if need is None else need
        if not ready(folder, step, need):
            raise SystemExit(f"{step} is not ready; it needs {', '.join(need)}")
        step_scope.check_reviewer(folder, step, who)
        uses = ", ".join(f"{n}@{rev(folder, n)}" for n in need)
        text = "\n".join([f"Status: active {who} #{max(claim_of(folder, step), int(number[1]) if number else 0) + 1}", f"Uses: {uses}", *lines[2:]])
        write_step(file, text)
        if on_claim is not None:
            try:
                on_claim(Path(folder))
            except BaseException as error:
                message = plain(str(error), keep_newlines=True).replace("\n", " ").replace("\t", " ")
                log(folder, step, f"take {who} failed: {message}")
                raise
            text = file.read_text(encoding="utf-8")
        # Logged inside the lock, so a take --force cannot count the same claim number.
        log(folder, step, f"take {who}")
        # done compares the work tree with this one, so only changes made after the take count.
        if step_scope.writes(text) is not None and (tree := step_scope.work_tree(folder)):  # SystemExit: done refuses
            log(folder, step, f"base {tree[1]}")
    print(text)


def done(folder, step, claim, rev=None, ends=(RESULT_HEAD,), rev_of=revision):
    with step_lock(folder, step, command="done"):
        file = Path(folder) / f"{step}.md"
        text = file.read_text(encoding="utf-8")
        start, end, cut = todo_scan(text, ends)
        if cut:
            raise SystemExit(f"{step} is not done: no todo list: the line {cut!r} inside the list ends it; "
                             "indent it or remove the leading '## '")
        if end < 0:
            raise SystemExit(f"{step} is not done: no todo list ({TODO_MARK}... head, its items, then {RESULT_HEAD}) found")
        todo = text[start:end]
        # Only `- [x]` counts as checked; `- [X]` is named as open, so it is never passed unread.
        open_items = [l for l in todo.splitlines() if l.startswith(("- [ ]", "- [X]"))]
        empty = [l for l in todo.splitlines() if l.startswith("- [x]") and not (":" in l[5:] and l.rsplit(":", 1)[-1].strip())]
        if open_items or empty:
            raise SystemExit(f"{step} is not done:\n" + "\n".join(open_items + [f"no evidence: {l}" for l in empty]))
        state = text.split("\n", 1)[0].removeprefix("Status: ")
        number = str(claim).removeprefix("#")
        # events.log also counts the takes, so a stale copy of the file written back cannot bring an old claim back.
        if not (state.startswith("active ") and state.rsplit(" #", 1)[-1] == number
                and number.isascii() and number.isdigit() and int(number) >= claim_of(folder, step)):
            raise SystemExit(f"{step} is not done: claim #{str(claim).removeprefix('#')} is not the current one; "
                             f"the step is {plain(state)}")
        if old := stale(folder, step, rev_of):
            raise SystemExit(f"{step} is not done: its input changed after take: {', '.join(old)}; take it again")
        step_scope.check(folder, step, text)
        run_checks(folder, step, todo, text)
        if file.read_text(encoding="utf-8") != text:
            raise SystemExit(f"{step} is not done: its file changed while its checks ran; run done again")
        if old := stale(folder, step, rev_of):
            raise SystemExit(f"{step} is not done: its input changed while its checks ran: {', '.join(old)}; take it again")
        rev = rev or revision(folder, step)
        write_step(file, f"Status: done {rev}\n" + text.split("\n", 1)[1])
    log(folder, step, f"done {rev}")
    print(f"{step}: done {rev}")


def skip(folder, step, *why):
    reason = plain(" ".join(why), keep_newlines=True).replace("\n", " ").replace("\t", " ").strip()
    if not reason:
        raise SystemExit("skip needs a reason")
    file = Path(folder) / f"{step}.md"
    with step_lock(folder, step, command="skip"):
        state = status_of(folder, step)
        if state.split()[0] not in ("open", "active"):
            raise SystemExit(f"{step} is {state}; only an open or active step can be skipped")
        text = file.read_text(encoding="utf-8")
        if step_scope.writes(text) is not None and step_scope.header(text, "Optional") != "yes":
            raise SystemExit(f"{step} is not marked (optional) in the plan; ask the owner to amend the plan")
        write_step(file, f"Status: skipped {reason}\n" + file.read_text(encoding="utf-8").split("\n", 1)[1])
    log(folder, step, f"skip {reason}")
    print(f"{step}: skipped {reason}")


def block(folder, step, *why):
    reason = plain(" ".join(why), keep_newlines=True).replace("\n", " ").replace("\t", " ").strip()
    if not reason:
        raise SystemExit("block needs a reason")
    file = Path(folder) / f"{step}.md"
    if not file.is_file():
        raise SystemExit(f"step file does not exist: {file}")
    with step_lock(folder, step, command="block"):
        state = status_of(folder, step)
        if state.split()[0] not in ("open", "active"):
            raise SystemExit(f"{step} is {state}; only an open or active step can be blocked")
        write_step(file, f"Status: blocked {reason}\n" + file.read_text(encoding="utf-8").split("\n", 1)[1])
    log(folder, step, f"block {reason}")
    print(f"{step}: blocked {reason}")


if __name__ == "__main__":
    if hasattr(sys.stdout, "reconfigure"):
        # Escape non-ASCII output when the console uses an ASCII encoding.
        sys.stdout.reconfigure(errors="backslashreplace")
    from step_amend import amend
    commands = {"start": start, "status": status, "take": take, "done": done, "skip": skip, "block": block, "amend": amend}
    arity = {"start": (2, 2), "status": (1, 1), "take": (3, 3), "done": (3, 3), "skip": (3, None), "block": (3, None),
             "amend": (2, 4)}
    if len(sys.argv) < 2 or sys.argv[1] not in commands:
        raise SystemExit(__doc__)
    try:
        args = sys.argv[2:]
        command = sys.argv[1]
        force = command == "take" and "--force" in args
        if command == "take":
            args = [arg for arg in args if arg != "--force"]
        if command in ("done", "skip", "block", "amend") and "--force" in args:
            raise SystemExit("done <folder> <step> <n> accepts no --force" if command == "done" else f"{command} accepts no --force")
        minimum, maximum = arity[command]
        if len(args) < minimum or maximum is not None and len(args) > maximum:
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
