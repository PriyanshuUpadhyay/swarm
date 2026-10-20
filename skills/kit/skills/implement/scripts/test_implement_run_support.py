# Implement test support cases and helpers.
import fcntl
import hashlib
import json
import os
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
from contextlib import redirect_stdout
from io import StringIO
from pathlib import Path
from unittest.mock import patch
import implement_run  # noqa: E402
import block_resolver  # noqa: E402

SCRIPT = Path(__file__).resolve().with_name("implement_run.py")  # the kit path, also when run through a linked skill folder
CALLERS_MEMORY_LIMIT = 1_000_000


def fill(f, evidence="evidence"):
    """Check every todo of a step file with evidence, as an agent would."""
    f.write_text("".join(l.replace("- [ ]", "- [x]").rstrip() + (f" {evidence}\n" if l.startswith("- [") and l.rstrip().endswith(":") else "\n")
                         for l in f.read_text(encoding="utf-8").splitlines()), encoding="utf-8")


# Every git call and implement run in this file: no global or system git config (signing, hooks, aliases), a fixed author,
# and no GIT_* variable of the caller, such as the GIT_DIR a git hook sets. Build each env with git_env().
GIT_ENV = {"GIT_CONFIG_GLOBAL": os.devnull, "GIT_CONFIG_NOSYSTEM": "1", "GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@t",
           "GIT_COMMITTER_NAME": "t", "GIT_COMMITTER_EMAIL": "t@t"}


def git_env():
    return {**{k: v for k, v in os.environ.items() if not k.startswith("GIT_")}, **GIT_ENV}

PARAMS = '[params]\nsource_of_truth = "docs/spec.md"\nsite = "site"\nshots = "tmp/shots"\n'
PROVE_BUILD = "- build: block:prove-it-works\n"  # the catalog line most block tests start from


def blocks_start(tmp, name, catalog, repo_toml=PARAMS, *args, env=None, python=sys.executable):
    """Start an implement run in a new repo whose catalog `any` section has the given step lines, with the kit's blocks."""
    home, repo = Path(tmp, name, "home"), Path(tmp, name, "app")
    (home / ".flow" / "app").mkdir(parents=True)
    (home / ".flow" / "domains.md").write_text("## any\n" + catalog, encoding="utf-8")
    (home / ".flow" / "app" / "profile.md").write_text("Domains: any\nRepo rules: none\n", encoding="utf-8")
    repo.mkdir(exist_ok=True)
    subprocess.run(["git", "init", "-q", "-b", "main"], cwd=repo, env=git_env(), check=True)
    if repo_toml is not None:
        (repo / ".blocks.toml").write_text(repo_toml, encoding="utf-8")
    run = lambda *a: subprocess.run([python, SCRIPT, *map(str, a)], cwd=repo,
                                    env={**git_env(), "HOME": str(home), **(env or {})}, capture_output=True, text=True)
    out = run("start", *args)
    folder = Path(out.stdout.splitlines()[0]) if out.returncode == 0 else None
    return out, folder, repo, run


def step_text(folder, step):
    return (folder / f"{step}.md").read_text(encoding="utf-8")


def has_control(text, keep="\t\n"):
    """True if text holds a C0 or C1 control character or DEL, other than those in keep."""
    return any(ord(c) < 32 and c not in keep or 127 <= ord(c) <= 159 for c in text)


def rule_lines(text):
    """The rule, todo, and check lines of a step file, without Example sections."""
    lines, example = [], False
    for line in text.splitlines():
        if line.startswith(("## ", "### ")):  # a step section or a block section
            example = line == "### Example"
        elif line.startswith("- ") and not example:
            lines.append(line)
    return sorted(lines)


# An ASCII locale: the child opens files as US-ASCII unless the code passes encoding="utf-8". The child
# runs without -I, which would ignore PYTHONUTF8=0 and turn on UTF-8 mode under the C locale.
ASCII_ENV = {"LC_ALL": "C", "PYTHONUTF8": "0", "PYTHONCOERCECLOCALE": "0"}


def assert_ascii_locale():
    out = subprocess.run([sys.executable, "-c", "print(open(__import__('os').devnull).encoding)"],
                         env={**os.environ, **ASCII_ENV}, capture_output=True, text=True)
    assert out.stdout.strip().lower() in ("ascii", "us-ascii", "ansi_x3.4-1968"), f"not an ASCII locale: {out.stdout}"


def block_rg(name, site):
    """Run each `rg` command of a block's Check (script) section on `site`, as written. An rg error (exit 2)
    fails the test, so a row that must stay clean cannot pass on an error."""
    text = (implement_run.BLOCKS / name / "SKILL.md").read_text(encoding="utf-8").split("## Check (script)\n", 1)[1].split("\n## ")[0]
    runs = [subprocess.run(c.replace("{{site}}", shlex.quote(str(site))), shell=True, capture_output=True, text=True)
            for c in re.findall(r"`([^`]+)`", text) if c.startswith("rg ")]
    for out in runs:
        assert out.returncode in (0, 1), (out.args, out.stderr)
    return runs
