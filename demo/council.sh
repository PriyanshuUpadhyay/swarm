#!/bin/sh
# S5 proof: three voices answer a question, an interrupted child is summarized, a killed pane is swept.
# Run inside tmux: cargo build && SWARM=target/debug/swarm sh demo/council.sh
set -eu

SWARM=$(command -v "${SWARM:-swarm}")
case $SWARM in /*) ;; *) SWARM=$PWD/$SWARM ;; esac   # children run in their own panes
export SWARM_HOME=$(mktemp -d)
export SWARM_ADAPTER=tmux
"$SWARM" init
SWARM_SESSION_ID=$("$SWARM" session new lane)
export SWARM_SESSION_ID
export SWARM_AGENT_ID=orchestrator
"$SWARM" agent add orchestrator orchestrator
echo "session $SWARM_SESSION_ID in $SWARM_HOME"
