#!/usr/bin/env python3
"""Deterministic parts of a review-check run. The script owns the units and the verdict; the reviewer
only writes rows into 02-review.md.

Every rule in references/lens*.md and in the repo's rules file has an ID, a `Files:` glob, and an
optional `Applies:` regex. `start` writes checklist.json: for each unit, the ID of every rule whose
glob matches the unit's file and whose regex matches the unit's code, plus `REF` when a symbol the
unit defines or removes has references elsewhere. `verdict` refuses until each ID has a result.

A rule with `Check: <tool>:<rule>` is owned by the build gate, not a seat, when the repo rules file
has a `CI:` line and the repo's lint config runs that rule at error level, with default options, on
the unit's file. `build` records the CI result for the run's head, and `verdict` counts those IDs
only from it.

  start <local|base..head> [--patch FILE]   make <repo>/tmp/review-check/<run>/ with 01-units.md
  build <run> [--run]                        record CI for the head, or run its command in the verify worktree
  verdict <run>                              gate the rows of 02-review*.md and write 03-verdict.md

`--patch` reviews that patch instead of the whole range, for example review-walk's diff since view.
`verdict` exits 1 when the gate fails, so a caller cannot read an APPROVE that no one earned.
"""
import fnmatch, hashlib, json, os, re, shutil, signal, subprocess, sys, urllib.parse
from contextlib import contextmanager, nullcontext
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3] / "references"))
import step_run  # noqa: E402

KINDS = ("pass", "fix", "ask", "note", "n/a")
RULE = re.compile(r"^- `([A-Z][A-Z0-9-]*-\d+)` (.*)$")
REFS_CAP = 20  # a symbol with more references than this gets the first 20; the seat searches the rest
BUILD_TIMEOUT = 3600
CLEANUP_SECONDS = 10
FUNC_KINDS = {"function", "method", "subroutine", "func", "procedure"}
PLAIN = ("--no-ext-diff", "--no-color", "--src-prefix=a/", "--dst-prefix=b/")  # user config may change these
GENERIC = {"", "n/a", "na", "none", "ok", "okay", "fine", "good", "looks fine", "looks good", "checked",
           "verified", "tested", "no issues", "lgtm", "-"}


def git(*args, check=True):
    r = subprocess.run(["git", *args], capture_output=True, text=True, encoding="utf-8", errors="replace")
    if check and r.returncode != 0:
        raise SystemExit(f"git {' '.join(args[:3])}: {r.stderr.strip()}")
    return r.stdout


def repo_name():
    return Path(git("rev-parse", "--git-common-dir").strip()).resolve().parent.name


def home():
    root = Path(git("rev-parse", "--show-toplevel").strip())
    exclude = Path(git("rev-parse", "--git-common-dir").strip()).resolve() / "info" / "exclude"
    lines = exclude.read_text(encoding="utf-8").splitlines() if exclude.exists() else []
    if "/tmp/" not in lines:  # local to this clone, so the team never sees it
        exclude.parent.mkdir(parents=True, exist_ok=True)
        exclude.write_text("\n".join(lines + ["/tmp/"]) + "\n", encoding="utf-8")
    return root / "tmp" / "review-check"


def expand(pattern):
    """`**/*.{ts,py}` -> ["**/*.ts", "**/*.py"]."""
    m = re.search(r"\{([^{}]*)\}", pattern)
    if not m:
        return [pattern]
    return [x for alt in m.group(1).split(",") for x in expand(pattern[:m.start()] + alt + pattern[m.end():])]


def glob_match(path, globs):
    for g in (x for pattern in globs for x in expand(pattern)):
        # fnmatch's * also crosses "/", so "**/" only needs to allow a file at the repo root
        if fnmatch.fnmatch(path, g) or (g.startswith("**/") and fnmatch.fnmatch(path, g[3:])):
            return True
    return False


def globs_of(text):
    return [g.strip() for g in re.split(r",(?![^{]*\})", text)]  # a comma inside {a,b} is not a separator


def load_rules(files, repo_file=None):
    """[{id, files, applies, source, aspect}] from rule lines, with the repo file identified by its caller."""
    rules = []
    for f in files:
        aspect = "repo" if f == repo_file else "lens" if f.name == "lens.md" else "lang"
        default = ["**/*"]
        for line in f.read_text(encoding="utf-8").splitlines():
            head = re.match(r"^Files: `([^`]+)`", line)
            if head:
                default = globs_of(head.group(1))
            m = RULE.match(line)
            if not m:
                continue
            globs = re.search(r"Files: `([^`]+)`", m.group(2))
            applies = re.search(r"Applies: `([^`]+)`", m.group(2))
            check = re.search(r"Check: `([^`]+)`", m.group(2))
            scope = re.search(r"Scope: (\w+)", m.group(2))
            rules.append({"id": m.group(1), "source": f.name, "aspect": aspect,
                          "files": globs_of(globs.group(1)) if globs else default,
                          "applies": re.compile(applies.group(1)) if applies else None,
                          "check": [c.strip() for c in check.group(1).split(",")] if check else [],
                          "scope": scope.group(1) if scope else None})
    return rules


def ci_of(rules_file):
    """{name, cmd, config} from the line ``CI: `<check name>` runs `<command>`; config `<file>` ``."""
    m = re.search(r"^CI: `([^`]+)` runs `([^`]+)`(?:; config `([^`]+)`)?", rules_file.read_text(encoding="utf-8"), re.M) \
        if rules_file.exists() else None
    return {"name": m.group(1), "cmd": m.group(2), "config": m.group(3)} if m else None


def lint_config(ci, head):
    if not ci or not ci["config"]:
        return None
    text = git("show", f"{head}:{ci['config']}", check=False) if head else \
        (Path(ci["config"]).read_text(encoding="utf-8") if Path(ci["config"]).exists() else "")
    try:
        return json.loads(text)
    except ValueError:  # a JSONC config with comments leaves every rule with the seats
        return None


def lint_name(name):
    return re.sub(r"^eslint/", "", re.sub(r"^@?typescript-eslint/", "typescript/", name))


def tool_owned(cfg, check, path):
    """True when the repo's oxlint config runs `check` on `path` at error level with default options.
    The fixtures run with default options, so a rule with options stays with the seats."""
    tool, _, rule = check.partition(":")
    if tool != "oxlint" or cfg is None:
        return False
    if rule.startswith("typescript/") and Path(path).suffix not in (".ts", ".tsx", ".mts", ".cts"):
        return False  # type-aware rules need type data, which a JS file may not have
    ignores = [g for p in cfg.get("ignorePatterns", []) for g in (p, p.rstrip("/") + "/**")]
    if glob_match(path, ignores):
        return False
    level = None
    for block in [cfg.get("rules", {})] + [o.get("rules", {}) for o in cfg.get("overrides", [])
                                           if glob_match(path, o.get("files", []))]:
        level = next((v for k, v in block.items() if lint_name(k) == lint_name(rule)), level)
    return level in ("error", "deny", 2)


# Names other files can reach: exported bindings, top-level functions and types, and class members with
# a modifier. A plain `const x` inside a function is local, so it gets no REF.
DEFINES = re.compile(r"(?:\bexport\s+(?:default\s+)?(?:async\s+)?(?:const|let|var|function|class|type|interface|enum)\s+"
                     r"|^(?:async\s+)?(?:def|function|func|class|struct|enum|protocol|interface|type)\s+"
                     r"|^\s+(?:def|func)\s+"
                     r"|^\s*(?:(?:public|private|protected|static|async|override|readonly)\s+)+)([A-Za-z_]\w{2,})")


FAMILY = {ext: fam for fam, exts in {
    "js": "ts tsx js jsx mjs cjs mts cts vue svelte", "py": "py pyi", "go": "go", "swift": "swift m h",
    "rs": "rs", "jvm": "java kt", "rb": "rb", "sql": "sql"}.items() for ext in exts.split()}


def family(path):
    return FAMILY.get(Path(path).suffix.lstrip("."), Path(path).suffix)


def references(symbols, head, own, lang):
    """{symbol: ["file:line", ...]} for each symbol that another place in the tree names."""
    out = {}
    for sym in sorted(symbols):
        found = git("grep", "-n", "-w", "-I", "-e", sym, *([head] if head else []), check=False).splitlines()
        hits = [":".join(h.removeprefix(f"{head}:").split(":")[:2]) for h in found]
        # the same name in another language is another symbol, for example Python `encode` and JS `.encode`
        hits = [h for h in hits if h not in own and family(h.split(":")[0]) == lang]
        if hits:
            out[sym] = hits[:REFS_CAP]
    return out


def revision(path):
    lines = path.read_text(encoding="utf-8").splitlines(True)
    return hashlib.sha1("".join(lines if path.name == "answers.md" else lines[1:]).encode()).hexdigest()[:12]


def local_patch():
    patch = git("diff", *PLAIN, "HEAD")
    for path in git("ls-files", "--others", "--exclude-standard", "-z").split("\0"):
        if path:  # exit code 1 means "files differ", which is the normal case here
            patch += git("diff", *PLAIN, "--no-index", "--", "/dev/null", path, check=False)
    return patch


def parse_diff(patch):
    """{path: {"new": {line: text}, "added": set, "removed": [text], "hunks": [(start, end, removes)]}}.
    A deleted file is keyed by its old path and has no new side."""
    files, cur, old, n = {}, None, None, 0
    for line in patch.splitlines():
        if line.startswith("--- "):
            old = line[4:].split("\t")[0].removeprefix("a/")
        elif line.startswith("+++ "):
            p = line[4:].split("\t")[0]
            cur = old if p == "/dev/null" else p.removeprefix("b/")
            files[cur] = {"new": {}, "added": set(), "removed": [], "hunks": [], "deleted": p == "/dev/null"}
        elif line.startswith("@@") and cur:
            m = re.match(r"@@ -\d+(?:,\d+)? \+(\d+)(?:,(\d+))? @@", line)
            n = int(m.group(1))
            files[cur]["hunks"].append([n, n + max(int(m.group(2) or 1), 1) - 1, False])
        elif cur and files[cur]["hunks"] and line[:1] in ("+", " "):
            files[cur]["new"][n] = line[1:]
            if line[0] == "+":
                files[cur]["added"].add(n)
            n += 1
        elif cur and files[cur]["hunks"] and line[:1] == "-":
            files[cur]["removed"].append(line[1:])
            files[cur]["hunks"][-1][2] = True
    return files


def functions(path):
    """(name, start, end) from universal-ctags, or [] when it cannot read the file's language."""
    try:
        out = subprocess.run(["ctags", "--output-format=json", "--fields=+ne", "-f", "-", str(path)],
                             capture_output=True, text=True, timeout=30).stdout
    except (OSError, subprocess.TimeoutExpired):
        return []
    tags = [json.loads(l) for l in out.splitlines() if l.startswith("{")]
    lines = Path(path).read_text(errors="replace").splitlines()
    return [(t["name"], t["line"], t.get("end") or brace_end(lines, t["line"])) for t in tags
            if t.get("_type") == "tag" and t.get("kind") in FUNC_KINDS and t.get("line")]


def brace_end(lines, start):
    """The line that closes the first `{` at or after `start`, for a parser that gives no end line
    (universal-ctags 6.2 has none for Rust). With no brace, the function is its first line."""
    # ponytail: counts braces, so a lone `{` inside a string or comment can shift the end; a real
    # parser per language if that shows up in a review.
    depth = 0
    for n, line in enumerate(lines[start - 1:], start):
        for ch in line:
            depth += (ch == "{") - (ch == "}")
            if ch == "}" and depth == 0:
                return n
    return start


def units_of(path, info, source):
    """Every added line lands in exactly one unit: the innermost function around it, else its hunk.
    A hunk that only removes lines, and a deleted file, are units too, because removed code can
    drop a guarantee."""
    if info["deleted"]:
        return [{"file": path, "symbol": "deleted file", "range": [0, 0], "lines": []}]
    units, left = [], set(info["added"])
    for name, s, e in sorted(functions(source), key=lambda f: f[2] - f[1]):
        lines = sorted(l for l in left if s <= l <= e)
        if lines:
            units.append({"file": path, "symbol": name, "range": [s, e], "lines": lines})
            left -= set(lines)
    for s, e, removes in info["hunks"]:
        lines = sorted(l for l in left if s <= l <= e)
        if lines or (removes and not any(s <= l <= e for l in info["added"])):
            units.append({"file": path, "symbol": f"hunk {s}-{e}", "range": [s, e], "lines": lines})
            left -= set(lines)
    return units


def seat_map(checklist, catalog):
    """Assign whole units to contiguous shards, minimizing the largest check count."""
    aspects = {r["id"]: r["aspect"] for r in catalog}
    aspects["REF"] = "refs"
    seats = {}
    for aspect in ("lens", "lang", "repo", "refs"):
        work = [(uid, [r for r in c["rules"] if aspects[r] == aspect]) for uid, c in checklist.items()]
        work = [(uid, ids) for uid, ids in work if ids]
        if not work:
            continue
        total = sum(len(ids) for _, ids in work)
        count = 1 if aspect == "refs" else min(3, (total + 149) // 150, len(work))
        low, high = max(len(ids) for _, ids in work), total
        while low < high:
            limit, groups, size = (low + high) // 2, 1, 0
            for _, ids in work:
                if size + len(ids) > limit:
                    groups, size = groups + 1, 0
                size += len(ids)
            if groups <= count:
                high = limit
            else:
                low = limit + 1
        shards, end, size = [], len(work), 0
        for i in range(len(work) - 1, -1, -1):
            if size + len(work[i][1]) > low or i + 1 < count - len(shards):
                shards.append(work[i + 1:end])
                end, size = i + 1, 0
            size += len(work[i][1])
        shards.append(work[:end])
        for n, shard in enumerate(reversed(shards), 1):
            seat = aspect if count == 1 else f"{aspect}-{n}"
            seats[seat] = {"units": [uid for uid, _ in shard],
                           "checks": sum(len(ids) for _, ids in shard),
                           "route": "review.deep" if aspect == "refs" else "review.check"}
            for uid, ids in shard:
                checklist[uid].setdefault("owners", {}).update({r: seat for r in ids})
    return seats


def start(target, *opts):
    if target == "local":
        prefix, base, head = "local", git("rev-parse", "HEAD").strip(), None
        patch = local_patch()
    elif ".." in target:
        base, head = (git("rev-parse", "--verify", f"{p}^{{commit}}").strip() for p in target.split("..", 1))
        prefix = f"range-{base[:7]}-{head[:7]}"
        patch = git("diff", *PLAIN, base, head)
    else:
        raise SystemExit("target is `local` or `<base>..<head>`")
    is_patch = opts[:1] == ("--patch",)
    if is_patch:
        if len(opts) < 2:
            raise SystemExit("--patch needs a file")
        try:
            patch = Path(opts[1]).read_text(encoding="utf-8")
        except OSError as error:
            raise SystemExit(f"cannot read patch {opts[1]}: {error.strerror}") from None

    d = home()
    prior = []
    if not is_patch:
        for target_file in d.glob("*/target.json"):
            metadata = json.loads(target_file.read_text(encoding="utf-8"))
            if metadata.get("patch"):
                continue
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
            for name, stamp in re.findall(r"([\w.-]+)@([a-f0-9]+)", lines[1] if len(lines) > 1 else ""):
                file = previous / f"{name}.{'json' if name == 'build' else 'md'}"
                if not file.exists() or revision(file) != stamp:
                    raise SystemExit(f"start refused by {previous}: its verdict is stale ({file.name} changed); "
                                     f"run `verdict {previous.name}` again")
            counts = re.search(r"; (\d+) fix, (\d+) ask", lines[2]) if len(lines) > 2 else None
            if not lines or not lines[0].startswith("Status: done ") or (counts and counts[1] == "0" and int(counts[2])):
                raise SystemExit(f"start refused by {previous}: answer its asks in {previous / 'answers.md'} and run "
                                 f"`verdict {previous.name}` again, or delete the run folder if it was abandoned")
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
                                             "patch": is_patch}, indent=2) + "\n", encoding="utf-8")
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
    for seat, plan in seats.items():
        print(f"{seat}: {len(plan['units'])} units, {plan['checks']} checks")


def tree_digest():
    return hashlib.sha1(local_patch().encode()).hexdigest()[:12]


def verify_tree(head):
    """Keep range builds apart from the live checkout, at the reviewed commit."""
    verify = home() / "verify"
    if not verify.exists():
        git("worktree", "add", "--force", "--detach", str(verify), head)
    else:
        root = git("-C", str(verify), "rev-parse", "--show-toplevel", check=False).strip()
        if not root or Path(root).resolve() != verify.resolve():
            raise SystemExit(f"verify path is not its own git worktree: {verify}")
        git("-C", str(verify), "reset", "-q", "--hard")
        git("-C", str(verify), "clean", "-qfd")
        git("-C", str(verify), "checkout", "--detach", head)
    return verify


@contextmanager
def build_signals():
    def stop(signum, frame):
        raise SystemExit(128 + signum)

    previous = {signum: signal.getsignal(signum) for signum in (signal.SIGTERM, signal.SIGHUP)}
    try:
        for signum, handler in previous.items():
            # A signal ignored at entry stays ignored, so a nohup build survives a hangup.
            if handler != signal.SIG_IGN:
                signal.signal(signum, stop)
        yield
    finally:
        for signum, handler in previous.items():
            signal.signal(signum, handler)


def build(name, *opts):
    """Write build.json from CI or a command on the reviewed commit's verify worktree."""
    if opts not in ((), ("--run",)):
        raise SystemExit("build accepts only --run; --force cannot override a running build")
    d = home() / name
    t = json.loads((d / "target.json").read_text(encoding="utf-8"))
    ci, head = t["ci"], t["head"]
    if not ci:
        raise SystemExit("the repo rules file has no `CI:` line, so this run has no build gate")
    cwd = Path.cwd()
    if opts[:1] == ("--run",):
        if not head:
            print("build: local target; proof is from the live tree")
        with (step_run.step_lock(home(), "verify", command="build --run") if head else nullcontext()), build_signals():
            if head:
                cwd = verify_tree(head)
            process = None
            try:
                try:
                    # Defer launch signals until process is set. The child inherits this blocked mask across exec,
                    # so preexec_fn resets it before exec; finally restores only the parent's mask.
                    previous_mask = signal.pthread_sigmask(signal.SIG_BLOCK, (signal.SIGTERM, signal.SIGHUP, signal.SIGINT))
                    try:
                        process = subprocess.Popen(["bash", "-c", ci["cmd"]], cwd=cwd, stdout=subprocess.PIPE,
                                                   stderr=subprocess.PIPE, text=True, start_new_session=True,
                                                   preexec_fn=lambda: signal.pthread_sigmask(signal.SIG_SETMASK, previous_mask))
                    finally:
                        signal.pthread_sigmask(signal.SIG_SETMASK, previous_mask)
                    stdout, stderr = process.communicate(timeout=BUILD_TIMEOUT)
                    conclusion, tail = ("success" if process.returncode == 0 else "failure"), (stdout + stderr).splitlines()[-40:]
                except (subprocess.TimeoutExpired, KeyboardInterrupt, SystemExit) as error:
                    if process is None:
                        raise
                    try:
                        os.killpg(process.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                    try:
                        process.communicate(timeout=CLEANUP_SECONDS)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        # A child in another session may still hold the output pipes open.
                        process.stdout.close()
                        process.stderr.close()
                        try:
                            process.wait(timeout=CLEANUP_SECONDS)
                        except subprocess.TimeoutExpired:
                            pass
                    if not isinstance(error, subprocess.TimeoutExpired):
                        raise
                    conclusion, tail = "failure", [f"timed out after {BUILD_TIMEOUT} s"]
            finally:
                if process is not None:
                    if not process.stdout.closed:
                        process.stdout.close()
                    if not process.stderr.closed:
                        process.stderr.close()
        result = {"source": "command", "name": ci["cmd"], "conclusion": conclusion, "tail": tail}
    elif not head:
        raise SystemExit("uncommitted changes have no CI result; use `build <run> --run`")
    else:
        q = urllib.parse.quote(ci["name"])
        try:
            out = subprocess.run(["gh", "api", f"repos/{{owner}}/{{repo}}/commits/{head}/check-runs?check_name={q}"],
                                 cwd=cwd, capture_output=True, text=True, timeout=BUILD_TIMEOUT)
        except subprocess.TimeoutExpired:
            raise SystemExit(f"gh: timed out after {BUILD_TIMEOUT} s") from None
        except (FileNotFoundError, NotADirectoryError):
            raise SystemExit("gh: not found") from None
        except OSError as error:
            raise SystemExit(f"gh: {error.strerror or error}") from None
        if out.returncode != 0:
            raise SystemExit(f"gh: {out.stderr.strip()}")
        # skipped, cancelled, or running checks prove nothing; a re-run of the same head gets a higher id
        found = sorted((c for c in json.loads(out.stdout)["check_runs"] if c["status"] == "completed"
                        and c["conclusion"] in ("success", "failure", "timed_out")), key=lambda c: c["id"])
        if not found:
            raise SystemExit(f"no finished `{ci['name']}` check for {head[:7]} (not pushed, a draft PR, or still "
                             "running); wait, or use `build <run> --run`")
        last = found[-1]
        result = {"source": "check-run", "name": ci["name"],
                  "conclusion": "success" if last["conclusion"] == "success" else "failure", "tail": [last["html_url"]]}
    result |= {"head": head, "digest": None if head else tree_digest()}
    (d / "build.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(f"build: {result['conclusion']} ({result['source']} `{result['name']}`)")


def review_files(d, seats):
    if not seats:
        return sorted(d.glob("02-review*.md"))
    files = [d / "02-review.md", *(d / f"02-review-{seat}.md" for seat in seats)]
    return [f for f in files if f.exists()]


def rows(d, seats=None):
    out = []
    for f in review_files(d, seats or {}):
        if not f.exists():
            continue
        for line in f.read_text(encoding="utf-8").split("\n"):
            if re.match(r"\|\s*u\d+\s*\|", line):
                cells = [c.strip().replace("\\|", "|") for c in re.split(r"(?<!\\)\|", line)[1:-1]]
                out.append((f.name, cells))
    return out


def answers(path):
    out = []
    for line in path.read_text(encoding="utf-8").split("\n") if path.exists() else []:
        if m := re.match(r"^\s*- (.+?) `(.*)$", line):
            file, text = m.groups()
            alternatives = [(file, text[:split.start()], text[split.end():])
                            for split in re.finditer(r"`: ?", text) if split.start()]
            if alternatives:
                out.append(alternatives)
    return out


def verdict(name):
    d = home() / name
    units = {u["id"]: u for u in json.loads((d / "units.json").read_text(encoding="utf-8"))}
    diff = parse_diff((d / "diff.patch").read_text(encoding="utf-8"))
    rules = re.search(r"^Rules: (.*)$", (d / "01-units.md").read_text(encoding="utf-8"), re.M).group(1)
    checklist = json.loads((d / "checklist.json").read_text(encoding="utf-8"))
    seats = checklist.pop("seats", {})
    t = json.loads((d / "target.json").read_text(encoding="utf-8")) if (d / "target.json").exists() else {"ci": None}
    problems, seen, kept, done = [], set(), [], {uid: set() for uid in units}
    replied = answers(d / "answers.md")
    past, history = answers(d / "answers-before.md"), []
    closed = [0] * len(replied)
    selected = [alternatives[0] for alternatives in replied]
    built = "none"
    if t["ci"]:  # the build must pass even with no tool IDs, because a missing import breaks no rule ID
        b = json.loads((d / "build.json").read_text(encoding="utf-8")) if (d / "build.json").exists() else None
        if b is None:
            problems.append("build: no result; run `build <run>`, or `build <run> --run` for an unpushed head")
        elif b["head"] != t["head"] or (t["head"] is None and b["digest"] != tree_digest()):
            problems.append("build: the result is for other code; run `build` again")
        else:
            built = "pass" if b["conclusion"] == "success" else "fail"
            for uid in units:
                done[uid] |= set(checklist[uid].get("tool", []))
            if built == "fail":
                kept.append(("build", f"build {b['name']}", "", "fix", f"build `{b['name']}` failed at "
                             f"{(t['head'] or 'working tree')[:7]}", b["tail"][-1] if b["tail"] else "no output"))
    for src, cells in rows(d, seats):
        if len(cells) != 7:
            problems.append(f"{src}: a row has {len(cells)} cells, not 7: {' | '.join(cells)}")
            continue
        uid, rule, loc, quote, kind, problem, proof = cells
        quote, kind = quote.strip("`"), kind.lower()
        ids = {r.strip().strip("`") for r in rule.split(",")} - {"-", ""}
        u = units.get(uid)
        if u is None or kind not in KINDS or not quote:
            problems.append(f"{uid}: unknown unit, kind not in {'/'.join(KINDS)}, or no quote")
            continue
        if kind in ("fix", "ask", "note") and len(ids) > 1:
            problems.append(f"{uid}: a {kind} row names one rule, or `-` for a finding that no rule owns")
            continue
        if ids & set(checklist[uid].get("tool", [])):
            problems.append(f"{uid}: {', '.join(sorted(ids & set(checklist[uid]['tool'])))} belongs to the build gate, not a seat")
            continue
        if seats and src == "02-review.md":
            owners = checklist[uid].get("owners", {})
            owned = {r for r in ids if r in owners}
            if owned:
                for seat in sorted({owners[r] for r in owned}):
                    names = ', '.join(sorted(r for r in owned if owners[r] == seat))
                    problems.append(f"{uid}: chair row names {names} owned by seat {seat}")
                continue
        elif seats:
            seat = src.removeprefix("02-review-").removesuffix(".md")
            wrong = {r for r in ids if checklist[uid].get("owners", {}).get(r) != seat}
            if wrong or uid not in seats[seat]["units"]:
                problems.append(f"seat {seat}: {uid}: row includes checks owned by another seat: {', '.join(sorted(wrong))}")
                continue
        if proof.lower().strip(". ") in GENERIC:
            problems.append(f"{uid} {rule}: a {kind} needs a proof that names the case, the input, or why the rule cannot apply")
            continue
        path = loc.split(":")[0].strip("`")
        if kind in ("pass", "n/a"):  # a quote from the unit, or from code the diff removed in that file
            src = d / "head" / u["file"]
            head = src.read_text(encoding="utf-8").splitlines() if src.exists() else []
            scope = head[max(u["range"][0] - 1, 0):u["range"][1]] + diff[u["file"]]["removed"]
        else:  # the quote must be at the line the row names, or in code the diff removed from that file
            at = re.search(r":(\d+)(?:-(\d+))?", loc)
            new = diff.get(path, {}).get("new", {})
            new_lines = [new[n] for n in range(int(at.group(1)), int(at.group(2) or at.group(1)) + 1) if n in new] if at else []
            matches = [(i, match) for i, alternatives in enumerate(replied)
                       if (match := next((alt for alt in alternatives if alt[0] == path
                                          and any(alt[1] == source.strip() for source in new_lines)
                                          and alt[2].strip().lower().startswith("limit:") == (kind == "fix")), None))] if kind in ("ask", "fix") else []
            scope = new_lines + diff.get(path, {}).get("removed", [])
        if not any(quote in line for line in scope):
            where = "in the unit" if kind in ("pass", "n/a") else f"at {loc.strip('`')} or in code removed from {path}"
            problems.append(f"{uid}: quote not found {where}: {quote!r}")
            continue
        seen.add(uid)
        done[uid] |= ids
        if kind in ("fix", "ask", "note"):
            problem = f"{rule}: {problem}" if ids else problem
            if kind == "ask" and not matches:
                for alternatives in past:
                    match = next((alt for alt in alternatives if alt[0] == path
                                  and any(alt[1] == source.strip() for source in new_lines)), None)
                    if match:
                        file, line, answer = match
                        history.append(f"History: {file} `{line}`: {answer}")
            if kind in ("ask", "fix") and matches:
                normalized = []
                for i, match in matches:
                    closed[i] += 1
                    selected[i] = match
                    text = match[2].strip()
                    normalized.append((text, text.lower().startswith("fix:")))
                answer, is_fix = next((item for item in normalized if item[1]), normalized[0])
                kind = "limit" if kind == "fix" else "fix" if is_fix else "answered"
                if kind == "fix":
                    problem = answer
                elif kind == "limit":
                    proof = answer[6:].strip()
                else:
                    proof = answer
            kept.append((uid, loc, quote, kind, problem, proof))
    for alternatives, match, n in zip(replied, selected, closed):
        path, quote, answer = match if n else min(alternatives, key=lambda alt: len(alt[2]))
        text = answer.strip()
        body = re.sub(r"^(?:fix|limit):\s*", "", text, flags=re.I)
        if body.lower().strip(". ") in GENERIC:
            problems.append(f"answers.md {path} `{quote}`: an answer needs a record the user approved or a failing test")
    missing = {uid: [r for r in checklist[uid]["rules"] if r not in done[uid]] for uid in units}
    if seats:
        for seat, plan in seats.items():
            for uid in plan["units"]:
                gaps = [r for r in missing[uid] if checklist[uid]["owners"][r] == seat]
                if gaps:
                    problems.append(f"seat {seat}: {uid}: no result for {', '.join(gaps)}")
    else:
        problems += [f"{uid}: no result for {', '.join(m)}" for uid, m in missing.items() if m]
    problems += [f"{uid}: no row ({u['file']} {u['symbol']})" for uid, u in units.items() if uid not in seen]
    checks = sum(len(c["rules"]) + len(c.get("tool", [])) for c in checklist.values())
    checked = sum(len(done[uid] & set(c["rules"] + c.get("tool", []))) for uid, c in checklist.items())

    count = {k: sum(1 for r in kept if r[3] == k) for k in ("fix", "ask", "note", "answered", "limit")}
    if problems:
        status, word = "blocked gate failed", "INCOMPLETE"
    else:
        status = "done"
        word = "REQUEST CHANGES" if count["fix"] else "NEEDS DISCUSSION" if count["ask"] else "APPROVE"
    line3 = (f"Verdict: {word} ({len(seen)} of {len(units)} units, {checked} of {checks} checks; {count['fix']} fix, {count['ask']} ask, "
             f"{count['note']} note, {count['answered']} answered, {count['limit']} limit; rules: {'none' if rules == 'none' else Path(rules).name}; build: {built})")
    uses = ", ".join(f"{f.stem}@{revision(f)}" for f in [d / "01-units.md", *sorted(d.glob("02-review*.md")),
                                                          *[f for f in [d / "build.json", d / "answers.md"] if f.exists()]])
    body = [line3, "", *(f"- {p}" for p in problems),
            *(f"- {k} `{loc}` {problem} ({'answer' if k == 'answered' else 'limit' if k == 'limit' else 'proof'}: {proof})"
              for _, loc, _, k, problem, proof in kept),
            *(f"Answered: {path} `{quote}` closes {n} {'fixes' if answer.strip().lower().startswith('limit:') else 'asks'}"
              for (path, quote, answer), n in zip(selected, closed)), *history]
    rest = f"Uses: {uses}\n" + "\n".join(body) + "\n"
    rev = f" {hashlib.sha1(rest.encode()).hexdigest()[:12]}" if status == "done" else ""
    (d / "03-verdict.md").write_text(f"Status: {status}{rev}\n{rest}", encoding="utf-8")
    print(*(re.sub(r"[\x00-\x08\x0b-\x1f\x7f-\x9f]", "", line) for line in body if line), sep="\n")
    return 1 if problems else 0


if __name__ == "__main__":
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(errors="backslashreplace")
    commands = {"start": start, "build": build, "verdict": verdict}
    if len(sys.argv) < 3 or sys.argv[1] not in commands:
        raise SystemExit(__doc__)
    sys.exit(commands[sys.argv[1]](*sys.argv[2:]))
