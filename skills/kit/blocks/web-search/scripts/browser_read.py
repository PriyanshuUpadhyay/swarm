#!/usr/bin/env python3
"""Read a page that a plain fetch cannot open, in the user's own signed-in browser, through playwriter.

  browser_read.py x "<query>"   newest X posts for the query, as JSON lines: link, time, text
  browser_read.py <url>         one page as text (an X page gives its posts; any other page its main text)

It opens its own tab, reads, and closes the tab. It never clicks, types, or posts.
"""

import json
import re
import subprocess
import sys
import urllib.parse

POSTS = """[...document.querySelectorAll("article")].map(a => ({
  link: a.querySelector('a[href*="/status/"]')?.href,
  time: a.querySelector("time")?.getAttribute("datetime"),
  text: a.querySelector('[data-testid="tweetText"]')?.innerText}))"""


def playwriter(*args):
    run = subprocess.run(["playwriter", *args], capture_output=True, text=True)
    if run.returncode != 0:
        raise SystemExit(f"playwriter {args[0]} failed: {run.stderr.strip() or run.stdout.strip()}")
    return run.stdout


def read(url):
    # The URL goes into the JavaScript as a JSON string, so it cannot break out of the code.
    x = urllib.parse.urlparse(url).hostname in ("x.com", "twitter.com")
    extract = (f"""await state.page.waitForSelector("article", {{ timeout: 25000 }})
const posts = await state.page.evaluate(() => {POSTS})
for (const p of posts) console.log(JSON.stringify(p))"""
               if x else "console.log(await getPageMarkdown({ page: state.page, showDiffSinceLastCall: false }))")
    code = f"""state.page = await context.newPage()
try {{
await state.page.goto({json.dumps(url)}, {{ waitUntil: "domcontentloaded" }})
await waitForPageLoad({{ page: state.page, timeout: 8000 }})
{extract}
}} finally {{ await state.page.close() }}"""
    session = re.search(r"Session (\S+) created", playwriter("session", "new", "--tab-group", "search"))
    if not session:
        raise SystemExit("playwriter gave no session; is the Playwriter extension on in the browser?")
    try:
        out = playwriter("-s", session.group(1), "--timeout", "60000", "-e", code)
    finally:
        subprocess.run(["playwriter", "session", "delete", session.group(1)], capture_output=True)
    # playwriter prints "[log] " before each console line and warns about the tab this script closed.
    print("\n".join(l.removeprefix("[log] ") for l in out.splitlines()
                    if l != "Console output:" and not l.startswith("[WARNING] Page closed")))


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "x":
        read("https://x.com/search?" + urllib.parse.urlencode({"q": sys.argv[2], "f": "live"}))
    elif len(sys.argv) == 2:
        read(sys.argv[1])
    else:
        raise SystemExit(__doc__)
