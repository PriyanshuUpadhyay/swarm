---
status: accepted
date: 2026-09-17
deciders: [user]
supersedes: 0012
related: [0011]
informed-by: []
---

# 0013. Every chat is a swarm session

In the context of the app showing two kinds of chat, app chats and swarm sessions started in Herdr,
facing the owner's point that both are one orchestrator with children, we chose to start every new
app chat as a swarm session whose chair CLI runs in tmux as `orchestrator`, to record the chair's
provider and id in swarm, and to show every chat, wherever it started, as one row under the workspace
that holds its folder with one view (chat log, input into the chair pane, children, Resume), and
neglected keeping the built-in chat as a Settings choice as 0012 did, so the app has one chat type,
accepting that old built-in chats stay readable but cannot be continued as swarm sessions, that
worktrees outside `~/swarm/workspaces.noindex` are not supported, and that Herdr chairs have no
embedded terminal.
