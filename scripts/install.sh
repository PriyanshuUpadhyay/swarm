#!/bin/sh
# Link this repo's skills and host files into every agent CLI on this machine. The repo stays the
# single source, so an edit here is live at once and nothing else can drift from it.
#   $HOME/.agents/skills/<name>        -> <repo>/skills/<name>   Codex global root, and the hub
#   $HOME/.claude/skills/<name>        -> ../../.agents/skills/<name>
#   $HOME/.gemini/config/skills/<name> -> ../../../.agents/skills/<name>   AGY global root
#   $HOME/.claude/scripts/agent-host-context.py -> <repo>/scripts/agent-host-context.py
#       the SessionStart hook in ~/.claude/settings.json and ~/.codex/hooks.json runs this path
#   $HOME/.config/herdr/bin/swarm-split.py -> <repo>/adapters/swarm-split.py
#       the herdr adapter's spawn verb runs this path
# Run it from the main checkout: every link points into the checkout that runs it, and a feature
# worktree is gone after merge. Re-run after a clone moves; idempotent.
set -eu

repo=$(cd "$(dirname "$0")/.." && pwd -P)
branch=$(git -C "$repo" rev-parse --abbrev-ref HEAD)
if [ "$branch" != main ]; then
    echo "install.sh: $repo is on $branch; run it from the main checkout" >&2
    exit 1
fi
hub=$HOME/.agents/skills
mkdir -p "$hub" "$HOME/.claude/skills" "$HOME/.gemini/config/skills" \
    "$HOME/.claude/scripts" "$HOME/.config/herdr/bin"
for skill in "$repo"/skills/*/; do
    name=$(basename "$skill")
    ln -sfn "$repo/skills/$name" "$hub/$name"
    ln -sfn "../../.agents/skills/$name" "$HOME/.claude/skills/$name"
    ln -sfn "../../../.agents/skills/$name" "$HOME/.gemini/config/skills/$name"
    echo "linked $name"
done
ln -sfn "$repo/scripts/agent-host-context.py" "$HOME/.claude/scripts/agent-host-context.py"
ln -sfn "$repo/adapters/swarm-split.py" "$HOME/.config/herdr/bin/swarm-split.py"
echo "linked agent-host-context.py swarm-split.py"
