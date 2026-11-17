# Rule loading, tool ownership, and review seat assignment.
import fnmatch, json, re, tomllib
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



def load_rules(files, repo_files=()):
    """[{id, text, files, applies, source, aspect}] from rule lines; the owner's files, named by the caller, get the repo seat."""
    from review_check import RULE, globs_of
    rules = []
    for f in files:
        aspect = "repo" if f in repo_files else "lens" if f.name == "lens.md" else "lang"
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
            text = re.split(r"\s+(?:Files: `|Applies: `|Check: `|Scope: \w|Source: )", m.group(2))[0]
            rules.append({"id": m.group(1), "text": text, "source": f.name, "aspect": aspect,
                          "files": globs_of(globs.group(1)) if globs else default,
                          "applies": re.compile(applies.group(1)) if applies else None,
                          "check": [c.strip() for c in check.group(1).split(",")] if check else [],
                          "scope": scope.group(1) if scope else None})
    return rules



def ci_of(rules_file):
    """Read the CI command, lint config, and optional test-gate marker."""
    m = re.search(r"^CI: `([^`]+)` runs `([^`]+)`(?:; config `([^`]+)`)?(?P<tests>; tests)?", rules_file.read_text(encoding="utf-8"), re.M) \
        if rules_file.exists() else None
    return {"name": m.group(1), "cmd": m.group(2), "config": m.group(3), **({"tests": True} if m["tests"] else {})} if m else None



def lint_config(ci, head):
    """{tool: parsed config} for each file in the `config` list: a `.json` file is oxlint, a `.toml` file is ruff."""
    from review_check import git
    configs = {}
    for name in (n.strip() for n in ci["config"].split(",")) if ci and ci["config"] else []:
        text = git("show", f"{head}:{name}", check=False) if head else \
            (Path(name).read_text(encoding="utf-8") if Path(name).exists() else "")
        try:
            if name.endswith(".json"):
                configs["oxlint"] = json.loads(text)
            elif name.endswith(".toml"):
                data = tomllib.loads(text)
                configs["ruff"] = data.get("tool", {}).get("ruff") if Path(name).name == "pyproject.toml" else data
        except ValueError:  # a JSONC config with comments, or bad TOML, leaves that tool's rules with the seats
            pass
    return configs



def lint_name(name):
    return re.sub(r"^eslint/", "", re.sub(r"^@?typescript-eslint/", "typescript/", name))



def tool_owned(cfg, check, path):
    """True when the repo's lint config for the check's tool runs `check` on `path` at error level with default
    options. The fixtures run with default options, so a rule with options stays with the seats."""
    from review_check import glob_match, lint_name
    tool, _, rule = check.partition(":")
    if tool == "ruff" and cfg.get("ruff"):
        return ruff_owned(cfg["ruff"], rule, path)
    cfg = cfg.get("oxlint")
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



RUFF_DEFAULT = ["E4", "E7", "E9", "F"]  # ruff's own default `select`


def ruff_owned(cfg, code, path):
    """True when the ruff config selects `code` for `path`. Every doubt leaves the rule with the seats: an
    `extend` base we cannot read, an ignore that may cover the code, and a path glob that may cover the file."""
    from review_check import glob_match
    lint = {**cfg, **cfg.get("lint", {})}  # top-level keys are ruff's older spelling of the `lint` keys
    if cfg.get("extend") or Path(path).suffix not in (".py", ".pyi"):
        return False

    def near(globs):  # ruff reads a glob with no "/" against any folder, so match it there too
        return [x for g in globs for x in (g, f"**/{g}", g.rstrip("/") + "/**", f"**/{g.rstrip('/')}/**")]

    if glob_match(path, near(cfg.get("exclude", []) + cfg.get("extend-exclude", []))):
        return False
    letters, digits = re.fullmatch(r"([A-Z]+)(\d+)", code).groups()

    def selects(s):  # "SIM" and "SIM1" select SIM116; "PL" is a group we do not expand, so it does not
        m = re.fullmatch(r"([A-Z]+)(\d*)", s)
        return s == "ALL" or bool(m and m.group(1) == letters and digits.startswith(m.group(2)))

    ignores = lint.get("ignore", []) + lint.get("extend-ignore", [])
    for key in ("per-file-ignores", "extend-per-file-ignores"):
        ignores += [s for g, codes in lint.get(key, {}).items() if glob_match(path, near([g])) for s in codes]
    if any(s == "ALL" or code.startswith(s) for s in ignores):
        return False
    return any(selects(s) for s in lint.get("select", RUFF_DEFAULT) + lint.get("extend-select", []))



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

