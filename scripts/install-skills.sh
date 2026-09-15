#!/bin/sh
# Link this repo's skills into every agent CLI on this machine.
# Layout: $HOME/.agents/skills/<name> -> <repo>/skills/<name> (the hub), and each agent's skill
# dir links to the hub, so the repo stays the single source: ~/.claude/skills, ~/.codex/skills,
# ~/.gemini/skills. Re-run after a clone moves; idempotent.
set -eu

repo=$(cd "$(dirname "$0")/.." && pwd -P)
hub=$HOME/.agents/skills
mkdir -p "$hub"
for skill in "$repo"/skills/*/; do
    name=$(basename "$skill")
    ln -sfn "$repo/skills/$name" "$hub/$name"
    for agent in .claude .codex .gemini; do
        mkdir -p "$HOME/$agent/skills"
        ln -sfn "../../.agents/skills/$name" "$HOME/$agent/skills/$name"
    done
    echo "linked $name"
done
