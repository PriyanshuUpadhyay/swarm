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
    assert out.returncode == 1 and out.stdout.strip() == "raise: 101 lines changed (small allows 100)", out
    out = case(tmp, "schema", {"a.py": 1, "db/schema.sql": 1})
    assert out.returncode == 1 and out.stdout.strip() == "raise: db/schema.sql is a schema or migration path", out
    out = case(tmp, "migration", {"migrations/0002_add.py": 3})
    assert out.returncode == 1 and "migrations/0002_add.py is a schema or migration path" in out.stdout, out
    bad = subprocess.run([sys.executable, "-I", str(GATE)], capture_output=True, text=True)
    assert bad.returncode != 0 and "usage" in bad.stderr, bad
print("ok")
