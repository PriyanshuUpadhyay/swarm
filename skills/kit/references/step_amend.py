"""Amend a started plan run by one step, within the owner's `Amend: N` budget in plan.md (council C6).

  step_run.py amend <folder> <row> [--before <step>]

<row> is one row of the plan's step table. Its Writes must be `none`: a write or a scope change goes back to the
owner. `--before <step>` adds the new step to the Needs of that open step, so the plan keeps one end. The plan is
checked again by plan_chart.py; on a refusal plan.md stays as it was. Each amend spends 1 and logs `amend <step>`.
"""

import fcntl
import re
import subprocess
import sys
from pathlib import Path

KIT = Path(__file__).resolve().parents[1]
CHART = KIT / "blocks" / "plan-chart" / "scripts" / "plan_chart.py"


def budget(text):
    found = re.search(r"^Amend: (\d+)$", text, re.M)
    return int(found[1]) if found else 0


def spent(folder):
    events = Path(folder) / "events.log"
    lines = events.read_text(encoding="utf-8").splitlines() if events.exists() else []
    return sum(1 for l in lines if l.split("\t")[2:3] and l.split("\t")[2].startswith("amend "))


def amend(folder, row, *opts):
    plan = Path(folder) / "plan.md"
    if not plan.is_file():
        raise SystemExit(f"amend needs the run's plan.md in {folder}")
    # One amend at a time: each reads the budget and plan.md, then writes plan.md or puts it back.
    with plan.open("r+", encoding="utf-8") as held:
        try:
            fcntl.flock(held, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise SystemExit("plan.md is locked by a running amend; wait for it") from None
        add(folder, row, *opts)


def add(folder, row, *opts):
    import step_run
    sys.path.insert(0, str(CHART.parent))
    import plan_chart
    folder = Path(folder)
    plan = folder / "plan.md"
    if "\n" in row or "\r" in row:
        raise SystemExit("amend takes one table row on one line")
    if opts and (len(opts) != 2 or opts[0] != "--before"):
        raise SystemExit("amend <folder> <row> [--before <step>]")
    before = opts[1] if opts else None
    text = plan.read_text(encoding="utf-8")
    if (used := spent(folder)) >= (allowed := budget(text)):
        raise SystemExit(f"no Amend budget left ({used} of {allowed} spent); ask the owner")
    lines = text.split("\n")
    head = next((i for i, l in enumerate(lines) if plan_chart.HEAD.match(l)), None)
    if head is None:
        raise SystemExit("plan.md has no step table")
    new = plan_chart.read_plan(None, f"{lines[head]}\n{lines[head + 1]}\n{row.strip()}\n")[2]
    if len(new) != 1:
        raise SystemExit("amend takes one table row")
    name, _, _, blocks, _, writes, _ = new[0]
    if not re.fullmatch(r"\d\d-[^-\s]\S*", name):
        raise SystemExit(f"amended step {name or '(empty)'} is not NN-name")
    if writes != []:
        raise SystemExit("an amended step writes none; a write or a scope change goes back to the owner")
    if (folder / f"{name}.md").exists():
        raise SystemExit(f"step {name} exists")
    end = head + 2
    while end < len(lines) and lines[end].startswith("|"):
        end += 1
    lines.insert(end, row.strip())
    if before:
        if not (folder / f"{before}.md").is_file() or step_run.status_of(folder, before) != "open":
            raise SystemExit(f"{before} is not open")
        at = next(i for i in range(head + 2, end)
                  if plan_chart.read_plan(None, f"{lines[head]}\n{lines[i]}\n")[2][0][0] == before)
        cells = lines[at].strip().strip("|").split("|")
        need = cells[1].strip()
        cells[1] = f" {name.split('-', 1)[1] if need == 'none' else need + ', ' + name.split('-', 1)[1]} "
        lines[at] = "|" + "|".join(cells) + "|"
    # The ledger lists resolved blocks (parents and parts) and their conflicts alternatives, as check_ledger reads it.
    import block_resolver
    from step_checks import work_root
    resolved = block_resolver.resolve_blocks(work_root(folder)[0], {name: blocks}, "default", blocks_dir=KIT / "blocks",
                                             pack=False)[name]
    ledger = plan_chart.read_ledger(plan)
    rivals = sorted({r for b in resolved for r in plan_chart.conflicts(b)} - set(ledger))
    if add := [f"| {b} | Chosen | amended step {name} needs it |" for b in resolved if b not in ledger] + \
              [f"| {b} | Rejected | it conflicts with a block of amended step {name} |" for b in rivals]:
        at = next((i for i, l in enumerate(lines) if plan_chart.LEDGER_HEAD.match(l)), None)
        if at is None:
            raise SystemExit("plan.md has no blocks table")
        at += 2
        while at < len(lines) and lines[at].startswith("|"):
            at += 1
        lines[at:at] = add
    plan.write_text("\n".join(lines), encoding="utf-8")
    checked = subprocess.run([sys.executable, "-I", str(CHART), str(plan)], capture_output=True, text=True)
    if checked.returncode:
        plan.write_text(text, encoding="utf-8")
        raise SystemExit(f"amend refused: {checked.stderr.strip()}")
    try:
        step_run.start(folder, plan)
    except BaseException:
        plan.write_text(text, encoding="utf-8")
        (folder / f"{name}.md").unlink(missing_ok=True)
        raise
    step_run.log(folder, name, f"amend {name}")
    if before:
        uses = step_run.check_needs([r[:3] for r in step_run.table(plan)])[before]
        file = folder / f"{before}.md"
        with step_run.step_lock(folder, before, command="amend"):
            if step_run.status_of(folder, before) != "open":
                raise SystemExit(f"{before} was taken during the amend; add {name} to its Needs by hand")
            rest = file.read_text(encoding="utf-8").split("\n")
            step_run.write_step(file, "\n".join([rest[0], f"Uses: {', '.join(uses)}", *rest[2:]]))
    print(f"{name}: amended ({spent(folder)} of {allowed} spent)")
