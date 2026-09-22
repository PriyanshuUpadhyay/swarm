---
status: accepted
date: 2026-09-17
deciders: [user]
related: [0003, 0005, 0007]
informed-by: []
---

# 0011. List every swarm session under its project

In the context of agent sessions that a terminal chat starts outside the app, facing an app that
showed only the swarm session it made for its own workspace and a bus that did not know a session's
folder, we chose to have `swarm session new` record the folder, the start time and the chair's Claude
Code log, and to have the app list every session under the project that shares its git repository,
read only, with the chair's chat and each agent's asks and summaries, and neglected showing only bus
messages and linking sessions to projects by hand, so the owner can read any session in the app
whichever tool started it, accepting that sessions made before migration 0007 stay hidden unless
backfilled, that a long chat shows its last 256 KB, and that only a Claude Code chair has a log.
