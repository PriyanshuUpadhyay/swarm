---
version: alpha
name: Swarm, direction B
description: Quiet surface, one signature. A work surface for parallel coding agents, with colour kept for agent state.
# Marks in comments: owner = the owner said it, default = the chair's default shown on a board, open = not decided.
colors:
  primary: "#fcd452"        # default (the yellow of today, one job)
  on-primary: "#1d1b18"     # default
  dark:
    surface: "#131211"      # default
    panel: "#1a1917"        # default
    selected: "#24221f"     # default
    row-hover: "#201e1b"    # default (round 5 and 7; the round 6 #1f1d1b was a slip)
    border: "#2d2a26"       # default
    on-surface: "#f3f1ec"   # default (text)
    muted: "#a39f97"        # default
    disabled-text: "#6f6b63"  # default
    state-failed: "#f26d6d"   # default
    state-waiting: "#f2a33a"  # default
    state-done: "#22c55e"     # default
    state-working: "#60a5fa"  # default
    state-ended: "#898984"    # default (also idle; round 8, was #6f6f6b; 5.32 surface, 5.00 panel, 4.51 selected)
    glyph-lit: "#fcd452"      # default
    glyph-unlit: "#45413a"    # default
    pr-quiet-text: "#8d887f"  # default (draft and merged chip text; round 8, was #77726a; 5.31, 4.99, 4.50)
    pr-draft-border: "#716b61"  # default (round 8, was #5a554d; 3.54, 3.33, 3.01)
    menu: "#22201d"           # default
    menu-border: "#3a3731"    # default
    menu-hover: "#2d2a26"     # default (round 8, hovered menu row)
    menu-selected: "#45413a"  # owner (round 21, selected rail item and pill; 1.60 on menu, 1.73 on panel)
    tooltip: "#2c2a26"        # default
    primary-hover: "#fddc6e"  # default
    primary-pressed: "#e8c03e"  # default
    primary-disabled: "#2d2a26" # default
    quiet-hover: "#24221f"    # default
    quiet-pressed: "#2d2a26"  # default
  light:
    surface: "#faf8f4"        # default
    panel: "#f2efe9"          # default
    selected: "#e9e5dc"       # default
    border: "#e0dbd0"         # default
    on-surface: "#1d1b18"     # default (text)
    row-hover: "#ede9e1"      # default (round 8)
    muted: "#6b665e"          # default (round 8, was #6f6a61; 5.37 surface, 4.96 panel, 4.53 selected)
    disabled-text: "#898379"  # default (round 8; no target, 3.54 on surface like dark)
    state-failed: "#b73c35"   # default (round 8, was #b83c35; 5.33, 4.93, 4.50)
    state-waiting: "#955800"  # default (round 8, was #9a5b00; 5.39, 4.98, 4.55)
    state-done: "#16763d"     # default (round 8, was #177a3f; 5.36, 4.96, 4.52)
    state-working: "#2563c9"  # default (5.34, 4.94, 4.51)
    state-ended: "#6b665e"    # default (also idle; round 8, was #6f6a61; 5.37, 4.96, 4.53)
    glyph-lit: "#a37a00"      # default (3.71, 3.43, 3.13)
    glyph-unlit: "#d6cfc2"    # default
    pr-quiet-text: "#6b665d"  # default (round 8, was #857f74; 5.37, 4.97, 4.54)
    pr-draft-border: "#8b8272"  # default (round 8, was #a39c8f; 3.58, 3.31, 3.02)
    menu: "#ffffff"           # default (round 8)
    menu-border: "#d3cec3"    # default (round 8)
    menu-hover: "#e9e5dc"     # default (round 8, hovered menu row)
    menu-selected: "#d3cec3"  # owner (round 21; 1.57 on menu, 1.37 on panel)
    tooltip: "#ffffff"        # default (round 8)
    primary-hover: "#fddc6e"  # default (round 8, same as dark)
    primary-pressed: "#e8c03e"  # default (round 8, same as dark)
    primary-disabled: "#e0dbd0" # default (round 8)
    quiet-hover: "#e9e5dc"    # default (round 8)
    quiet-pressed: "#e0dbd0"  # default (round 8)
typography:
  display:
    fontFamily: Instrument Serif   # owner (new-chat question only)
    fontSize: 44px
    fontWeight: 400
    lineHeight: 52px
  title:
    fontFamily: DM Sans            # default
    fontSize: 15px
    fontWeight: 600
    lineHeight: 20px
  body-lg:
    fontFamily: DM Sans            # default (composer text, question text at 600)
    fontSize: 14px
    fontWeight: 400
    lineHeight: 22px
  body:
    fontFamily: DM Sans            # default
    fontSize: 13px
    fontWeight: 400
    lineHeight: 20px
  small:
    fontFamily: DM Sans            # default (400 to 500; group labels at 500)
    fontSize: 12px
    fontWeight: 400
    lineHeight: 16px
  badge:
    fontFamily: DM Sans            # default
    fontSize: 11px
    fontWeight: 600
    lineHeight: 14px
  code:
    fontFamily: JetBrains Mono     # default (code, commands, cut line, timers)
    fontSize: 12px
    fontWeight: 400
    lineHeight: 18px
  branch:
    fontFamily: JetBrains Mono     # default (branch, diff stat)
    fontSize: 11px
    fontWeight: 400
    lineHeight: 16px
  chip:
    fontFamily: JetBrains Mono     # default (PR chip)
    fontSize: 11px
    fontWeight: 600
    lineHeight: 16px
spacing:
  base: 4px                 # default (the app step today)
  mark-box: 16px            # default
  sidebar-width: 264px      # default
  sidebar-rail: 52px        # owner (it starts under a window width of 1024 px, owner)
  tab-strip: 40px           # default
  tab-height: 32px          # default
  branch-bar: 36px          # default
  transcript-width: 760px   # default
  new-chat-width: 680px     # default
  status-line: 28px         # default
  row-padding-y: 6px        # default
  row-padding-x: 8px        # default
rounded:
  sm: 4px                   # default (PR chip, key box)
  md: 6px                   # default (sidebar row, Stop, branch chip)
  lg: 8px                   # default (buttons, tabs, tool cards, text fields)
  xl: 10px                  # owner for menus (menus, popovers, question card, toasts)
  2xl: 12px                 # default (composer, user message, dialog)
  full: 999px               # default (badges)
shadow:
  dark:
    shadow-1: "0 2px 8px rgba(0,0,0,0.40)"       # default (round 8: tooltip, toast)
    shadow-2: "0 8px 24px rgba(0,0,0,0.50)"      # default (round 8: menu, popover)
  light:
    shadow-1: "0 2px 8px rgba(29,27,24,0.10)"    # default (round 8: tooltip, toast)
    shadow-2: "0 8px 24px rgba(29,27,24,0.14)"   # default (round 8: menu, popover)
motion:
  fade-quick:
    duration: 50ms          # default (Material 3 short 1)
    delay: 0ms
    timingFunction: cubic-bezier(0.2, 0, 0, 1)
  state-change:
    duration: 100ms         # default (Material 3 short 2)
    delay: 0ms
    timingFunction: cubic-bezier(0.2, 0, 0, 1)
  surface-exit:
    duration: 150ms         # default (Material 3 short 3)
    delay: 0ms
    timingFunction: cubic-bezier(0.2, 0, 0, 1)
  surface-enter:
    duration: 200ms         # default (Material 3 short 4)
    delay: 0ms
    timingFunction: cubic-bezier(0.2, 0, 0, 1)
  # owner: the glyph loop is the wave pattern, 1.8 s per loop
components:
  button-primary:
    backgroundColor: "{colors.primary}"
    textColor: "{colors.on-primary}"
    typography: "{typography.body}"
    rounded: "{rounded.lg}"
    height: 32px
  button-primary-hover:
    backgroundColor: "{colors.dark.primary-hover}"
  button-primary-pressed:
    backgroundColor: "{colors.dark.primary-pressed}"
  button-primary-disabled:
    backgroundColor: "{colors.dark.primary-disabled}"
    textColor: "{colors.dark.disabled-text}"
  button-send:
    backgroundColor: "{colors.primary}"
    textColor: "{colors.on-primary}"
    rounded: "{rounded.lg}"
    width: 64px
    height: 28px
  button-quiet:
    backgroundColor: transparent
    textColor: "{colors.dark.on-surface}"
    rounded: "{rounded.lg}"
    height: 32px
    padding: 14px
  button-quiet-hover:
    backgroundColor: "{colors.dark.quiet-hover}"
  button-quiet-pressed:
    backgroundColor: "{colors.dark.quiet-pressed}"
  sidebar-row:
    rounded: "{rounded.md}"
    typography: "{typography.body}"
    padding: 6px
  sidebar-row-selected:
    backgroundColor: "{colors.dark.selected}"
  needs-count-badge:
    backgroundColor: "{colors.dark.selected}"
    textColor: "{colors.dark.muted}"
    typography: "{typography.badge}"
    rounded: "{rounded.full}"
    height: 16px
  child-badge:
    textColor: "{colors.dark.muted}"
    typography: "{typography.badge}"
    rounded: "{rounded.full}"
    height: 16px
  unread-dot:
    backgroundColor: "{colors.dark.on-surface}"
    size: 6px
  pr-chip:
    textColor: "{colors.dark.muted}"
    typography: "{typography.chip}"
    rounded: "{rounded.sm}"
    height: 18px
    padding: 5px
  pr-chip-draft:
    textColor: "{colors.dark.pr-quiet-text}"
  pr-chip-merged:
    backgroundColor: "{colors.dark.selected}"
    textColor: "{colors.dark.pr-quiet-text}"
  pr-chip-hover:
    textColor: "{colors.dark.on-surface}"
  glyph:
    size: 16px
  glyph-background-line:
    size: 12px
  tab:
    textColor: "{colors.dark.muted}"
    typography: "{typography.body}"
    rounded: "{rounded.lg}"
    height: 32px
  tab-selected:
    backgroundColor: "{colors.dark.surface}"
    textColor: "{colors.dark.on-surface}"
    width: 224px
  composer:
    backgroundColor: "{colors.dark.panel}"
    typography: "{typography.body-lg}"
    rounded: "{rounded.2xl}"
    padding: 12px
    width: 760px
  cut-line:
    textColor: "{colors.dark.muted}"
    typography: "{typography.code}"
  question-card:
    backgroundColor: "{colors.dark.panel}"
    rounded: "{rounded.xl}"
    padding: 16px
  question-choice:
    backgroundColor: "{colors.dark.surface}"
    rounded: "{rounded.lg}"
    height: 34px
  menu:
    backgroundColor: "{colors.dark.menu}"
    rounded: "{rounded.xl}"
    padding: 4px
  tooltip:
    backgroundColor: "{colors.dark.tooltip}"
    typography: "{typography.small}"
    rounded: "{rounded.md}"
---

# Swarm design, direction B

A mark follows each rule. **owner** is the owner's word, **default** is shown on a board but not confirmed, and **open** is not decided. Components use the dark tokens, and the light theme uses the light token of the same name.

## Overview

Swarm is a work surface for many coding agents. The direction is "B, quiet surface, one signature" (owner). The surface stays quiet, because the owner reads it for hours. The seven-dot glyph is Swarm's own, because a quiet look alone is common to agent apps. Both themes use warm greys (default), because today only dark has an identity.

## Sections

- [Foundations](docs/design/foundations.md): colours, the role to CSS variable table, type, layout, depth, shapes and motion.
- [Components](docs/design/components.md): the rule for each part of the app.
- [Voice, rules and open points](docs/design/voice-and-rules.md): content rules, do's and don'ts, and what is not decided.
