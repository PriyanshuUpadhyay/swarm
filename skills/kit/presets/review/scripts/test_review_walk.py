"""Run: python3 test_review_walk.py, or pytest. Every test runs in its own throwaway repo, and the repo the
test started in is checked before and after, so a stray git call fails the run. It checks the parts that can go wrong
quietly: the diff since view must hide base-branch merges but keep files new to the reviewer, status
must catch a bad map or a copied verdict that does not match, and anchors must land on new-side lines."""
import contextlib, functools, io, json, os, subprocess, sys, tempfile
from contextlib import redirect_stdout
from pathlib import Path
from unittest.mock import patch

import review_walk as rw

import review_check as rc


def state(path):
    """HEAD, branches and status of the repo at `path`, for the before/after guard."""
    return [subprocess.run(["git", "-C", path, *a], capture_output=True, text=True).stdout
            for a in (["rev-parse", "HEAD"], ["branch", "--list"], ["status", "--porcelain"])]


@contextlib.contextmanager
def throwaway_repo():
    """chdir into a fresh repo with one commit and no inherited GIT_* variables; restore both on exit."""
    saved_cwd, saved_env = os.getcwd(), {k: os.environ.pop(k) for k in list(os.environ) if k.startswith("GIT_")}
    try:
        with tempfile.TemporaryDirectory() as d:
            os.chdir(d)
            sh("git", "init", "-qb", "main")
            sh("git", "config", "user.email", "t@t")
            sh("git", "config", "user.name", "t")
            sh("git", "config", "commit.gpgsign", "false")
            sh("git", "commit", "-q", "--allow-empty", "-m", "init")
            yield
    finally:
        os.chdir(saved_cwd)
        os.environ.update(saved_env)


def isolated(test):
    """Run `test` in a throwaway repo, and fail it if the repo it was started from changed. A test may
    replace `rw.resolve`; it is put back after."""
    @functools.wraps(test)
    def run():
        before, resolve = state(os.getcwd()), rw.resolve
        try:
            with throwaway_repo():
                test()
        finally:
            rw.resolve = resolve
        assert state(os.getcwd()) == before, "the test changed the repo it was started from"
    return run


def sh(*cmd):
    return subprocess.run(cmd, check=True, capture_output=True, text=True).stdout.strip()


def commit(files, msg):
    for name, text in files.items():
        Path(name).write_text(text)
    sh("git", "add", "-A")
    sh("git", "commit", "-qm", msg)
    return sh("git", "rev-parse", "HEAD")


@isolated
def test_since_view_hides_base_merges():
    base1 = commit({"a.ts": "one\ntwo\nthree\nfour\nfive\n", "b.ts": "x\n"}, "base")
    sh("git", "switch", "-qc", "feature")
    head1 = commit({"a.ts": "one\nTWO\nthree\nfour\nfive\n", "b.ts": "x\ny\n"}, "pr change")
    sh("git", "switch", "-q", "main")
    base2 = commit({"a.ts": "one\ntwo\nthree\nfour\nFIVE\n"}, "develop change")
    sh("git", "switch", "-q", "feature")
    sh("git", "merge", "-q", "--no-edit", "main")
    head2 = commit({"b.ts": "x\ny\nz\n"}, "second pr change")

    prev = {"target": {"base": base1}, "viewed": {"a.ts": head1, "b.ts": head1}}
    now = {"base": base2, "head": head2}
    assert rw.since_view(prev, now, "a.ts") == "", "a develop-only change must not show as changed since view"
    b = rw.since_view(prev, now, "b.ts")
    assert "+z" in b and "+y" not in b, b


@isolated
def test_new_round_patch_and_status():
    """A file the reviewer never viewed must reach review-check in the next round, and `status` must
    catch a map that skips a file and a verdict line that review-check did not print."""
    base = sh("git", "rev-parse", "HEAD")
    head1 = commit({"p.ts": "one\n", "q.ts": "q\n"}, "round 1")
    head2 = commit({"p.ts": "one\ntwo\n", "r.ts": "r\n"}, "round 2")
    heads = iter([head1, head2])
    rw.resolve = lambda target: ("pr-9", {"kind": "range", "base": base, "head": next(heads)})
    with redirect_stdout(io.StringIO()):
        rw.start("pr")
    first = rw.home() / "pr-9-01"
    (first / "viewed.json").write_text(json.dumps({"viewed": {"p.ts": head1}}))  # q.ts was never viewed
    with redirect_stdout(io.StringIO()):
        rw.start("pr")
    d = rw.home() / "pr-9-02"
    patch = (d / "since-view.patch").read_text()
    assert "+two" in patch and "+one" not in patch, "p.ts shows only what changed after the view"
    assert "b/q.ts" in patch and "b/r.ts" in patch, "q.ts and r.ts are new to the reviewer"

    (d / "02-map.md").write_text("Status: done x\nUses: 01-scope@x\n\n`p.ts` `q.ts` `q.ts`\n")
    rc = Path("tmp/review-check/range-x-01")
    rc.mkdir(parents=True)
    (rc / "03-verdict.md").write_text("Status: done y\nUses: x\nVerdict: REQUEST CHANGES (1 fix)\n")
    (rc / "target.json").write_text(json.dumps({"head": head2}))
    (d / "03-check.md").write_text(f"Status: done x\nUses: 01-scope@x\nVerdict: APPROVE (0 fix)\nRun: {rc}\n")
    state = {s: (d / f"{s}.md").read_text().splitlines()[0].removeprefix("Status: ") for s in rw.STEPS}
    problems = rw.checks(d, state)
    assert any("not mapped r.ts" in p and "mapped twice q.ts" in p for p in problems), problems
    assert any("line 3 is not line 3" in p for p in problems), problems
    (d / "03-check.md").write_text(f"Status: done x\nUses: 01-scope@x\nVerdict: REQUEST CHANGES (1 fix)\nRun: {rc}\n")
    assert not any(p.startswith("03-check") for p in rw.checks(d, state)), "the copied line matches"
    (d / "02-map.md").write_text("Status: done x\nUses: 01-scope@x\n\n`p.ts` `q.ts`\n```mermaid\nflowchart LR\n  a-->b\n```\n- `r.ts`, see `r.ts:3`\n")
    assert not any(p.startswith("02-map") for p in rw.checks(d, state)), "a diagram and citations do not hide a mapped file"


@isolated
def test_secret_pattern():
    assert rw.SECRET.search("token ghp_" + "a" * 36) and not rw.SECRET.search("a GitHub token in plain text")


@isolated
def test_anchor_uses_new_side_lines():
    patch = ("diff --git a/a.ts b/a.ts\n--- a/a.ts\n+++ b/a.ts\n@@ -1,3 +1,4 @@\n one\n-two\n+TWO\n+extra\n three\n")
    assert rw.anchor(patch, "a.ts", "TWO") == (None, 2)
    assert rw.anchor(patch, "a.ts", "TWO", "three") == (2, 4)


@isolated
def test_four_checks_on_main():
    instruction = Path(__file__).resolve().parents[1] / "references" / "walk.md"
    assert "--parent-loop review-walk:<run>" in instruction.read_text(), "Check step must pass the run's key"
    with patch.object(rc, "load_rules", return_value=[]):
        for n in range(4):
            base = sh("git", "rev-parse", "HEAD")
            head = commit({"value.py": f"def value():\n    return {n}\n"}, f"PR {n}")
            with redirect_stdout(io.StringIO()) as out:
                rw.start(f"{base}..{head}")
            walk = Path(out.getvalue().strip())
            with redirect_stdout(io.StringIO()) as out:
                rc.start(f"{base}..{head}", "--parent-loop", f"review-walk:{walk.name}")
            review = Path(out.getvalue().splitlines()[0])
            metadata = json.loads((review / "target.json").read_text())
            assert metadata["parent_loop"] == f"review-walk:{walk.name}" and metadata["round"] == 1, metadata
            unit = json.loads((review / "units.json").read_text())[0]
            (review / "02-review.md").write_text(
                f"| {unit['id']} | - | value.py:2 | `return {n}` | pass | | the caller accepts this value |\n")
            with redirect_stdout(io.StringIO()):
                assert rc.verdict(review.name) == 0
        assert sh("git", "branch", "--show-current") == "main"


if __name__ == "__main__":
    for t in (test_since_view_hides_base_merges, test_new_round_patch_and_status, test_secret_pattern,
              test_anchor_uses_new_side_lines, test_four_checks_on_main):
        t()
    print("ok")
