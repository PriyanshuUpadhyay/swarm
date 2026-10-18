"""Run: python3 test_start_plan.py"""
import hashlib, json, subprocess, sys, tempfile
from pathlib import Path

SCRIPT = Path(__file__).with_name("start_plan.py")
PLAN = """# Plan: x

Done when: y.
- y
Evidence:
- z

| File | Needs | Holds | Block | Why |
|---|---|---|---|---|
| 01-reply | none | the reply | voice-reply | the owner reads it |
"""


def approve(run, decision="approved"):
    files = {f"../{name}": hashlib.sha256((run / name).read_bytes()).hexdigest() for name in ("plan.html", "plan.md")}
    result = {"decision": decision, "message": "", "annotationCount": 0}
    (run / "plan-review").mkdir(exist_ok=True)
    (run / "plan-review" / "1.json").write_text(json.dumps({"files": files, "result": result}))


def start(run):
    return subprocess.run([sys.executable, "-I", SCRIPT, run], capture_output=True, text=True)


def test_start_needs_the_current_approval():
    run = Path(tempfile.mkdtemp())
    (run / "plan.md").write_text(PLAN, encoding="utf-8")
    (run / "plan.html").write_text("<p>chart</p>", encoding="utf-8")
    out = start(run)
    assert out.returncode == 1 and "no round in" in out.stderr and not (run / "01-reply.md").exists(), out
    approve(run, "annotated")
    assert "round 1 is annotated, not approved" in start(run).stderr
    approve(run)
    (run / "plan.md").write_text(PLAN.replace("it |", "it now |"), encoding="utf-8")
    out = start(run)
    assert "../plan.md changed after round 1" in out.stderr and not (run / "01-reply.md").exists(), out
    approve(run)
    out = start(run)
    assert out.returncode == 0 and (run / "01-reply.md").read_text().startswith("Status: open\n"), out


if __name__ == "__main__":
    test_start_needs_the_current_approval()
    print("ok")
