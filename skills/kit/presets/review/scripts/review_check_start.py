# Review start locking, parent loop budgets, and checklist creation.
import fcntl, hashlib, json, os, re, shutil, subprocess, tempfile, time
from contextlib import contextmanager
from pathlib import Path


@contextmanager
def start_lock():
    from review_check import START_LOCK_WAIT, git, home
    root = Path(git("rev-parse", "--path-format=absolute", "--git-common-dir").strip()) / "review-check"
    shared = root
    writable = True
    try:
        root.mkdir(parents=True, exist_ok=True)
        # An existing lock may still open when the folder denies atomic ledger writes.
        with tempfile.TemporaryFile(dir=root):
            pass
        fd = os.open(root / "review-check.lock", os.O_CREAT | os.O_WRONLY, 0o600)
    except OSError:
        writable, root = False, home()
        root.mkdir(parents=True, exist_ok=True)
        fd = os.open(root / "review-check.lock", os.O_CREAT | os.O_WRONLY, 0o600)
    try:
        deadline = time.monotonic() + START_LOCK_WAIT
        while True:
            try:
                fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
                break
            except BlockingIOError:
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise SystemExit(f"start refused: {root / 'review-check.lock'} is locked by a running start; "
                                     f"waited {START_LOCK_WAIT:g} s; try again when it finishes") from None
                time.sleep(min(0.1, remaining))
        yield shared, writable
    finally:
        os.close(fd)



def start(target, *opts):
    from review_check import start_lock, start_locked
    with start_lock() as (ledger_home, writable):
        return start_locked(target, *opts, ledger_home=ledger_home, ledger_writable=writable)



def start_locked(target, *opts, ledger_home, ledger_writable):
    from review_check import DEFINES, PLAIN, ROUND_CAP, ci_of, family, git, glob_match, home, lint_config, load_rules, local_patch, parse_diff, references, repo_name, revision, seat_map, stale_verdict, step_run, tool_owned, units_of
    options = {}
    while opts:
        if opts == ("--patch",):
            raise SystemExit("--patch needs a file")
        if len(opts) < 2 or opts[0] not in ("--patch", "--parent-loop", "--continue") or opts[0] in options:
            raise SystemExit("start accepts --patch FILE, --parent-loop KEY, and --continue FILE once each")
        options[opts[0]], opts = opts[1], opts[2:]
    parent_loop = options.get("--parent-loop")
    if parent_loop is None:
        branch = git("branch", "--show-current").strip()
        if not branch:
            raise SystemExit("detached HEAD needs --parent-loop KEY")
        parent_loop = f"branch:{branch}"
    if not parent_loop.strip() or any(ord(c) < 32 or ord(c) == 127 for c in parent_loop):
        raise SystemExit("--parent-loop needs a nonempty key with no control characters")
    if target == "local":
        prefix, base, head = "local", git("rev-parse", "HEAD").strip(), None
        patch = local_patch()
    elif ".." in target:
        base, head = (git("rev-parse", "--verify", f"{p}^{{commit}}").strip() for p in target.split("..", 1))
        prefix = f"range-{base[:7]}-{head[:7]}"
        patch = git("diff", *PLAIN, base, head)
    else:
        raise SystemExit("target is `local` or `<base>..<head>`")
    is_patch = "--patch" in options
    if is_patch:
        try:
            patch = Path(options['--patch']).read_text(encoding="utf-8")
        except OSError as error:
            raise SystemExit(f"cannot read patch {options['--patch']}: {error.strerror}") from None

    d = home()
    prior = []
    loop_runs = []
    if not is_patch:
        for target_file in d.glob("*/target.json"):
            metadata = json.loads(target_file.read_text(encoding="utf-8"))
            if metadata.get("patch"):
                continue
            if metadata.get("parent_loop") == parent_loop:
                loop_runs.append((target_file.parent, metadata))
            before = metadata["head"]
            if before and (before == (head or base) or subprocess.run(
                    ["git", "merge-base", "--is-ancestor", before, head or base], capture_output=True).returncode == 0):
                prior.append((target_file.parent, before))
        prior.sort(key=lambda item: (int(git("rev-list", "--count", item[1])),
                                     (item[0] / "target.json").stat().st_mtime, item[0].name), reverse=True)
        newest = {}
        for previous, before in prior:
            newest.setdefault(before, (previous, before))
        prior = list(newest.values())
        for previous, before in prior:
            if target != "local" and before == head:
                continue
            result = previous / "03-verdict.md"
            lines = result.read_text(encoding="utf-8").splitlines() if result.exists() else []
            if stale := stale_verdict(previous, lines):
                raise SystemExit(f"start refused by {previous}: its verdict is stale ({stale} changed); "
                                 f"run `verdict {previous.name}` again")
            counts = re.search(r"; (\d+) fix, (\d+) ask", lines[2]) if len(lines) > 2 else None
            if not lines or not lines[0].startswith("Status: done ") or (counts and counts[1] == "0" and int(counts[2])):
                raise SystemExit(f"start refused by {previous}: answer its asks in {previous / 'answers.md'} and run "
                                 f"`verdict {previous.name}` again, or delete the run folder if it was abandoned")
    loop_runs.sort(key=lambda item: item[1]["round"])
    ledger_file = ledger_home / f"{hashlib.sha256(parent_loop.encode()).hexdigest()}.json" if not is_patch else None
    ledger = {
        "used": max((metadata["round"] for _, metadata in loop_runs), default=0),
        "continued": any(metadata.get("continued") for _, metadata in loop_runs),
        "runs": [str(folder) for folder, _ in loop_runs],
    }
    if ledger_file:
        try:
            shared = json.loads(ledger_file.read_text(encoding="utf-8"))
            valid = (isinstance(shared, dict) and type(shared.get("used")) is int and shared["used"] >= 0
                     and isinstance(shared.get("continued"), bool) and isinstance(shared.get("runs"), list)
                     and all(isinstance(run, str) for run in shared["runs"]))
        except FileNotFoundError:
            shared, valid = None, True
        except (OSError, ValueError, UnicodeError):
            valid = False
        if not valid:
            raise SystemExit(f"cannot read loop ledger {ledger_file}; fix or delete it from a session that can "
                         "write the git dir, then start again") from None
        if shared is not None:
            ledger = {"used": max(shared["used"], ledger["used"]),
                      "continued": shared["continued"] or ledger["continued"],
                      "runs": list(dict.fromkeys([*shared["runs"], *ledger["runs"]]))}
        if ledger_writable and loop_runs:
            step_run.write_step(ledger_file, json.dumps(ledger, indent=2) + "\n")
    used, continued = ledger["used"], ledger["continued"]
    snapshots = []
    for folder in map(Path, ledger["runs"][-2:]):
        try:
            snapshot = json.loads((folder / "snapshot.json").read_text(encoding="utf-8"))
            valid = (isinstance(snapshot, dict) and isinstance(snapshot.get("verdict"), str)
                     and all(isinstance(snapshot.get(key), list) for key in ("open", "checks"))
                     and all(isinstance(snapshot.get(key), bool) for key in ("certain", "complete")))
            if not valid:
                snapshot = None
        except (OSError, ValueError, UnicodeError):
            snapshot = None
        if snapshot is not None:
            verdict_file = folder / "03-verdict.md"
            if (not verdict_file.exists() or revision(verdict_file) != snapshot.pop("verdict")
                    or stale_verdict(folder, verdict_file.read_text(encoding="utf-8").splitlines())):
                snapshot = None
        snapshots.append(snapshot)
    plateau = (len(snapshots) == 2 and all(s and s["certain"] and s["complete"] and s["open"] for s in snapshots)
               and snapshots[0] == snapshots[1])
    reason = "cap" if used >= ROUND_CAP else "no progress" if plateau else None
    extension = False
    if reason:
        if "--continue" in options and not continued:
            try:
                extension = f"continue: {parent_loop}" in Path(options["--continue"]).read_text(encoding="utf-8").splitlines()
            except (OSError, UnicodeError) as error:
                raise SystemExit(f"cannot read --continue file: {error}") from None
        if not extension:
            if continued:
                raise SystemExit(f"start refused by {reason}; {used} rounds used for {parent_loop}; "
                                 f"the extension is used (`continue: {parent_loop}`); bring the open rows to the user")
            raise SystemExit(f"start refused by {reason}; {used} rounds used for {parent_loop}; "
                             f"the user can write `continue: {parent_loop}` in a file and pass --continue FILE "
                             "for one more round, once")
    numbers = (p.name[len(prefix) + 1:] for p in d.glob(f"{prefix}-[0-9]*") if p.is_dir())
    n = max((int(number) for number in numbers if number.isdecimal()), default=0) + 1
    d = d / f"{prefix}-{n:02d}"
    (d / "head").mkdir(parents=True)
    if prior and (prior[0][0] / "answers.md").exists():
        shutil.copyfile(prior[0][0] / "answers.md", d / "answers-before.md")
    (d / "diff.patch").write_text(patch, encoding="utf-8")
    units = []
    for path, info in parse_diff(patch).items():
        src = d / "head" / path  # seats read full functions here, never from a moving checkout
        if not info["deleted"]:
            src.parent.mkdir(parents=True, exist_ok=True)
            src.write_text(Path(path).read_text(encoding="utf-8") if head is None else git("show", f"{head}:{path}"),
                           encoding="utf-8")
        units += units_of(path, info, src)
    if not units:
        raise SystemExit("the diff has no changed lines to review")
    for i, u in enumerate(units, 1):
        u["id"] = f"u{i}"
    (d / "units.json").write_text(json.dumps(units, indent=2) + "\n", encoding="utf-8")

    rules = Path.home() / ".review-check" / "rules" / f"{repo_name()}.md"
    catalog = load_rules(sorted((Path(__file__).resolve().parent.parent / "references").glob("lens*.md"))
                         + ([rules] if rules.exists() else []), repo_file=rules)
    ci = ci_of(rules)
    cfg = lint_config(ci, head)
    (d / "target.json").write_text(json.dumps({"target": target, "base": base, "head": head, "ci": ci,
                                             "patch": is_patch, "parent_loop": parent_loop,
                                             "round": used + 1, "continued": extension}, indent=2) + "\n", encoding="utf-8")
    if ledger_file and ledger_writable:
        ledger.update(used=used + 1, continued=continued or extension, runs=[*ledger["runs"], str(d)])
        step_run.write_step(ledger_file, json.dumps(ledger, indent=2) + "\n")
    diff = parse_diff(patch)
    checklist = {}
    for u in units:
        info = diff[u["file"]]
        src = d / "head" / u["file"]
        body = src.read_text(encoding="utf-8", errors="replace").splitlines()[max(u["range"][0] - 1, 0):u["range"][1]] if src.exists() else []
        text = "\n".join(body + info["removed"])  # removed code counts, so a rule about what was dropped still fires
        added = [info["new"][n] for n in u["lines"]]
        matched = [r for r in catalog if glob_match(u["file"], r["files"]) and (r["applies"] is None or r["applies"].search(
            "\n".join(added) if r["scope"] == "added" else text))]
        tool = [r["id"] for r in matched if r["check"] and all(tool_owned(cfg, c, u["file"]) for c in r["check"])]
        ids = [r["id"] for r in matched if r["id"] not in tool]
        symbols = {m.group(1) for line in added + info["removed"] for m in [DEFINES.search(line)] if m}
        if not u["symbol"].startswith(("hunk ", "deleted ")):
            symbols.add(u["symbol"])
        own = {f"{u['file']}:{n}" for n in range(u["range"][0], u["range"][1] + 1)}
        refs = references(symbols, head, own, family(u["file"]))
        if refs:
            ids.append("REF")
        checklist[u["id"]] = {"rules": ids, "tool": tool, "refs": refs}
    seats = seat_map(checklist, catalog)
    (d / "checklist.json").write_text(json.dumps(checklist | {"seats": seats}, indent=2) + "\n", encoding="utf-8")
    owned = sum(len(c["tool"]) for c in checklist.values())
    body = [f"Target: {target}", f"Base: {base}", f"Head: {head or 'working tree'}",
            f"Rules: {rules if rules.exists() else 'none'}", "",
            f"Catalog: {len(catalog)} rules from lens*.md and the rules file.",
            f"{len(units)} units in {len({u['file'] for u in units})} files, "
            f"{sum(len(c['rules']) for c in checklist.values())} checks. `checklist.json` lists each unit's rule IDs "
            "and the references that `REF` must check.",
            f"Build: `{ci['name']}` must pass at this head, and it owns {owned} more checks (`tool` in checklist.json); "
            "run `build` before `verdict`." if ci else "Build: none, because the rules file has no `CI:` line.", "",
            "| Unit | File | Symbol | Range | Added lines | Checks |", "|---|---|---|---|---|---|",
            *(f"| {u['id']} | `{u['file']}` | `{u['symbol']}` | {u['range'][0]}-{u['range'][1]} | {len(u['lines'])} "
              f"| {len(checklist[u['id']]['rules'])} |" for u in units)]
    rest = "Uses:\n" + "\n".join(body) + "\n"
    (d / "01-units.md").write_text(f"Status: done {hashlib.sha1(rest.encode()).hexdigest()[:12]}\n{rest}", encoding="utf-8")
    (d / "02-review.md").write_text(
        f"Status: open\nUses: 01-units@{revision(d / '01-units.md')}\n\n"
        "| Unit | Rule | file:line | Quote | Kind | Problem | Proof |\n|---|---|---|---|---|---|---|\n", encoding="utf-8")
    print(d)
    note = " (local writes; the git dir is not writable)" if not ledger_writable else ""
    print(f"parent loop: {parent_loop}{note}")
    for seat, plan in seats.items():
        print(f"{seat}: {len(plan['units'])} units, {plan['checks']} checks")

