"""Run: python3 test_build_checks.py. Exercise the commands shipped in build blocks."""
import re
import shlex
import subprocess
import tempfile
from pathlib import Path

KIT = Path(__file__).resolve().parents[1]


def test_inline_determinism_scan():
    text = (KIT / "blocks/build-canvas/SKILL.md").read_text(encoding="utf-8")
    command = next(re.findall(r"`([^`]+)`", line)[0] for line in text.splitlines()
                   if line.startswith("- `rg ") and "Math" in line)
    with tempfile.TemporaryDirectory() as tmp:
        site = Path(tmp)
        page = site / "index.html"
        args = shlex.split(command.replace("{{site}}", shlex.quote(tmp)))
        for call in ("Math.random()", "Date.now()"):
            page.write_text(f"<script>const state = {call};</script>\n", encoding="utf-8")
            out = subprocess.run(args, capture_output=True, text=True)
            assert out.returncode == 0 and call in out.stdout, out
        page.write_text("<script>const state = seeded(7);</script>\n", encoding="utf-8")
        out = subprocess.run(args, capture_output=True, text=True)
        assert out.returncode == 1 and not out.stdout and not out.stderr, out


if __name__ == "__main__":
    test_inline_determinism_scan()
    print("ok")
