#!/bin/sh
# Run the kit checks, keep full logs, and print one result line per check.
# Usage: scripts/kit-checks.sh [worktree]
set -eu

[ "$#" -le 1 ] || { printf '%s\n' 'usage: scripts/kit-checks.sh [worktree]' >&2; exit 1; }
root=$(git -C "${1:-$(dirname "$0")/..}" rev-parse --show-toplevel)
# subprocess sets cwd without changing the caller's working directory.
exec python3 -I - "$root" <<'PY'
import os
from pathlib import Path
import subprocess
import sys

root = Path(sys.argv[1])
logs = root / "tmp/kit-checks"
logs.mkdir(parents=True, exist_ok=True)
env = dict(os.environ, TMPDIR=str(root / "tmp"))
checks = [
    "skills/kit/scripts/test_block_refs.py",
    "skills/kit/scripts/test_plugin_skills.py",
    "skills/kit/presets/implement/scripts/test_implement_run.py",
    "skills/kit/references/test_step_run.py",
    "skills/kit/blocks/plan-chart/scripts/test_plan_chart.py",
    "skills/kit/blocks/plan-chart/scripts/test_plan_page.py",
    "skills/kit/blocks/plan-review/scripts/test_start_plan.py",
    "skills/kit/skills/swarm/scripts/test_preset.py",
    "skills/kit/presets/review/scripts/test_review_check.py",
    "skills/kit/scripts/test_build_checks.py",
    "skills/kit/presets/review/scripts/test_review_walk.py",
    "skills/kit/presets/review/scripts/test_validate_citations.py",
    "skills/kit/presets/implement/scripts/test_land.py",
    "skills/kit/blocks/web-search/scripts/test_reach.py",
    "scripts/test_owner_scripts.py",
]
# These suites import their sibling modules through PYTHONPATH, which -I ignores.
sibling_imports = {"test_review_walk", "test_validate_citations", "test_land"}
failed = False
for check in checks:
    name = Path(check).stem
    command = [sys.executable, "-I", check]
    check_env = env.copy()
    if name in sibling_imports:
        command = [sys.executable, check]
        check_env["PYTHONPATH"] = str(Path(check).parent)
    log = logs / f"{name}.log"
    with log.open("wb") as output:
        try:
            result = subprocess.run(command, cwd=root, env=check_env, stdout=output, stderr=subprocess.STDOUT)
            code = result.returncode
        except OSError as error:
            output.write(f"Cannot run {check}: {error}\n".encode())
            code = 1
    lines = log.read_text(errors="replace").strip().splitlines()
    tail = lines[-1] if lines else "(no output)"
    print(f"{name} | exit {code} | {tail}", flush=True)
    if code != 0:
        # A CI log keeps no files, so a failure shows the end of its traceback here.
        print("\n".join(lines[-30:]), flush=True)
    failed |= code != 0
sys.exit(1 if failed else 0)
PY
