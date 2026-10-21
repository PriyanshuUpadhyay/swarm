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
    import step_run
    plan = Path(folder) / "plan.md"
    if not plan.is_file():
        raise SystemExit(f"amend needs the run's plan.md in {folder}")
    if opts and (len(opts) != 2 or opts[0] != "--before"):
        raise SystemExit("amend <folder> <row> [--before <step>]")
    before = opts[1] if opts else None
    # One amend at a time: each reads the budget and plan.md, then writes plan.md or puts it back. The --before
    # step is locked first too, so no take of it can land between the plan write and its Uses line.
    with plan.open("r+", encoding="utf-8") as held:
        try:
            fcntl.flock(held, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise SystemExit("plan.md is locked by a running amend; wait for it") from None
        if before is None:
            return add(folder, row, None)
        if not (Path(folder) / f"{before}.md").is_file():
            raise SystemExit(f"{before} is not open")
        sys.path.insert(0, str(CHART.parent))
        import plan_chart
        if before not in [r[0] for r in plan_chart.read_plan(plan)[2]]:
            raise SystemExit(f"{before} is not a step of plan.md")
        with step_run.step_lock(folder, before, command="amend"):
            add(folder, row, before)


def add(folder, row, before):
    import step_run
    sys.path.insert(0, str(CHART.parent))
    import plan_chart
    folder = Path(folder)
    plan = folder / "plan.md"
    if "\n" in row or "\r" in row:
        raise SystemExit("amend takes one table row on one line")
    old_plan = plan.read_bytes()
    text = old_plan.decode("utf-8").replace("\r\n", "\n")  # as read_text gives it; a refusal writes old_plan back
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
        if step_run.status_of(folder, before) != "open":
            raise SystemExit(f"{before} is not open")
        at = next(i for i in range(head + 2, end)
                  if plan_chart.read_plan(None, f"{lines[head]}\n{lines[i]}\n")[2][0][0] == before)
        cells = lines[at].strip().strip("|").split("|")
        need = cells[1].strip().strip("`")
        cells[1] = f" {name.split('-', 1)[1] if need == 'none' else need + ', ' + name.split('-', 1)[1]} "
        lines[at] = "|" + "|".join(cells) + "|"
    # The ledger lists resolved blocks (parents and parts). check_ledger resolves
    # with no repo root, so a repo [off] block still counts as used, and so does it here.
    import block_resolver
    resolved = block_resolver.resolve_blocks(None, {name: blocks}, "default", blocks_dir=KIT / "blocks", pack=False)[name]
    ledger = plan_chart.read_ledger(plan)
    if add := [f"| {b} | Chosen | amended step {name} needs it |" for b in resolved if b not in ledger]:
        at = next((i for i, l in enumerate(lines) if plan_chart.LEDGER_HEAD.match(l)), None)
        if at is None:
            raise SystemExit("plan.md has no blocks table")
        at += 2
        while at < len(lines) and lines[at].startswith("|"):
            at += 1
        lines[at:at] = add
    page = plan.with_suffix(".html")
    old_page = page.read_bytes() if page.is_file() else None
    held = folder / f"{before}.md" if before else None
    old_held = held.read_bytes() if held else None

    def restore():
        """Put plan.md, plan.html, and the --before step file back as they were, and drop the new step file, so a
        refusal leaves no trace."""
        plan.write_bytes(old_plan)
        page.write_bytes(old_page) if old_page is not None else page.unlink(missing_ok=True)
        if held:
            held.write_bytes(old_held)
        (folder / f"{name}.md").unlink(missing_ok=True)

    # From the first write of plan.md, any stop (a refusal, an error, Ctrl-C) puts every file back.
    try:
        plan.write_text("\n".join(lines), encoding="utf-8")
        checked = subprocess.run([sys.executable, "-I", str(CHART), str(plan)], capture_output=True, text=True,
                                 encoding="utf-8", errors="replace")
        if checked.returncode:
            raise SystemExit(f"amend refused: {checked.stderr.strip()}")
        step_run.start(folder, plan)
        if held:
            uses = step_run.check_needs([r[:3] for r in step_run.table(plan)])[before]
            rest = old_held.decode("utf-8").split("\n")
            step_run.write_step(held, "\n".join([rest[0], f"Uses: {', '.join(uses)}", *rest[2:]]))
    except BaseException:
        restore()
        raise
    step_run.log(folder, name, f"amend {name}")
    print(f"{name}: amended ({spent(folder)} of {allowed} spent)")
