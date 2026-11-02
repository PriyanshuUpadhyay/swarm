# Rule loading, tool ownership, and review seat assignment.
import fnmatch, json, re
from pathlib import Path


def expand(pattern):
    """`**/*.{ts,py}` -> ["**/*.ts", "**/*.py"]."""
    from review_check import expand
    m = re.search(r"\{([^{}]*)\}", pattern)
    if not m:
        return [pattern]
    return [x for alt in m.group(1).split(",") for x in expand(pattern[:m.start()] + alt + pattern[m.end():])]



def glob_match(path, globs):
    from review_check import expand
    for g in (x for pattern in globs for x in expand(pattern)):
        # fnmatch's * also crosses "/", so "**/" only needs to allow a file at the repo root
        if fnmatch.fnmatch(path, g) or (g.startswith("**/") and fnmatch.fnmatch(path, g[3:])):
            return True
    return False



def globs_of(text):
    return [g.strip() for g in re.split(r",(?![^{]*\})", text)]  # a comma inside {a,b} is not a separator



def load_rules(files, repo_file=None):
    """[{id, files, applies, source, aspect}] from rule lines, with the repo file identified by its caller."""
    from review_check import RULE, globs_of
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
    from review_check import git
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
    from review_check import glob_match, lint_name
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

