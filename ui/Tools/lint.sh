#!/bin/zsh
set -euo pipefail

root="${0:A:h:h}"
failed=0
# rg exits 0 on a match, 1 on none, and 2 on an error (a missing path, a bad pattern); an error
# fails lint, so a moved folder cannot turn a guard into a silent pass.
check() {
  local code=0
  rg -n --glob '*.swift' "$@" || code=$?
  case $code in
    0) failed=1 ;;
    1) ;;
    *) print -u2 "lint: rg failed with exit $code"; exit 2 ;;
  esac
}
check '^import (SwiftUI|AppKit|Cocoa|SwiftTerm)$' \
    "$root/Sources/SwarmCore" "$root/Sources/TranscriptTool"
check 'Process\(\)|CapturedProcess|Shell\.run' \
    "$root/Sources/Swarm"
# App state is installed after init; startup must read it from the root scene.
check --multiline --pcre2 \
    '(?s)^struct SwarmApp: App \{.*?\n    init\(\) \{(?:(?!\n    \}).)*\b(settings|model)\b' \
    "$root/Sources/Swarm/SwarmApp.swift"
# Each surface folder is one module: it names no app store and no other surface's entry view.
stores='SwarmSession|SessionsTreeModel|AgentPaneStore|SessionDetail'
typeset -A surfaces=(
  Composer ComposerView PaneStrip 'PaneStrip\b' Sidebar SidebarView
  Tabs ChatTabsView Transcript TranscriptView Palette CommandPalette Prompt PromptCard Design '^$'
  Profiles ProfilesPage Accounts AccountsPage Managed ManagedChangesPage Runs StepRunsView Skills SkillsView Settings AdvancedSettingsPage Home HomeView
)
for folder entry in ${(kv)surfaces}; do
  others=(${(v)surfaces:#$entry})
  others=(${others:#'^$'})
  check -e "$stores" -e "(${(j:|:)others})" "$root/Sources/Swarm/$folder"
done
# Views take spacing, radii, colors, and font sizes from Design/DesignTokens.swift; 0 is allowed.
check --glob '!**/Design/**' \
    -e '\.padding\((\.[a-zA-Z]+, )?[1-9]' -e 'spacing: [1-9]' -e 'cornerRadius: [1-9]' \
    -e 'lineWidth: [1-9]' -e 'Color\(red:' -e '\.system\(size:' -e 'opacity\(0\.' \
    "$root/Sources/Swarm"
# A count's plural comes from inflection, ^[\(n) word](inflect: true), not from appending "s".
check '== 1 \? "" : "s"' "$root/Sources"
exit "$failed"
