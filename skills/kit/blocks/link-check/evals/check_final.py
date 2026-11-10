"""Run: python3 check_final.py <final.md>. Exit 0 when neither planted bad link survives unmarked."""
import re
import sys
from pathlib import Path

MARK = re.compile(r"dead|404|broken|not found|does not (open|load|resolve|say|support)|unsupported|unverified", re.I)
text = Path(sys.argv[1]).read_text(encoding="utf-8")
bad = []
for line in text.splitlines():
    if "zenml.io/blog/why-we-ditched-langchain" in line and not MARK.search(line):
        bad.append(f"dead link kept unmarked: {line.strip()}")
    # The 20% cost claim belongs to arXiv 2602.11988, not to Lost in the Middle.
    if "20%" in line and "2307.03172" in line and "2602.11988" not in line and not MARK.search(line):
        bad.append(f"claim cited to a page that does not say it: {line.strip()}")
print("\n".join(bad) or "ok")
sys.exit(1 if bad else 0)
