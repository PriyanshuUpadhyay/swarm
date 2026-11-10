#!/usr/bin/env python3
"""Reach a site through its known routes, in a fixed order, and log each route that fails.

  reach.py search <site> "<query>" [--days N]   site: hn, reddit, x, lobsters, github, youtube, web
  reach.py read <url>                            one page as text
  reach.py paper <arxiv id> refs|cites           the paper's references, or the papers that cite it

Stdout holds the first route that gave content: JSON lines (title, url, date) from an API route,
or page text from a page route. Stderr holds one line per route tried, then a `blocked:` line when
every route failed. Exit 0 on content, 3 when blocked, 2 on a usage error.

Routes only read. The browser route uses the user's signed-in browser through browser_read.py.
Keys are optional: S2_API_KEY (Semantic Scholar) and OPENALEX_API_KEY. Each is read from the
environment, else from the macOS Keychain entry `swarm-<name>`, so every pane finds it.
"""

import json
import os
import re
import subprocess
import sys
import time
import urllib.parse
from pathlib import Path

UA = "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0) AppleWebKit/605.1.15 Safari/605.1.15"
BROWSER = Path(__file__).with_name("browser_read.py")
LAST30DAYS = Path(os.environ.get(
    "REACH_LAST30DAYS",
    Path.home() / ".local/share/swarm-vendor/last30days-v3.31.0/skills/last30days/scripts/last30days.py"))
# A bot wall often answers 200, so the body decides too.
WALLS = ("not a bot", "Just a moment", "AbuseAlleviationError", "Access Denied", "blocked by network security")


class Fail(Exception):
    pass


def run(cmd, timeout=120):
    try:
        out = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
    except FileNotFoundError:
        raise Fail(f"{cmd[0]} not installed")
    except subprocess.TimeoutExpired:
        raise Fail(f"timeout after {timeout}s")
    if out.returncode != 0:
        raise Fail((out.stderr.strip() or out.stdout.strip() or f"exit {out.returncode}").splitlines()[-1][:160])
    return out.stdout


def get(url, headers=()):
    cmd = ["curl", "-sS", "-L", "-m", "40", "-A", UA, "-w", "\n%{http_code}", url]
    for h in headers:
        cmd += ["-H", h]
    body, _, code = run(cmd).rpartition("\n")
    if code != "200":
        raise Fail(f"HTTP {code}")
    if not body.strip():
        raise Fail("empty body")
    if any(w in body[:3000] for w in WALLS):
        raise Fail("bot wall")
    return body


def get_json(url, headers=()):
    try:
        return json.loads(get(url, headers))
    except json.JSONDecodeError:
        raise Fail("not JSON")


def lines(items):
    items = [i for i in items if i.get("url")]
    if not items:
        raise Fail("no results")
    return "".join(json.dumps(i, ensure_ascii=False) + "\n" for i in items)


def browser(url):
    text = run([sys.executable, str(BROWSER), url], timeout=150)
    if not text.strip() or any(w in text[:3000] for w in WALLS):
        raise Fail("empty or bot wall")
    return text


def text(page):
    page = re.sub(r"(?is)<(script|style|noscript)[^>]*>.*?</\1>", " ", page)
    return re.sub(r"\n\s*\n+", "\n\n", re.sub(r"<[^>]+>", "", page)).strip() + "\n"


def q(s):
    return urllib.parse.quote(s)


def api_key(name):
    if os.environ.get(name):
        return os.environ[name]
    try:
        return run(["security", "find-generic-password", "-s", f"swarm-{name}", "-w"], timeout=10).strip() or None
    except Fail:
        return None


# --- search routes ---------------------------------------------------------------------------

def hn_algolia(query, days):
    since = int(time.time()) - days * 86400
    data = get_json(f"https://hn.algolia.com/api/v1/search?query={q(query)}&tags=story"
                    f"&numericFilters={q(f'created_at_i>{since}')}&hitsPerPage=30")
    return lines({"title": h.get("title"), "date": h.get("created_at", "")[:10],
                  "url": f"https://news.ycombinator.com/item?id={h['objectID']}",
                  "points": h.get("points"), "comments": h.get("num_comments")} for h in data["hits"])


def last30days(source):
    def route(query, days):
        if not LAST30DAYS.is_file():
            raise Fail(f"last30days not installed at {LAST30DAYS}")
        out = run([sys.executable, str(LAST30DAYS), query, "--search", source, "--quick",
                   "--days", str(days), "--emit=json"], timeout=300)
        data = json.loads(out)
        status = data.get("source_status", {}).get(source)
        if not data.get("results"):
            raise Fail(f"no results (status {status})")
        return lines({"title": r.get("title"), "url": r.get("url"), "date": r.get("date")}
                     for r in data["results"])
    route.__name__ = "last30days"
    return route


def reddit_browser(query, days):
    return browser(f"https://old.reddit.com/search?q={q(query)}&sort=new&t={'month' if days <= 31 else 'year'}")


def x_browser(query, days):
    out = run([sys.executable, str(BROWSER), "x", query], timeout=150)
    if not out.strip():
        raise Fail("no posts")
    return out


def lobsters_page(query, days):
    return get(f"https://lobste.rs/search?q={q(query)}&what=stories&order=newest")


def lobsters_browser(query, days):
    return browser(f"https://lobste.rs/search?q={q(query)}&what=stories&order=newest")


def github_gh(query, days):
    since = time.strftime("%Y-%m-%d", time.gmtime(time.time() - days * 86400))
    data = json.loads(run(["gh", "search", "issues", query, f"--updated=>{since}", "--sort", "updated",
                           "--limit", "30", "--json", "title,url,updatedAt"]))
    return lines({"title": i["title"], "url": i["url"], "date": i["updatedAt"][:10]} for i in data)


def youtube_ytdlp(query, days):
    out = run(["yt-dlp", "--flat-playlist", "--dump-json", f"ytsearch15:{query}"], timeout=150)
    return lines({"title": v.get("title"), "url": v.get("url") or f"https://www.youtube.com/watch?v={v['id']}",
                  "channel": v.get("channel")} for v in map(json.loads, out.splitlines()))


def web_exa(query, days):
    out = run(["mcporter", "call", "exa.web_search_exa", f"query={query}", "numResults=10"])
    if "URL:" not in out:
        raise Fail("no results")
    return out


SEARCH = {
    "hn": [hn_algolia],
    "reddit": [last30days("reddit"), reddit_browser],
    "x": [last30days("x"), x_browser],
    "lobsters": [lobsters_page, lobsters_browser],
    "github": [github_gh],
    "youtube": [youtube_ytdlp],
    "web": [web_exa],
}


# --- read routes -----------------------------------------------------------------------------

def arxiv_id(url):
    m = re.search(r"arxiv\.org/(?:abs|html|pdf)/([0-9]{4}\.[0-9]{4,5})(v\d+)?", url)
    return m and m.group(1) + (m.group(2) or "")


def read_arxiv(url):
    aid = arxiv_id(url)
    # arxiv.org rate-limits shared readers; export.arxiv.org serves the same abstract page.
    page = get(f"https://export.arxiv.org/abs/{aid}")
    m = re.search(r'<blockquote class="abstract[^>]*>(.*?)</blockquote>', page, re.S)
    t = re.search(r'<h1 class="title[^>]*>(.*?)</h1>', page, re.S)
    if not m:
        raise Fail("no abstract on the page")
    strip = lambda s: re.sub(r"\s+", " ", re.sub(r"<[^>]+>", "", s)).strip()
    return f"{strip(t.group(1)) if t else aid}\n{url}\n\n{strip(m.group(1))}\n"


def read_jina(url):
    return get(f"https://r.jina.ai/{url}")


def read_browser(url):
    return browser(url)


def read_wayback(url):
    snap = get_json(f"https://archive.org/wayback/available?url={q(url)}")
    closest = snap.get("archived_snapshots", {}).get("closest")
    if not closest:
        raise Fail("no copy")
    return text(get(closest["url"]))


READ = [read_jina, read_browser, read_wayback]


# --- paper routes ----------------------------------------------------------------------------

def paper_s2(aid, kind):
    field = "citedPaper" if kind == "refs" else "citingPaper"
    key = api_key("S2_API_KEY")
    data = get_json(f"https://api.semanticscholar.org/graph/v1/paper/arXiv:{aid}/"
                    f"{'references' if kind == 'refs' else 'citations'}?fields=title,year,externalIds,url&limit=100",
                    [f"x-api-key: {key}"] if key else ())
    return lines({"title": p.get("title"), "date": str(p.get("year") or ""),
                  "url": (f"https://arxiv.org/abs/{p['externalIds']['ArXiv']}"
                          if (p.get("externalIds") or {}).get("ArXiv") else p.get("url"))}
                 for p in (row[field] for row in data.get("data", [])))


def paper_openalex(aid, kind):
    key = api_key("OPENALEX_API_KEY")
    auth = f"&api_key={key}" if key else ""
    work = get_json(f"https://api.openalex.org/works/doi:10.48550/arXiv.{aid.split('v')[0]}?select=id,referenced_works{auth}")
    wid = work["id"].rsplit("/", 1)[-1]
    filt = f"cited_by:{wid}" if kind == "refs" else f"cites:{wid}"
    data = get_json(f"https://api.openalex.org/works?filter={filt}&per-page=100"
                    f"&select=title,publication_date,doi,id{auth}")
    return lines({"title": w.get("title"), "date": w.get("publication_date"), "url": w.get("doi") or w.get("id")}
                 for w in data.get("results", []))


def paper_arxiv_html(aid, kind):
    if kind != "refs":
        raise Fail("the paper page lists references only")
    page = get(f"https://arxiv.org/html/{aid}")
    items = re.findall(r'<li[^>]*class="ltx_bibitem"[^>]*>(.*?)</li>', page, re.S)
    if not items:
        raise Fail("no reference list on the page")
    out = []
    for item in items:
        text = re.sub(r"\s+", " ", re.sub(r"<[^>]+>", "", item)).strip()
        link = re.search(r'href="(https?://[^"]+)"', item)
        out.append({"title": text[:300], "url": link.group(1) if link else f"https://arxiv.org/html/{aid}#bib"})
    return lines(out)


PAPER = [paper_s2, paper_openalex, paper_arxiv_html]


# --- driver ----------------------------------------------------------------------------------

def ladder(label, routes, *args):
    tried = []
    for route in routes:
        try:
            out = route(*args)
        except (Fail, KeyError, ValueError) as e:
            reason = str(e) if isinstance(e, Fail) else f"unexpected reply ({type(e).__name__})"
            print(f"reach: {route.__name__} failed: {reason}", file=sys.stderr)
            tried.append(f"{route.__name__} → {reason}")
            continue
        print(f"reach: {route.__name__} ok", file=sys.stderr)
        try:
            sys.stdout.write(out)
        except BrokenPipeError:  # a reader such as head closed the pipe early
            pass
        return 0
    print(f"blocked: {label}: " + "; ".join(tried), file=sys.stderr)
    return 3


def main(argv):
    days = 30
    if "--days" in argv:
        i = argv.index("--days")
        days = int(argv[i + 1])
        argv = argv[:i] + argv[i + 2:]
    if len(argv) == 3 and argv[0] == "search" and argv[1] in SEARCH:
        return ladder(argv[1], SEARCH[argv[1]], argv[2], days)
    if len(argv) == 2 and argv[0] == "read":
        routes = [read_arxiv, *READ] if arxiv_id(argv[1]) else READ
        return ladder(urllib.parse.urlparse(argv[1]).hostname or argv[1], routes, argv[1])
    if len(argv) == 3 and argv[0] == "paper" and argv[2] in ("refs", "cites"):
        aid = arxiv_id(argv[1]) or argv[1]
        return ladder(f"arXiv:{aid} {argv[2]}", PAPER, aid, argv[2])
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
