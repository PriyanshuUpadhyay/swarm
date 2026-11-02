"""Run: python3 -I test_file_review.py. The file review outside a repo stops by code: no approve over a fix row,
at most 2 rounds unless the owner allows one more, and a blocked verdict at the cap."""
import subprocess
import sys
import tempfile
from pathlib import Path

SCRIPT = Path(__file__).resolve().parent / "file_review.py"


def run(folder, *args):
    return subprocess.run([sys.executable, "-I", str(SCRIPT), str(folder), *args], capture_output=True, text=True)


def rows(folder, *kinds, verdict=None):
    lines = ["| File or claim | pass or fix | Evidence |", "|---|---|---|"]
    lines += [f"| `index.html` row {i} | {kind} | render at 1440 |" for i, kind in enumerate(kinds)]
    (folder / "file-review.md").write_text("\n".join(lines + ([verdict] if verdict else [])) + "\n", encoding="utf-8")


with tempfile.TemporaryDirectory() as tmp:
    d = Path(tmp)
    out = run(d)
    assert out.returncode == 1 and "no " in out.stderr, out
    rows(d, "pass", "fix", verdict="Verdict: APPROVE")
    out = run(d)
    assert out.returncode == 1 and "APPROVE with 1 open fix" in out.stderr, out
    # A fix row with an unescaped pipe in its evidence refuses; it never closes as an approval.
    (d / "file-review.md").write_text("| index.html | fix | grep a | wc -l |\n", encoding="utf-8")
    out = run(d, "--close")
    assert out.returncode == 1 and "has 4 cells, not 3" in out.stderr and "Verdict" not in (d / "file-review.md").read_text(), out
    rows(d, "pass", "note")
    assert run(d).returncode == 1, "no verdict line yet"
    out = run(d, "--close")
    assert out.returncode == 0 and (d / "file-review.md").read_text().splitlines()[-1] == "Verdict: APPROVE", out
    assert run(d).returncode == 0
    assert run(d, "--close").returncode == 0 and (d / "file-review.md").read_text().count("Verdict:") == 1, "close twice keeps one line"

    rows(d, "pass", "fix")
    out = run(d, "--close")
    assert out.returncode == 1 and "Fix them, then run --next" in out.stdout and "Verdict" not in (d / "file-review.md").read_text(), out
    assert run(d, "--next").returncode == 0 and (d / "file-review-1.md").is_file()
    rows(d, "fix", "fix")
    out = run(d, "--close")
    assert out.returncode == 1 and (d / "file-review.md").read_text().splitlines()[-1] == "Verdict: BLOCKED (2 fix)", out
    out = run(d)
    assert out.returncode == 1 and "BLOCKED" in out.stderr, out
    out = run(d, "--next")
    assert out.returncode == 1 and "past the cap" in out.stderr and not (d / "file-review-2.md").exists(), out

    (d / "file-review-2.md").write_text("old round\n")
    rows(d, "pass", verdict="Verdict: APPROVE")
    out = run(d)
    assert out.returncode == 1 and "round 3 is past the cap" in out.stderr, "a third round file without an owner line refuses"
    (d / "answers.md").write_text("continue: file-review\n")
    assert run(d).returncode == 0, "the owner's continue line allows round 3"
    (d / "file-review-3.md").write_text("old round\n")
    assert run(d).returncode == 1, "continue allows one round, not more"
    assert run(d, "--bad").returncode != 0
print("ok")
