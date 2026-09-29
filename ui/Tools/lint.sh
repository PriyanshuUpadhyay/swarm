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
    "$root/Sources/Swarm/Composer" "$root/Sources/Swarm/PaneStrip" "$root/Sources/Swarm/Design" \
    "$root/Sources/Swarm/Sidebar" "$root/Sources/Swarm/Tabs" "$root/Sources/Swarm/Transcript"; then
  failed=1
fi
# Views take spacing, radii, colors, and font sizes from Design/DesignTokens.swift; 0 is allowed.
if rg -n --glob '*.swift' --glob '!**/Design/**' \
    -e '\.padding\((\.[a-zA-Z]+, )?[1-9]' -e 'spacing: [1-9]' -e 'cornerRadius: [1-9]' \
    -e 'lineWidth: [1-9]' -e 'Color\(red:' -e '\.system\(size:' -e 'opacity\(0\.' \
    "$root/Sources/Swarm"; then
  failed=1
fi
exit "$failed"
