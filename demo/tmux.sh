#!/bin/sh
# S5 proof with a live agent on tmux: one agent CLI voice answers a question through swarm.
# Run from inside tmux: cargo install --path . && VOICE=claude|codex|agy sh demo/tmux.sh
set -eu

command -v swarm >/dev/null || { echo "swarm is not on PATH; run: cargo install --path ." >&2; exit 1; }
[ -n "${TMUX_PANE:-}" ] || { echo "run this inside tmux; spawn splits \$TMUX_PANE" >&2; exit 1; }

# The voice CLI. Model and approval flags mirror the Herdr roles. VOICE_CMD overrides the
# whole line, split on spaces with globbing off, so a token like Bash(x:*) stays literal.
case ${VOICE:=claude} in
    claude) set -- claude --allowedTools 'Bash(swarm:*)' 'Bash(printf:*)' 'Bash(echo:*)' ;;
    codex) set -- codex --model gpt-6-astra -c 'model_reasoning_effort="high"' --sandbox workspace-write --ask-for-approval never ;;
    agy) set -- agy --model gemini-3.8-flash-high --effort high ;;   # accept-edits: skip-permissions keeps Bash sandboxed
    *) echo "usage: VOICE=claude|codex|agy sh demo/tmux.sh" >&2; exit 1 ;;
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
export SWARM_ADAPTER=tmux
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
# the first prompt once the CLI has replaced the shell as the pane's foreground command.
tries=0
while [ "$tries" -lt 30 ]; do
    case $(tmux display -p -t "$PANE" '#{pane_current_command}') in
        sh|bash|zsh|fish) sleep 1; tries=$((tries + 1)) ;;
        *) break ;;
    esac
done
sleep 3   # let the TUI draw its input box before typing
tmux send-keys -t "$PANE" -l "You are agent reviewer. Read $PWD/skills/swarm-voice/SKILL.md and follow it. Answer from your own knowledge in one short line. Wait for the prompt 'swarm: new message'."
tmux send-keys -t "$PANE" Enter
sleep 5

echo "In one line: what does the swarm CLI do?" | swarm send reviewer ask
waited=0
until swarm inbox | grep -q ' reviewer summary '; do
    [ "$waited" -lt 300 ] || { echo timeout; tmux capture-pane -p -t "$PANE" | tail -40; exit 1; }
    sleep 2; waited=$((waited + 2))
done
swarm inbox | while read -r seq sender kind body; do
    echo "$sender ($kind): $(cat "$SWARM_HOME/.swarm/$body")"
    swarm ack "$seq"
done
