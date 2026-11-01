"""The step graph and input state of a step run: which steps a step needs, each step's status and revision,
and whether a step is ready or stale. step_run.py imports each name, so `step_run.<name>` keeps working."""

import hashlib
import re
from pathlib import Path


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


def ready(folder, step, need=None, rev=revision):
    """Each needed step is done or skipped, and no done step on the chain before it is stale."""
    need = needs(folder, step) if need is None else need
    return not waiting(folder, need) and not stale_before(folder, need, rev)


def waiting(folder, need):
    """The needed steps that are not done or skipped yet."""
    return [n for n in need if status_of(folder, n).split()[0] not in ("done", "skipped")]


def stale(folder, step, rev=revision):
    """The needed steps whose revision changed after this step took them."""
    line = (Path(folder) / f"{step}.md").read_text(encoding="utf-8").splitlines()[1]
    used = dict(u.strip().split("@", 1) for u in line.removeprefix("Uses:").split(",") if "@" in u)
    return [n for n, r in used.items() if rev(folder, n) != r]


def stale_before(folder, need, rev=revision):
    """(step, its changed inputs) for each done step on the chain before a step, its needed steps first and then
    the steps they need, whose own input changed after it was taken. Its result is old, so a step that uses it,
    even two links on, would build on an old result. A step that is not done ends the walk on its branch, and
    so does a name with no step file, which a hand-written Uses line can hold."""
    found, seen, queue = [], set(), list(need)
    while queue:
        step = queue.pop(0)
        if step in seen or not (Path(folder) / f"{step}.md").exists() or status_of(folder, step).split()[0] != "done":
            continue
        seen.add(step)
        if old := stale(folder, step, rev):
            found.append((step, old))
        queue += needs(folder, step)
    return found


def stale_note(folder, chain, rev=revision):
    """(the stale step, why it is stale, what to do). The step to take again is the first stale one whose own chain holds no stale
    step, because take refuses a step that has a stale step behind it. When it also waits for an input that is
    not done, such as one that is taken again, that input comes first."""
    step, old = next(((s, o) for s, o in chain if not stale_before(folder, needs(folder, s), rev)), chain[-1])
    before = waiting(folder, needs(folder, step))
    return (step, f"(its input {', '.join(old)} changed after it was done)",
            (f"finish {', '.join(before)}, then " if before else "") + f"take {step} again")

