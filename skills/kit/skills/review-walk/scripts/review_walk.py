#!/usr/bin/env python3
"""Deterministic parts of a review-walk run: the run folder, the diffs, GitHub viewed marks, and
the pending review. The agent owns every judgment; this script owns every git and GitHub call.

  start <target>            make <repo>/tmp/review-walk/<run>/ and write 01-scope.md
  status [<run>]            print each step's status, readiness, and staleness
  viewed <run>              record GitHub viewed marks and pending comments for 05-walk
  remark <run>              mark files viewed on GitHub again when they did not change since view
  post <run> <comments.json>  add comments to your pending review; it never submits

<target> is a PR number or URL, `local` for uncommitted changes, or `<base>..<head>`.
"""
import hashlib, json, re, subprocess, sys
from pathlib import Path

SECRET = re.compile(r"AKIA[0-9A-Z]{16}|gh[pousr]_[A-Za-z0-9]{36,}|sk-[A-Za-z0-9_-]{20,}|xox[abposr]-[A-Za-z0-9-]{10,}"
                    r"|-----BEGIN [A-Z ]*PRIVATE KEY-----")

STEPS = {  # step -> steps it uses
    "01-scope": [],
    "02-map": ["01-scope"],
    "03-check": ["01-scope"],
    "04-comments": ["02-map", "03-check"],
    "05-walk": ["04-comments"],
}


def run(*cmd, cwd=None, check=True, input=None):
    r = subprocess.run(cmd, cwd=cwd, input=input, capture_output=True, text=True)
    if check and r.returncode != 0:
        raise SystemExit(f"{' '.join(cmd[:4])}: {r.stderr.strip() or r.stdout.strip()}")
    return r.stdout


def git(*args, check=True):
    return run("git", *args, check=check)


def gql(query, **variables):
    payload = json.dumps({"query": query, "variables": variables})
    out = json.loads(run("gh", "api", "graphql", "--input", "-", input=payload))
    if out.get("errors"):
        raise SystemExit(f"GitHub: {out['errors'][0]['message']}")
    return out["data"]


def home():
    root = Path(git("rev-parse", "--show-toplevel").strip())
    exclude = Path(git("rev-parse", "--git-common-dir").strip()).resolve() / "info" / "exclude"
    lines = exclude.read_text().splitlines() if exclude.exists() else []
    if "/tmp/" not in lines:  # local to this clone, so the team never sees it
        exclude.parent.mkdir(parents=True, exist_ok=True)
        exclude.write_text("\n".join(lines + ["/tmp/"]) + "\n")
    return root / "tmp" / "review-walk"


def revision(path):
    return hashlib.sha1("".join(path.read_text().splitlines(True)[1:]).encode()).hexdigest()[:12]


def resolve(target):
    """Return (run prefix, target record). Fetches only objects and FETCH_HEAD, never a branch."""
    if target == "local":
        head = git("rev-parse", "HEAD").strip()
        return "local", {"kind": "local", "base": head, "head": None}
    if ".." in target:
        base, head = (git("rev-parse", "--verify", f"{p}^{{commit}}").strip() for p in target.split("..", 1))
        return f"range-{base[:7]}-{head[:7]}", {"kind": "range", "base": base, "head": head}
    pr = json.loads(run("gh", "pr", "view", target, "--json", "number,url,title,headRefOid,baseRefName,author"))
    git("fetch", "--no-tags", "--quiet", "origin", f"pull/{pr['number']}/head", pr["baseRefName"])
    base = git("merge-base", f"origin/{pr['baseRefName']}", pr["headRefOid"]).strip()
    return f"pr-{pr['number']}", {"kind": "pr", "base": base, "head": pr["headRefOid"], "number": pr["number"],
                                  "url": pr["url"], "title": pr["title"], "author": pr["author"]["login"]}


PLAIN = ("--no-ext-diff", "--no-color", "--src-prefix=a/", "--dst-prefix=b/")  # user config may change these


def diff(t, *extra):
    if t["kind"] == "local":
        return git("diff", *PLAIN, *extra, "HEAD")  # untracked files are listed in 01-scope, not diffed
    return git("diff", *PLAIN, *extra, t["base"], t["head"])


def since_view(prev, t, path):
    """The file's change since the reviewer viewed it at `prev`, with base-branch merges taken out."""
    old_head, old_base = prev["viewed"][path], prev["target"]["base"]
    rebased = old_head
    if old_base != t["base"]:
        r = subprocess.run(["git", "merge-tree", "--write-tree", "--merge-base", old_base, old_head, t["base"]],
                           capture_output=True, text=True)
        if r.returncode == 0:  # on a conflict, fall back to the plain diff, which shows base changes too
            rebased = r.stdout.split()[0]
    return git("diff", *PLAIN, rebased, t["head"], "--", path)


def start(target):
    prefix, t = resolve(target)
    runs = sorted(p for p in home().glob(f"{prefix}-[0-9][0-9]") if (p / "target.json").exists())
    d = home() / f"{prefix}-{len(runs) + 1:02d}"
    d.mkdir(parents=True)
    (d / "target.json").write_text(json.dumps(t, indent=2) + "\n")
    patch, patch_w = diff(t), diff(t, "-w")
    (d / "diff.patch").write_text(patch)
    (d / "diff-w.patch").write_text(patch_w)
    files = re.findall(r"^diff --git a/.* b/(.*)$", patch, re.M)
    hunks_w = {re.match(r"diff --git a/.* b/(.*)", c).group(1) for c in re.split(r"(?m)^(?=diff --git )", patch_w)
               if c.startswith("diff --git") and "\n@@ " in c}
    whitespace_only = [f for f in files if f not in hunks_w]  # -w keeps the header of a whitespace-only file
    untracked = git("ls-files", "--others", "--exclude-standard").split() if t["kind"] == "local" else []

    prev = None
    if runs and (runs[-1] / "viewed.json").exists():
        prev = {"name": runs[-1].name, "target": json.loads((runs[-1] / "target.json").read_text()),
                "viewed": json.loads((runs[-1] / "viewed.json").read_text())["viewed"]}
    rows, unchanged, changed, since = [], [], [], []
    for f in files:
        if prev and f in prev["viewed"]:
            p = since_view(prev, t, f)
            (changed if p else unchanged).append(f)
            since.append(p)
            rows.append(f"| `{f}` | {'changed since view' if p else 'unchanged since view'} |")
        else:
            rows.append(f"| `{f}` | {'whitespace only' if f in whitespace_only else 'new to you'} |")
            if prev:  # review-check gets only since-view.patch in a new round, so a file new to you needs its whole diff
                since.append(git("diff", *PLAIN, t["base"], t["head"], "--", f))
    (d / "since-view.patch").write_text("".join(since))
    (d / "unchanged.txt").write_text("".join(f + "\n" for f in unchanged))

    what = {"pr": f"PR #{t.get('number')} {t.get('title')} by {t.get('author')}", "local": "uncommitted changes",
            "range": target}[t["kind"]]
    body = [f"Target: {what}", f"Base: {t['base']}", f"Head: {t['head'] or 'working tree'}",
            f"Previous run: {prev['name'] if prev else 'none'}", "",
            f"{len(files)} files. {len(changed)} changed since your last view, {len(unchanged)} unchanged since view, "
            f"{len(whitespace_only)} whitespace only.", "",
            "`diff.patch` is the whole change. `since-view.patch` is what changed after you viewed each file, plus the "
            "whole diff of each file new to you.",
            "`unchanged.txt` lists the files that `remark` can mark viewed on GitHub again.", "",
            "| File | State |", "|---|---|", *rows, *(f"| `{f}` | untracked |" for f in untracked)]
    rest = "Uses:\n" + "\n".join(body) + "\n"
    (d / "01-scope.md").write_text(f"Status: done {hashlib.sha1(rest.encode()).hexdigest()[:12]}\n{rest}")
    skipped = t["kind"] != "pr"
    for step, uses in list(STEPS.items())[1:]:
        status = "skipped not a PR" if skipped and step == "05-walk" else "open"
        (d / f"{step}.md").write_text(f"Status: {status}\nUses: {', '.join(uses)}\n")
    print(d)


def status(name=None):
    runs = sorted(p for p in home().glob("*-[0-9][0-9]") if (p / "target.json").exists())
    d = home() / name if name else (runs[-1] if runs else None)
    if not d or not d.exists():
        raise SystemExit("no run")
    current = {s: revision(d / f"{s}.md") for s in STEPS}
    state = {s: (d / f"{s}.md").read_text().splitlines()[0].removeprefix("Status: ") for s in STEPS}
    print(d)
    for s, uses in STEPS.items():
        used = dict(re.findall(r"([\w-]+)@(\w+)", (d / f"{s}.md").read_text().splitlines()[1]))
        stale = [u for u, rev in used.items() if current.get(u) != rev and state.get(u, "").startswith("done")]
        ready = all(state[u].split()[0] in ("done", "skipped") for u in uses)
        note = "stale: " + ", ".join(stale) if stale else ("ready" if ready and state[s] == "open" else "")
        print(f"{s:12} {state[s]:30} {note}")
    for problem in checks(d, state):
        print("problem:", problem)


def checks(d, state):
    """What a done step claims and the files can disprove: every file mapped once, and the verdict line
    copied from a done review-check run of this head."""
    out = []
    text = {s: (d / f"{s}.md").read_text() for s in STEPS}
    if state["02-map"].startswith("done"):
        files = re.findall(r"^\| `([^`]+)` \|", text["01-scope"], re.M)
        # A fenced diagram has three backticks, which would shift every inline pair after it.
        mapped = re.findall(r"`([^`]+)`", re.sub(r"```.*?```", "", text["02-map"], flags=re.S))
        missing, twice = [f for f in files if f not in mapped], [f for f in files if mapped.count(f) > 1]
        if missing or twice:
            out.append(f"02-map: not mapped {', '.join(missing) or 'none'}; mapped twice {', '.join(twice) or 'none'}")
    if state["03-check"].startswith("done"):
        run = re.search(r"^Run: (\S+)", text["03-check"], re.M)
        rc = home().parent.parent / run.group(1) if run else None
        verdict = (rc / "03-verdict.md").read_text().splitlines() if rc and (rc / "03-verdict.md").exists() else []
        target = json.loads((rc / "target.json").read_text()) if rc and (rc / "target.json").exists() else {}
        head = json.loads((d / "target.json").read_text())["head"]
        if len(verdict) < 3 or not verdict[0].startswith("Status: done"):
            out.append("03-check: its `Run:` line names no done review-check verdict")
        elif text["03-check"].splitlines()[2:3] != verdict[2:3]:
            out.append("03-check: line 3 is not line 3 of the review-check verdict")
        elif target and target.get("head") != head:
            out.append("03-check: the review-check run is for another head")
        elif state["04-comments"].startswith("done") and text["04-comments"].splitlines()[2:3] != verdict[2:3]:
            out.append("04-comments: line 3 is not the review-check verdict")
    return out


def pr_query(t, fields):
    owner, repo = re.match(r"https://github.com/([^/]+)/([^/]+)/", t["url"]).groups()
    return gql(f"""query($o:String!,$r:String!,$n:Int!){{repository(owner:$o,name:$r){{pullRequest(number:$n){{
      id headRefOid {fields} }}}}}}""", o=owner, r=repo, n=t["number"])["repository"]["pullRequest"]


def load(name):
    d = home() / name
    t = json.loads((d / "target.json").read_text())
    if t["kind"] != "pr":
        raise SystemExit("viewed, remark and post need a PR run")
    return d, t


def viewed(name):
    d, t = load(name)
    files, after = [], None
    while True:
        pr = pr_query(t, f"""files(first:100{f',after:"{after}"' if after else ''}){{
            nodes{{path viewerViewedState}} pageInfo{{hasNextPage endCursor}} }}""")
        files += pr["files"]["nodes"]
        if not pr["files"]["pageInfo"]["hasNextPage"]:
            break
        after = pr["files"]["pageInfo"]["endCursor"]
    # GitHub drops the mark when a file changes, so a file marked now was viewed at the current head.
    marks = {f["path"]: pr["headRefOid"] for f in files if f["viewerViewedState"] == "VIEWED"}
    notes = pr_query(t, """reviews(first:5,states:PENDING){nodes{comments(first:100){
        nodes{path line body}}}}""")["reviews"]["nodes"]
    notes = [c for r in notes for c in r["comments"]["nodes"]]
    (d / "viewed.json").write_text(json.dumps({"viewed": marks, "pending_comments": notes}, indent=2) + "\n")
    print(f"{len(marks)} of {len(files)} files viewed at {pr['headRefOid'][:7]}, {len(notes)} pending comments")
    if pr["headRefOid"] != t["head"]:
        print(f"the PR head moved to {pr['headRefOid'][:7]} after this run; start a new run for the new commits")


def remark(name):
    d, t = load(name)
    pr = pr_query(t, "")
    if pr["headRefOid"] != t["head"]:
        raise SystemExit("the PR head moved after this run, so a file may have changed again; start a new run")
    for path in (d / "unchanged.txt").read_text().split():
        gql("mutation($p:ID!,$f:String!){markFileAsViewed(input:{pullRequestId:$p,path:$f}){clientMutationId}}",
            p=pr["id"], f=path)
        print("viewed", path)


def new_side_lines(patch, path):
    """(hunk index, new line number, text) for every line that exists after the change."""
    chunk = next((c for c in re.split(r"(?m)^(?=diff --git )", patch)
                  if re.match(rf"diff --git a/\S+ b/{re.escape(path)}\n", c)), None)
    if chunk is None:
        raise SystemExit(f"{path}: not in the diff")
    out, hunk, new = [], -1, 0
    for line in chunk.splitlines():
        h = re.match(r"@@ -\d+(?:,\d+)? \+(\d+)(?:,\d+)? @@", line)
        if h:
            hunk, new = hunk + 1, int(h.group(1))
        elif hunk >= 0 and not line.startswith(("-", "\\")):
            out.append((hunk, new, line[1:]))
            new += 1
    return out


def anchor(patch, path, first, last=None):
    """Line range of a quote on the new side. `first` must match exactly one line."""
    lines = new_side_lines(patch, path)
    hits = [i for i, (_, _, text) in enumerate(lines) if first in text]
    if len(hits) != 1:
        raise SystemExit(f"{path}: {first!r} matches {len(hits)} diff lines; quote a longer part")
    i = hits[0]
    if last is None:
        return None, lines[i][1]
    same_hunk = [j for j in range(i + 1, len(lines)) if lines[j][0] == lines[i][0]]
    for match in (lambda text: text.strip() == last, lambda text: last in text):
        j = next((j for j in same_hunk if match(lines[j][2])), None)
        if j is not None:
            return lines[i][1], lines[j][1]
    raise SystemExit(f"{path}: {last!r} not found after line {lines[i][1]} in the same hunk")


def post(name, comments_file):
    """Each comment is {path, body, first?, last?}. No `first` makes a file-level comment."""
    d, t = load(name)
    patch = (d / "diff.patch").read_text()
    planned = []
    for c in json.loads(Path(comments_file).read_text()):
        if SECRET.search(c["body"]):  # a literal check only; the chair still reads each body for other secrets
            raise SystemExit(f"{c['path']}: a comment holds what looks like a secret; tell the user in chat instead")
        thread = {"path": c["path"], "body": c["body"], "subjectType": "FILE"}
        if c.get("first"):
            start_line, end = anchor(patch, c["path"], c["first"], c.get("last"))
            thread |= {"subjectType": "LINE", "line": end, "side": "RIGHT"}
            if start_line is not None:
                thread |= {"startLine": start_line, "startSide": "RIGHT"}
        planned.append(thread)  # every anchor resolves before anything is written

    pr = pr_query(t, "reviews(first:5,states:PENDING){nodes{id comments(first:100){nodes{path body}}}}")
    if pr["headRefOid"] != t["head"]:
        raise SystemExit("the PR head moved after this run, so line numbers may be wrong; start a new run")
    pending = pr["reviews"]["nodes"]
    if pending:
        review = pending[0]["id"]
        seen = {(c["path"], c["body"]) for c in pending[0]["comments"]["nodes"]}
    else:
        review = gql("mutation($p:ID!,$c:GitObjectID!){addPullRequestReview(input:{pullRequestId:$p,commitOID:$c})"
                     "{pullRequestReview{id}}}", p=pr["id"], c=t["head"])["addPullRequestReview"]["pullRequestReview"]["id"]
        seen = set()
    for thread in planned:
        if (thread["path"], thread["body"]) in seen:
            print("already there", thread["path"])
            continue
        gql("mutation($i:AddPullRequestReviewThreadInput!){addPullRequestReviewThread(input:$i){thread{id}}}",
            i={"pullRequestReviewId": review, **thread})
        seen.add((thread["path"], thread["body"]))
        print("added", thread["path"], thread.get("line", "file"))
    print(f"pending review {review}; submit it yourself on GitHub")


if __name__ == "__main__":
    commands = {"start": start, "status": status, "viewed": viewed, "remark": remark, "post": post}
    if len(sys.argv) < 2 or sys.argv[1] not in commands:
        raise SystemExit(__doc__)
    commands[sys.argv[1]](*sys.argv[2:])
