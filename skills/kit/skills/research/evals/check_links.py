"""Run: python3 -I check_links.py <report.md>. Open each cited link; exit 0 when every link opens."""
import re
import sys
import urllib.request
from pathlib import Path

# Some sites refuse a client with no browser name, so a plain request reads as a dead link.
AGENT = "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0) AppleWebKit/605.1.15 (KHTML, like Gecko) Safari/605.1.15"
text = Path(sys.argv[1]).read_text(encoding="utf-8")
links = sorted(set(u.rstrip(".,;:)]>'\"`") for u in re.findall(r"https?://[^\s)<>\]`\"']+", text)))
dead = []
for url in links:
    try:
        with urllib.request.urlopen(urllib.request.Request(url, headers={"User-Agent": AGENT}), timeout=20) as r:
            ok = r.status < 400
    except Exception as e:  # noqa: BLE001 - any failure to open counts as dead
        ok, r = False, e
    if not ok:
        dead.append(f"dead: {url} ({r})")
print(f"{len(links)} links, {len(dead)} dead")
print("\n".join(dead))
sys.exit(1 if dead or not links else 0)
