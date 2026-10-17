#!/bin/sh
# List merged, clean linked worktrees. --yes removes them and their branches.
# Usage: scripts/prune-worktrees.sh [--yes]
set -eu

case "$#:${1:-}" in
    0:|1:--yes) ;;
    *) printf '%s\n' 'usage: scripts/prune-worktrees.sh [--yes]' >&2; exit 1 ;;
esac
root=$(git -C "$(dirname "$0")/.." rev-parse --show-toplevel)
common=$(git -C "$root" rev-parse --path-format=absolute --git-common-dir)
git --git-dir="$common" fetch --prune origin
exec python3 -I - "$common" "${1:-}" <<'PY'
from pathlib import Path
import subprocess
import sys

common = sys.argv[1]
apply = sys.argv[2] == "--yes"

def git(*args, check=True):
    return subprocess.run(["git", f"--git-dir={common}", *args], cwd=common,
                          check=check, capture_output=True, text=True)

def merged(ref):
    result = git("merge-base", "--is-ancestor", ref, "origin/main", check=False)
    if result.returncode not in (0, 1):
        raise RuntimeError(result.stderr.strip())
    return result.returncode == 0

def keep_reason(tree):
    path = tree["worktree"]
    branch = tree.get("branch", "")
    if "bare" in tree:
        return "bare repository"
    if tree is trees[0] or branch == "refs/heads/main":
        return "main worktree"
    if "locked" in tree:
        return "locked worktree"
    if "prunable" in tree or not Path(path).is_dir():
        return "missing worktree; inspect it by hand"
    if not branch:
        return "detached HEAD"
    if tree["HEAD"] in mainline:
        return "no commits yet"
    if not merged(branch):
        return "unmerged commits"
    remote = "refs/remotes/origin/" + branch.removeprefix("refs/heads/")
    if git("show-ref", "--verify", "--quiet", remote, check=False).returncode == 0 and not merged(remote):
        return "unmerged remote commits"
    status = subprocess.run(["git", "-C", path, "status", "--porcelain", "--untracked-files=all",
                             "--", ".", ":(exclude)tmp"], cwd=common, capture_output=True, text=True)
    if status.returncode:
        return "cannot read tree status"
    if status.stdout:
        return "changes outside tmp/"
    return None

try:
    git("rev-parse", "--verify", "origin/main")
    mainline = set(git("rev-list", "--first-parent", "origin/main").stdout.splitlines())
    trees = []
    for record in git("worktree", "list", "--porcelain", "-z").stdout.strip("\0").split("\0\0"):
        tree = {}
        for field in record.split("\0"):
            key, _, value = field.partition(" ")
            tree[key] = value
        trees.append(tree)
    candidates = []
    for tree in trees:
        reason = keep_reason(tree)
        path = tree["worktree"]
        if reason:
            print(f"KEEP {path} — {reason}", flush=True)
        else:
            branch = tree["branch"].removeprefix("refs/heads/")
            print(f"REMOVE {path} — merged branch {branch}, no changes outside tmp/", flush=True)
            candidates.append(tree)
    if not apply:
        print(f"Listed {len(candidates)} worktree(s); no worktree or branch was removed.")
        sys.exit(0)
    for tree in candidates:
        reason = keep_reason(tree)
        if reason:
            raise RuntimeError(f"{tree['worktree']} changed since listing: {reason}")
        branch = tree["branch"].removeprefix("refs/heads/")
        remote_ref = "refs/heads/" + branch
        remote = git("ls-remote", "--exit-code", "--heads", "origin", remote_ref, check=False)
        if remote.returncode not in (0, 2):
            raise RuntimeError(remote.stderr.strip())
        remote_tip = remote.stdout.split()[0] if remote.stdout else ""
        if remote_tip and not merged(remote_tip):
            raise RuntimeError(f"remote branch {branch} now has unmerged commits")
        # --force allows disposable tmp/ files. The status check protects other changes.
        git("worktree", "remove", "--force", tree["worktree"])
        # -d checks HEAD or the upstream, but this script checks origin/main.
        git("branch", "-D", "--", branch)
        if remote_tip:
            git("push", f"--force-with-lease={remote_ref}:{remote_tip}", "origin", f":{remote_ref}")
        print(f"Removed {tree['worktree']} and branch {branch}" + (" on origin" if remote_tip else ""), flush=True)
except (subprocess.CalledProcessError, RuntimeError) as error:
    message = error.stderr if isinstance(error, subprocess.CalledProcessError) else str(error)
    print(f"prune-worktrees: {message.strip()}", file=sys.stderr)
    sys.exit(1)
PY
