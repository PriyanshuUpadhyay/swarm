#!/bin/zsh
# Starts a real CLI chat, uses it, restarts it, and says PASS or FAIL. See `SmokeChat`.
#
# usage: Tools/chat-smoke.sh [directory] [--agent codex|claudeCode] [--model <name>]
#
# It spends three short turns of a real model, so it is run by hand before an install.
set -eu
root=${0:a:h:h}
directory=${1:-$PWD}
[[ $# -gt 0 ]] && shift
swift build --package-path "$root" --product Swarm >/dev/null
db=$(mktemp -d)/smoke.sqlite
SWARM_UI_DB_PATH=$db "$root/.build/debug/Swarm" --smoke-chat "$directory" "$@"
