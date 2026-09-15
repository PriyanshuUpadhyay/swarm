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

# The voice is an interactive Claude CLI. The protocol rides in the system prompt, so the ring
# text `swarm: new message` is its first prompt. Bash is allowed only for swarm and echo.
PROTOCOL="You are agent reviewer in a swarm session. When you receive the prompt 'swarm: new message': run \`swarm inbox\` (each line is: seq sender kind body_path), Read the body file at $SWARM_HOME/.swarm/<body_path>, answer the question in one line with \`echo '<answer>' | swarm finish\`, then run \`swarm ack <seq>\`. Do nothing else and do not ask questions."
swarm spawn reviewer voice -- claude --append-system-prompt "$PROTOCOL" --allowedTools 'Bash(swarm:*)' 'Bash(echo:*)'
