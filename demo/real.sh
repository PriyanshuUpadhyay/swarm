#!/bin/sh
# S5 proof with a live agent: one Claude CLI voice answers a question through swarm.
# Run from a Herdr pane: cargo build && SWARM=target/debug/swarm sh demo/real.sh
set -eu

SWARM=$(command -v "${SWARM:-swarm}")
case $SWARM in /*) ;; *) SWARM=$PWD/$SWARM ;; esac
PATH=$(dirname "$SWARM"):$PATH   # the voice calls `swarm` by name
export PATH
SWARM_HOME=$(mktemp -d)
export SWARM_HOME
export SWARM_ADAPTER=herdr
swarm init
SWARM_SESSION_ID=$(swarm session new lane)
export SWARM_SESSION_ID
export SWARM_AGENT_ID=orchestrator
swarm agent add orchestrator orchestrator
echo "session $SWARM_SESSION_ID in $SWARM_HOME"
