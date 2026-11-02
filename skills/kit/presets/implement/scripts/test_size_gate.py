"""Run: python3 -I test_size_gate.py. The size gate keeps a 2-file change small and raises a large or schema one."""
import os
import subprocess
import sys
import tempfile
from pathlib import Path

GATE = Path(__file__).resolve().parent / "size_gate.py"


def case(tmp, name, files):
    """A repo with one base commit and `files` ({path: lines}) changed after it; returns the gate's output."""
    repo = Path(tmp, name)
    repo.mkdir()
    env = {**os.environ, "GIT_CONFIG_GLOBAL": os.devnull, "GIT_CONFIG_NOSYSTEM": "1",
           "GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@t", "GIT_COMMITTER_NAME": "t", "GIT_COMMITTER_EMAIL": "t@t"}
    git = lambda *a: subprocess.run(["git", *a], cwd=repo, env=env, check=True, capture_output=True, text=True).stdout.strip()
    git("init", "-q", "-b", "main")
    (repo / "README.md").write_text("base\n")
    git("add", ".")
    git("commit", "-q", "-m", "base")
    base = git("rev-parse", "HEAD")
    committed = dict(list(files.items())[:-1])
    for path, lines in committed.items():
        (repo / path).parent.mkdir(parents=True, exist_ok=True)
        (repo / path).write_text("".join(f"line {i}\n" for i in range(lines)))
    if committed:
        git("add", ".")
        git("commit", "-q", "-m", "change")
    # The last file stays untracked, so the gate must count work that is not committed yet.
    path, lines = list(files.items())[-1]
    (repo / path).parent.mkdir(parents=True, exist_ok=True)
    (repo / path).write_text("".join(f"line {i}\n" for i in range(lines)))
    return subprocess.run([sys.executable, "-I", str(GATE), base], cwd=repo, env=env, capture_output=True, text=True)


with tempfile.TemporaryDirectory() as tmp:
    out = case(tmp, "two", {"a.py": 10, "b.py": 20})
    assert out.returncode == 0 and out.stdout.strip() == "small: 2 files, 30 lines", out
    out = case(tmp, "five", {f"f{i}.py": 2 for i in range(5)})
    assert out.returncode == 1 and out.stdout.strip() == "raise: 5 files changed (small allows 3)", out
    out = case(tmp, "long", {"a.py": 60, "b.py": 41})
    assert out.returncode == 1 and out.stdout.strip() == "raise: 101 non-test lines changed (small allows 100)", out
    # Test lines do not count toward the line limit (trial 6: 41 code lines and 128 test lines raised).
    out = case(tmp, "tests", {"tasks.py": 41, "test_tasks.py": 128})
    assert out.returncode == 0 and out.stdout.strip() == "small: 2 files, 41 lines (+128 test lines)", out
    out = case(tmp, "testdir", {"src/a.py": 30, "tests/helpers.py": 90, "web/a.spec.ts": 50})
    assert out.returncode == 0 and out.stdout.strip() == "small: 3 files, 30 lines (+140 test lines)", out
    # Swift keeps tests in Tests/; a spec/ folder may hold an API spec, which is code (round 5).
    out = case(tmp, "swift", {"Sources/A.swift": 10, "Tests/ATests/Fixture.swift": 120})
    assert out.returncode == 0 and out.stdout.strip() == "small: 2 files, 10 lines (+120 test lines)", out
    out = case(tmp, "spec", {"src/a.py": 10, "spec/openapi.yaml": 500})
    assert out.returncode == 1 and "510 non-test lines" in out.stdout, out
    out = case(tmp, "cache", {"a.py": 5, "__pycache__/a.cpython-312.pyc": 3})
    assert out.returncode == 0 and out.stdout.strip() == "small: 1 files, 5 lines", out
    out = case(tmp, "notatest", {"contest.py": 101})
    assert out.returncode == 1 and "101 non-test lines" in out.stdout, "a name that only contains test is code"
    out = case(tmp, "schema", {"a.py": 1, "db/schema.sql": 1})
    assert out.returncode == 1 and out.stdout.strip() == "raise: db/schema.sql is a schema or migration path", out
    out = case(tmp, "migration", {"migrations/0002_add.py": 3})
    assert out.returncode == 1 and "migrations/0002_add.py is a schema or migration path" in out.stdout, out
    # A committed path with an accent still meets the schema globs, because the diff is read with -z.
    out = case(tmp, "accent", {"db/données.sql": 1, "a.py": 1})
    assert out.returncode == 1 and "db/données.sql is a schema or migration path" in out.stdout, out
    out = case(tmp, "escape", {"a\x1b[2Jschema.txt": 1})
    assert out.returncode == 1 and out.stdout.strip() == "raise: a\\x1b[2Jschema.txt is a schema or migration path", out
    out = case(tmp, "escape-c1", {"c1\x9b[2Jschema.txt": 1})
    assert out.returncode == 1 and out.stdout.strip() == "raise: c1\\x9b[2Jschema.txt is a schema or migration path", out
    # A new binary file counts 0 lines, as git counts it after the commit.
    out = case(tmp, "binary", {"a.py": 1, "shot.png": 0})
    Path(tmp, "binary", "shot.png").write_bytes(bytes(range(256)) * 80)
    out = subprocess.run([sys.executable, "-I", str(GATE), "HEAD~1"], cwd=Path(tmp, "binary"), capture_output=True, text=True)
    assert out.returncode == 0 and out.stdout.strip() == "small: 2 files, 1 lines", out
    # A NUL byte after the first 8000 bytes is text to git, so the lines count; a symlink counts 1 line.
    late = Path(tmp, "binary", "late.txt")
    late.write_bytes(b"x\n" * 4500 + b"\0\n" + b"y\n" * 500)
    out = subprocess.run([sys.executable, "-I", str(GATE), "HEAD~1"], cwd=Path(tmp, "binary"), capture_output=True, text=True)
    assert out.returncode == 1 and out.stdout.strip() == "raise: 5002 non-test lines changed (small allows 100)", out
    late.unlink()
    Path(tmp, "binary", "link").symlink_to("gone")
    out = subprocess.run([sys.executable, "-I", str(GATE), "HEAD~1"], cwd=Path(tmp, "binary"), capture_output=True, text=True)
    assert out.returncode == 0 and out.stdout.strip() == "small: 3 files, 2 lines", out
    Path(tmp, "binary", "link").unlink()
    # A new file that cannot be read raises, because its size is unknown.
    locked = Path(tmp, "binary", "locked.txt")
    locked.write_text("x\n")
    locked.chmod(0)
    out = subprocess.run([sys.executable, "-I", str(GATE), "HEAD~1"], cwd=Path(tmp, "binary"), capture_output=True, text=True)
    locked.chmod(0o644)
    assert out.returncode == 1 and "locked.txt cannot be read" in out.stdout, out
    locked.unlink()
    # A base that starts with - is refused, not passed to git as an option.
    out = subprocess.run([sys.executable, "-I", str(GATE), f"--output={tmp}/x"], cwd=Path(tmp, "binary"), capture_output=True, text=True)
    assert out.returncode == 1 and "not a commit" in out.stderr and not Path(tmp, "x").exists(), out
    bad = subprocess.run([sys.executable, "-I", str(GATE)], capture_output=True, text=True)
    assert bad.returncode != 0 and "usage" in bad.stderr, bad
print("ok")
