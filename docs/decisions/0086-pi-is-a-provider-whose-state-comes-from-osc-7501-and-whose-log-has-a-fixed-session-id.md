---
status: accepted
date: 2026-10-14
deciders: [user]
related: ["0021", "0041", "0008", "0084"]
informed-by:
  - 'owner, 2026-10-14 (paraphrase): add pi as a provider; would it be easier to extend to future tools if we follow a standard, such as the OSC 7501 Program Status Protocol, rather than invent one?'
  - 'owner, 2026-10-14: yes go get the implmeentation done'
---

# 0086. pi is a provider whose state comes from OSC 7501 and whose log has a fixed session id

In the context of adding the pi coding agent (1.2.0) as a fourth provider, facing a CLI with no
hook config that swarm can pass at launch, and a state signal that pi already sends as the OSC
7501 Program Status Protocol (draft 0.4) when `PI_PROGRAM_STATUS=1` is set, we chose to read
that standard from the pane, where the tmux-swarm adapter pipes each pane's raw output to
`swarm status-feed`, which stores each root report as the agent's state the way `swarm hook`
does, and to give each pi agent the session id `<swarm session>-<agent>` with `--session-id`, so
the chair is named at launch and the server finds any pi log by that id, and to pass the host
contract with `--append-system-prompt`, and neglected a swarm pi extension that calls `swarm
hook`, the Agent Host Protocol, and a new store column for pi's id, to achieve live state, rows,
and a host contract for pi with no file in pi's config and a state path that any agent that
speaks OSC 7501 also gets, accepting that the feed runs only in tmux-swarm panes (Herdr reads pi
state through its own pi extension, and does not plan OSC 7501), that pi has no swarm guard,
accounts, or usage meters, and that the server scans pi's session folders on a read until a log
is stored.
