---
status: accepted
date: 2026-09-25
deciders: [user]
related: ["0014", "0019"]
informed-by:
  - "User request and confirmation on 2026-09-25"
  - "https://code.visualstudio.com/docs/configure/custom-layout"
---

# 0020. Use one movable sidebar

## Context and Problem Statement

The current app has a workspace sidebar and a separate right inspector for changes, branch
comparison, PR details, and usage. The owner wants one place for these lists and controls,
with the view selection and left-or-right placement used by VS Code's primary sidebar.

## Considered Options

- Keep workspace navigation visible beside a separate right inspector.
- Use one sidebar with selectable views and allow it on either side of the main area.

## Decision Outcome

We chose one sidebar that can move left or right. It contains selectable views for Workspaces,
Files, Changes, and related details. Workspace grouping stays within the Workspaces view.
The separate right inspector will be removed.

For example, selecting Changes replaces the workspace list in the same sidebar. Moving the
sidebar to the right moves that same sidebar and its view controls.

This reverses the separate-inspector layout introduced on 2026-09-25. That layout has no earlier
decision record to supersede. This record accepts the direction; implementation is pending.

### Consequences

- The user has one place to find workspace lists, files, changes, and related details.
- The user can choose which side holds the sidebar.
- The user must switch views to see different lists; workspace navigation and changes are
  no longer visible as two separate sidebars at the same time.
