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
# Each surface folder is one module: it names no app store and no other surface's entry view.
stores='SwarmSession|SessionsTreeModel|AgentPaneStore|SessionDetail'
typeset -A surfaces=(
  Composer ComposerView PaneStrip 'PaneStrip\b' Sidebar SidebarView
  Tabs ChatTabsView Transcript TranscriptView Palette CommandPalette Prompt PromptCard Design '^$'
  Profiles AgentProfilesHome Managed ManagedChangesPage Runs StepRunsView
)
for folder entry in ${(kv)surfaces}; do
  others=(${(v)surfaces:#$entry})
  others=(${others:#'^$'})
  if rg -n --glob '*.swift' -e "$stores" -e "(${(j:|:)others})" "$root/Sources/Swarm/$folder"; then
    failed=1
  fi
done
# Views take spacing, radii, colors, and font sizes from Design/DesignTokens.swift; 0 is allowed.
if rg -n --glob '*.swift' --glob '!**/Design/**' \
    -e '\.padding\((\.[a-zA-Z]+, )?[1-9]' -e 'spacing: [1-9]' -e 'cornerRadius: [1-9]' \
    -e 'lineWidth: [1-9]' -e 'Color\(red:' -e '\.system\(size:' -e 'opacity\(0\.' \
    "$root/Sources/Swarm"; then
  failed=1
fi
exit "$failed"
