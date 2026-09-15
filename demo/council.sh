#!/bin/sh
# S5 proof: three voices answer a question, an interrupted child is summarized, a killed pane is swept.
# Run inside tmux: cargo build && SWARM=target/debug/swarm sh demo/council.sh
set -eu

SWARM=$(command -v "${SWARM:-swarm}")
case $SWARM in /*) ;; *) SWARM=$PWD/$SWARM ;; esac   # children run in their own panes
SWARM_HOME=$(mktemp -d)
export SWARM_HOME
export SWARM_ADAPTER=tmux
"$SWARM" init
SWARM_SESSION_ID=$("$SWARM" session new lane)
export SWARM_SESSION_ID
export SWARM_AGENT_ID=orchestrator
"$SWARM" agent add orchestrator orchestrator
echo "session $SWARM_SESSION_ID in $SWARM_HOME"

# Each voice waits for a message, then answers with finish. The pane has SWARM_SESSION_ID and
# SWARM_AGENT_ID stamped by spawn; the binary path is baked in because PATH may not carry it.
VOICES="reviewer tester architect"
for voice in $VOICES; do
    "$SWARM" spawn "$voice" voice -- sh -c "until $SWARM inbox | grep -q .; do sleep 1; done; echo \"$voice says: ship it\" | $SWARM finish"
done
for voice in $VOICES; do
    echo "Is the parser ready to ship? Answer in one line." | "$SWARM" send "$voice" ask
done

# Wait for one summary per voice, print each body, and ack it.
until [ "$("$SWARM" inbox | grep -c ' summary ')" -eq 3 ]; do sleep 1; done
"$SWARM" inbox | while read -r seq sender kind body; do
    echo "$sender ($kind): $(cat "$SWARM_HOME/.swarm/$body")"
    "$SWARM" ack "$seq"
done
