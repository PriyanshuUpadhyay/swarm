---
status: accepted
date: 2026-10-07
deciders: [user]
related: ["0046", "0049"]
informed-by:
  - "Owner verdict on 2026-10-07 in the redesign checklist (tmp/redesign/checklist.md): D-3 (c), S-15, S-16, S-17 later"
  - "StepRuns.swift:353: a step file names only the agent (`Status: active <agent>`), with no session id"
---

# 0051. A step run belongs to its folder, and its active agent links it to a chat

## Context and Problem Statement

ADR 0046 shows step runs in the Runs view of the selected workspace only. A run that waits on a
question in another workspace shows nowhere, and a run step that says `active coder` has no link
to the chat that runs coder.

## Considered Options

- The folder, as before.
- The session that started the run.
- The folder owns the run, and the agent name on the active step links it to a chat.

## Decision Outcome

Chosen: the folder owns the run. The app matches the agent name on each active step against the
agents of the chats in the same workspace. A match shows the run on that chat row, and each side
jumps to the other. The workspace row shows the most urgent run state, such as "1 run waits".
Rejected: the session, because a step file has no session id, one run spans many chats (a chair
after compaction, a review seat), and archiving a chat would hide a live run. An all-workspaces
Runs view comes later.

### Consequences

- Good: a waiting run shows on the sidebar from any workspace.
- Good: no change to the step-file format.
- Bad: two chats in one workspace with the same agent name make the link ambiguous; the app then
  links to the newest.
- Bad: the sidebar must scan runs while the Runs view is hidden, so the scan needs a slower timer
  than the view's 1 s poll.
