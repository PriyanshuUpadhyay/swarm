"""Run: python3 test_reach.py"""
import json, os, subprocess, sys, tempfile
from pathlib import Path

SCRIPT = Path(__file__).with_name("reach.py")

# Stand-ins for curl and playwriter. curl answers from $FAKE_CURL, a JSON map of URL substring to
# [body, status]; an unmatched URL gets a 403. playwriter prints $FAKE_PAGE as the page.
CURL = """#!/usr/bin/env python3
import json, os, sys
url = sys.argv[-1]
for part, (body, code) in json.loads(os.environ.get("FAKE_CURL", "{}")).items():
    if part in url:
        print(body + "\\n" + str(code), end="")
        break
else:
    print("Forbidden\\n403", end="")
"""
PLAYWRITER = """#!/bin/sh
case "$1" in
  session) [ "$2" = new ] && echo "Session s1 created" ;;
  -s) printf '%s\\n' "$FAKE_PAGE" ;;
esac
"""


def reach(*args, curl=None, page=""):
    root = Path(tempfile.mkdtemp())
    (root / "curl").write_text(CURL)
    (root / "playwriter").write_text(PLAYWRITER)
    for f in ("curl", "playwriter"):
        (root / f).chmod(0o755)
    env = {**os.environ, "PATH": f"{root}:/usr/bin:/bin", "FAKE_CURL": json.dumps(curl or {}),
           "FAKE_PAGE": page, "REACH_LAST30DAYS": str(root / "absent.py")}
    return subprocess.run([sys.executable, "-I", SCRIPT, *args], capture_output=True, text=True, env=env)


def test_search_hn_gives_stories():
    hits = {"hits": [{"objectID": "42", "title": "AGENTS.md study", "created_at": "2026-10-01T00:00:00Z"}]}
    r = reach("search", "hn", "AGENTS.md", curl={"hn.algolia.com": [json.dumps(hits), 200]})
    assert r.returncode == 0, r.stderr
    assert json.loads(r.stdout)["url"] == "https://news.ycombinator.com/item?id=42"


def test_read_falls_back_to_the_browser_when_the_reader_is_blocked():
    r = reach("read", "https://example.com/post", page="The post text")
    assert r.returncode == 0, r.stderr
    assert "The post text" in r.stdout


def test_blocked_when_every_route_fails():
    r = reach("read", "https://example.com/post", page="")
    assert r.returncode == 3
    assert "blocked: example.com:" in r.stderr


def test_paper_refs_from_semantic_scholar():
    refs = {"data": [{"citedPaper": {"title": "Agent READMEs", "year": 2025, "externalIds": {"ArXiv": "2511.12884"}}}]}
    r = reach("paper", "2602.11988", "refs", curl={"api.semanticscholar.org": [json.dumps(refs), 200]})
    assert r.returncode == 0, r.stderr
    assert json.loads(r.stdout)["url"] == "https://arxiv.org/abs/2511.12884"


if __name__ == "__main__":
    for name, fn in list(globals().items()):
        if name.startswith("test_"):
            fn()
            print("ok", name)
