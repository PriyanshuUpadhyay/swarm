# Git snapshots, changed units, and symbol references.
import hashlib, json, re, subprocess
from pathlib import Path


def git(*args, check=True):
    r = subprocess.run(["git", *args], capture_output=True, text=True, encoding="utf-8", errors="replace")
    if check and r.returncode != 0:
        raise SystemExit(f"git {' '.join(args[:3])}: {r.stderr.strip()}")
    return r.stdout



def repo_name():
    from review_check import git
    return Path(git("rev-parse", "--git-common-dir").strip()).resolve().parent.name



def home():
    from review_check import git
    root = Path(git("rev-parse", "--show-toplevel").strip())
    exclude = Path(git("rev-parse", "--git-common-dir").strip()).resolve() / "info" / "exclude"
    try:  # a sandbox may deny writes to the git dir; excluding tmp is optional
        lines = exclude.read_text(encoding="utf-8").splitlines() if exclude.exists() else []
        if "/tmp/" not in lines:  # local to this clone, so the team never sees it
            exclude.parent.mkdir(parents=True, exist_ok=True)
            exclude.write_text("\n".join(lines + ["/tmp/"]) + "\n", encoding="utf-8")
    except OSError:
        pass
    return root / "tmp" / "review-check"



def family(path):
    from review_check import FAMILY
    return FAMILY.get(Path(path).suffix.lstrip("."), Path(path).suffix)



def references(symbols, head, own, lang):
    """{symbol: ["file:line", ...]} for each symbol that another place in the tree names."""
    from review_check import REFS_CAP, family, git
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
    from review_check import PLAIN, git
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
    from review_check import FUNC_KINDS, brace_end
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
    from review_check import functions
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



def tree_digest():
    from review_check import local_patch
    return hashlib.sha1(local_patch().encode()).hexdigest()[:12]



def stale_verdict(folder, lines):
    from review_check import revision
    for name, stamp in re.findall(r"([\w.-]+)@([a-f0-9]+)", lines[1] if len(lines) > 1 else ""):
        file = folder / f"{name}.{'json' if name == 'build' else 'md'}"
        if not file.exists() or revision(file) != stamp:
            return file.name
    return None

