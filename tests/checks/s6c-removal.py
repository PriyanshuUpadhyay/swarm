"""S6C-FENCES-V1 removal audit. Run python3.12 tests/checks/s6c-removal.py."""
from pathlib import Path
import json
import re

root = Path(__file__).resolve().parents[2]
web = root / "web"
legacy = re.compile(r"\b(?:diffFence|prefixHtml|isPatch|parsePatch|PatchLine)\b")
# Only production source counts. Frozen tests name the removed APIs as evidence.
source_hits = []
for path in sorted((web / "src").rglob("*")):
    if path.suffix not in {".ts", ".tsx", ".css"} or ".test." in path.name:
        continue
    text = path.read_text()
    if legacy.search(text) or "diff2html" in text or "d2h" in text:
        source_hits.append(str(path.relative_to(root)))
packages = ["diff2html", "@profoundlogic/hogan", "diff@8.0.4", "nopt@1.0.10", "abbrev@1.1.1"]
# highlight.js is conditional in the contract and needs a reference audit.
lock = (web / "pnpm-lock.yaml").read_text()
actual = {
    "source": source_hits,
    "raw_html": "dangerouslySetInnerHTML" in (web / "src/react/markdown-view.tsx").read_text(),
    "importer": "diff2html" in (web / "package.json").read_text(),
    "lock": [name for name in packages if name in lock],
}
expected = {"source": [], "raw_html": False, "importer": False, "lock": []}
print(json.dumps({"actual": actual, "expected": expected}, indent=2))
assert actual == expected
