#!/bin/zsh
set -euo pipefail

root="${0:A:h:h}"
failed=0
if rg -n --glob '*.swift' '^import (SwiftUI|AppKit|Cocoa|SwiftTerm)$' \
    "$root/Sources/SwarmCore" "$root/Sources/TranscriptTool"; then
  failed=1
fi
if rg -n --glob '*.swift' 'Process\(\)|CapturedProcess|Shell\.run' \
    "$root/Sources/Swarm"; then
  failed=1
fi
if rg -n --glob '*.swift' \
    'SwarmSession|SessionsTreeModel|AgentPaneStore|SessionDetail' \
    "$root/Sources/Swarm/Composer"; then
  failed=1
fi
exit "$failed"
