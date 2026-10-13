"""Run: python3 test_comment_round.py"""
import json, os, subprocess, sys, tempfile
from pathlib import Path

SCRIPT = Path(__file__).with_name("comment_round.py")

# A stand-in for plannotator: it writes $FAKE_RESULT to the --result-file path, as the real one does.
FAKE = """#!/bin/sh
while [ $# -gt 0 ]; do [ "$1" = --result-file ] && out="$2"; shift; done
printf '%s' "$FAKE_RESULT" > "$out"
"""


def setup():
    root = Path(tempfile.mkdtemp())
    (root / "bin").mkdir()
    (root / "bin" / "plannotator").write_text(FAKE)
    (root / "bin" / "plannotator").chmod(0o755)
    (root / "plan.md").write_text("# Plan: x\n", encoding="utf-8")
    return root


def cr(root, command, result=None, path=None, files=("plan.md",)):
    env = {**os.environ, "PATH": f"{root / 'bin'}:/usr/bin:/bin" if path is None else path,
           "FAKE_RESULT": json.dumps(result) if isinstance(result, dict) else (result or "")}
    return subprocess.run([sys.executable, "-I", SCRIPT, command, root / "review", *(root / f for f in files)],
                          capture_output=True, text=True, env=env)


def note(decision, count=0, message=""):
    return {"v": 1, "surface": "annotate", "decision": decision, "message": message, "noop": False, "annotationCount": count}


def test_rounds_bind_approval_to_the_reviewed_bytes():
    root = setup()
    assert "no round in" in cr(root, "check").stderr
    out = cr(root, "run", note("annotated", 1, "drop step 3"))
    assert out.returncode == 0 and out.stdout == "round 1: annotated, 1 notes\n", out
    saved = json.loads((root / "review" / "1.json").read_text())
    assert saved["files"] == {"../plan.md": __import__("hashlib").sha256(b"# Plan: x\n").hexdigest()}, saved
    assert "round 1 has notes but no answers" in cr(root, "run", note("approved")).stderr
    assert "round 1 has notes but no answers" in cr(root, "check").stderr
    (root / "review" / "1-answers.md").write_text("> drop step 3\nremoved 03-tokens\n")
    (root / "plan.md").write_text("# Plan: x, no tokens\n", encoding="utf-8")
    assert "round 1 is annotated, not approved" in cr(root, "check").stderr
    assert cr(root, "run", note("approved")).returncode == 0
    out = cr(root, "check")
    assert out.returncode == 0 and out.stdout == "round 2 approved these files\n", out
    # An edit after approval voids it, and a check on other files does not borrow it.
    (root / "plan.md").write_text("# Plan: x, edited\n", encoding="utf-8")
    assert "../plan.md changed after round 2; start a new round" in cr(root, "check").stderr
    (root / "other.md").write_text("y")
    assert "round 2 reviewed ../plan.md, not these files" in cr(root, "check", files=("plan.md", "other.md")).stderr


def test_stops():
    root = setup()
    out = cr(root, "run", note("approved"), path="/usr/bin:/bin")
    assert out.returncode == 2 and out.stderr == "unavailable plannotator\n", out
    out = cr(root, "run", "not json")
    assert out.returncode == 1 and "gave no decision JSON" in out.stderr and not (root / "review" / "1.json").exists(), out
    assert "is not a file; a round reviews files only" in cr(root, "run", note("approved"), files=("https://x.test/a",)).stderr
    for _ in range(3):
        assert cr(root, "run", note("dismissed")).returncode == 0
    out = cr(root, "run", note("approved"))
    assert out.returncode == 1 and "loop stop: 3 rounds ran" in out.stderr and not (root / "review" / "4.json").exists(), out
    (root / "review" / "2.json").write_text("{}")
    assert "2.json is not a round file" in cr(root, "check").stderr


if __name__ == "__main__":
    for name, test in list(globals().items()):
        if name.startswith("test_"):
            test()
    print("ok")
