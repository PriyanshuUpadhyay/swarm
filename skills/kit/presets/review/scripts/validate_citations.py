#!/usr/bin/env python3
"""
validate_citations.py — verify every file:line citation in an explanation
points at a real changed hunk of the PR diff.

Why this exists: a prompt rule ("always cite") catches a *missing* citation,
never a *fabricated* one. A well-formatted but hallucinated [file:42] reads as
more authoritative than honest uncertainty, so the reviewer stops spot-checking
exactly when they should. This is the mechanical enforcement that makes
"evidence-backed" real instead of aspirational.

Usage:
  validate_citations.py DIFF.patch EXPLANATION.md [--context CONTEXT.txt]

Exit codes:
  0  all citations valid (or none found — prints a warning)
  1  one or more citations don't land in a changed hunk (or cite a file not in the PR)

DIFF.patch    a unified diff (gh pr diff / git diff)
EXPLANATION.md the drafted explanation to check
--context     optional file of "path:line" entries you deliberately cited as
              surrounding context (labelled "context, not part of this change");
              these are accepted even though they're outside the hunks.
"""
import re
import sys
from collections import defaultdict

CITATION_RE = re.compile(r"([\w./\\-]*[\w]):(\d+)(?:-(\d+))?")
HUNK_RE = re.compile(r"^@@ -\d+(?:,\d+)? \+(\d+)(?:,(\d+))? @@")


def parse_diff(path):
    """Return {normalized_file_path: [(new_start, new_end), ...]} of changed-line ranges (new side)."""
    ranges = defaultdict(list)
    cur = None
    with open(path, encoding="utf-8", errors="replace") as fh:
        for line in fh:
            if line.startswith("+++ "):
                # "+++ b/src/foo.ts" or "+++ /dev/null"
                p = line[4:].strip()
                if p[:2] in ("a/", "b/", "i/", "w/", "c/", "o/"):  # git default + mnemonicPrefix
                    p = p[2:]
                cur = None if p == "/dev/null" else p
            elif line.startswith("@@"):
                m = HUNK_RE.match(line)
                if m and cur:
                    start = int(m.group(1))
                    span = int(m.group(2)) if m.group(2) else 1
                    if span > 0:
                        ranges[cur].append((start, start + span - 1))
    return ranges


def parse_context(path):
    ctx = set()
    if not path:
        return ctx
    with open(path, encoding="utf-8", errors="replace") as fh:
        for line in fh:
            m = CITATION_RE.search(line)
            if m:
                a = int(m.group(2))
                b = int(m.group(3)) if m.group(3) else a
                ctx.add((m.group(1), a, b))
    return ctx


def looks_like_path(token):
    # require a file-ish token to cut false positives like "12:30" or "step:3"
    return ("." in token or "/" in token or "\\" in token) and not token.isdigit()


def match_file(cite_path, diff_files):
    """Match a citation path to a diff file. Exact / path-suffix match wins over basename,
    and a basename match counts ONLY when it is unique among the diff files (otherwise the
    citation must be path-qualified). Two passes — never let an early basename collision
    (e.g. several `index.ts` files) beat a later exact path match."""
    cp = cite_path.replace("\\", "/")
    has_slash = "/" in cp
    for f in diff_files:  # pass 1: exact, or path-suffix ONLY for a path-qualified citation
        if f == cp:
            return f
        if has_slash and (f.endswith("/" + cp) or cp.endswith("/" + f)):
            return f
    base = cp.split("/")[-1]  # pass 2: bare basename, only if it's unique among diff files
    hits = [f for f in diff_files if f.split("/")[-1] == base]
    return hits[0] if len(hits) == 1 else None


def extract_citations(path):
    cites = []
    with open(path, encoding="utf-8", errors="replace") as fh:
        for lineno, line in enumerate(fh, 1):
            for m in CITATION_RE.finditer(line):
                token = m.group(1)
                if not looks_like_path(token):
                    continue
                a = int(m.group(2))
                b = int(m.group(3)) if m.group(3) else a
                cites.append((token, a, b, lineno))
    return cites


def overlaps(a, b, ranges):
    return any(not (b < lo or a > hi) for lo, hi in ranges)


def main():
    args = [x for x in sys.argv[1:]]
    ctx_path = None
    if "--context" in args:
        i = args.index("--context")
        ctx_path = args[i + 1]
        del args[i : i + 2]
    if len(args) != 2:
        print(__doc__)
        return 2

    diff_path, expl_path = args
    ranges = parse_diff(diff_path)
    diff_files = list(ranges.keys())
    context = parse_context(ctx_path)
    cites = extract_citations(expl_path)

    if not cites:
        print("⚠️  No file:line citations found in the explanation.")
        print("    A comprehension artifact should ground its claims in the diff — "
              "add inline citations unless the PR is truly trivial.")
        return 0

    invalid = []
    for token, a, b, src_line in cites:
        matched = match_file(token, diff_files)
        rng = f"{a}" if a == b else f"{a}-{b}"
        if matched is None:
            # accept if explicitly declared context
            if any(token.endswith(cp) or cp.endswith(token) for cp, _, _ in context):
                print(f"  ctx  {token}:{rng}  (declared context)")
                continue
            invalid.append((token, rng, src_line, "file not in this PR's diff"))
        elif overlaps(a, b, ranges[matched]):
            print(f"  OK   {token}:{rng}  → {matched}")
        elif (token, a, b) in context or any(
            (matched.endswith(cp) or cp.endswith(token)) for cp, _, _ in context
        ):
            print(f"  ctx  {token}:{rng}  → {matched} (declared context)")
        else:
            hunks = ", ".join(f"{lo}-{hi}" for lo, hi in ranges[matched])
            invalid.append((token, rng, src_line, f"line outside changed hunks (changed: {hunks})"))

    print()
    total = len(cites)
    if invalid:
        print(f"❌ {len(invalid)}/{total} citation(s) FAILED — fix the line or drop the claim:")
        for token, rng, src_line, why in invalid:
            print(f"   - {token}:{rng}  ({expl_path}:{src_line}) — {why}")
        return 1

    print(f"✅ all {total} citation(s) land in real changed hunks.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
