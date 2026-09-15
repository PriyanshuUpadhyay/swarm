#!/bin/sh
# S5 proof with a live agent: one agent CLI voice answers a question through swarm.
# Run from a Herdr pane: cargo install --path . && VOICE=claude|codex|agy sh demo/real.sh
set -eu

command -v swarm >/dev/null || { echo "swarm is not on PATH; run: cargo install --path ." >&2; exit 1; }

# The voice CLI. Model and approval flags mirror the Herdr roles. VOICE_CMD overrides the
# whole line, split on spaces with globbing off, so a token like Bash(x:*) stays literal.
case ${VOICE:=claude} in
    claude) set -- claude --allowedTools 'Bash(swarm:*)' 'Bash(printf:*)' 'Bash(echo:*)' ;;
    codex) set -- codex --model gpt-6-astra -c 'model_reasoning_effort="high"' --sandbox workspace-write --ask-for-approval never ;;
    agy) set -- agy --model gemini-3.8-flash-high --effort high ;;   # accept-edits: skip-permissions keeps Bash sandboxed
    *) echo "usage: VOICE=claude|codex|agy sh demo/real.sh" >&2; exit 1 ;;
esac
if [ -n "${VOICE_CMD:-}" ]; then
    set -f
    # shellcheck disable=SC2086
    set -- $VOICE_CMD
    set +f
fi

# The home lives inside the workspace so a workspace-write sandbox (codex) can use it.
SWARM_HOME=$(mktemp -d "$PWD/.swarm-demo.XXXXXX")
export SWARM_HOME
export SWARM_ADAPTER=herdr
swarm init
SWARM_SESSION_ID=$(swarm session new lane)
export SWARM_SESSION_ID
export SWARM_AGENT_ID=orchestrator
swarm agent add orchestrator orchestrator
echo "session $SWARM_SESSION_ID in $SWARM_HOME, voice $1"

PANE=$(swarm spawn reviewer voice -- "$@")
trap 'swarm close reviewer; rm -rf "$SWARM_HOME"' EXIT
echo "pane $PANE"

# The voice gets the protocol from the swarm-voice skill. The pointer is typed into the pane as
# the first prompt once the CLI is in the foreground, because AGY drops a positional prompt and
# Codex has no system-prompt flag. The pane already carries the swarm env from spawn.
tries=0
until herdr pane process-info --pane "$PANE" | grep -qi "\"cmdline\":\"[^\"]*$1" || [ "$tries" -ge 30 ]; do
    sleep 1; tries=$((tries + 1))
done
sleep 3   # let the TUI draw its input box before typing
herdr pane run "$PANE" "You are agent reviewer. Read $PWD/skills/swarm-voice/SKILL.md and follow it. Answer from your own knowledge in one short line. Wait for the prompt 'swarm: new message'." >/dev/null
sleep 5

echo "In one line: what does the swarm CLI do?" | swarm send reviewer ask
waited=0
until swarm inbox | grep -q ' reviewer summary '; do
    [ "$waited" -lt 300 ] || { echo timeout; herdr pane read "$PANE" | tail -40; exit 1; }
    sleep 2; waited=$((waited + 2))
done
swarm inbox | while read -r seq sender kind body; do
    echo "$sender ($kind): $(cat "$SWARM_HOME/.swarm/$body")"
    swarm ack "$seq"
done
