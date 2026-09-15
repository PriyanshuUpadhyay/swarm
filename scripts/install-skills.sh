#!/bin/sh
# Link this repo's skills into every agent CLI on this machine. The repo stays the single source.
#   $HOME/.agents/skills/<name>        -> <repo>/skills/<name>   Codex global root, and the hub
#   $HOME/.claude/skills/<name>        -> ../../.agents/skills/<name>
#   $HOME/.gemini/config/skills/<name> -> ../../../.agents/skills/<name>   AGY global root
# Re-run after a clone moves; idempotent.
set -eu

repo=$(cd "$(dirname "$0")/.." && pwd -P)
hub=$HOME/.agents/skills
mkdir -p "$hub" "$HOME/.claude/skills" "$HOME/.gemini/config/skills"
for skill in "$repo"/skills/*/; do
    name=$(basename "$skill")
    ln -sfn "$repo/skills/$name" "$hub/$name"
    ln -sfn "../../.agents/skills/$name" "$HOME/.claude/skills/$name"
    ln -sfn "../../../.agents/skills/$name" "$HOME/.gemini/config/skills/$name"
    echo "linked $name"
done
