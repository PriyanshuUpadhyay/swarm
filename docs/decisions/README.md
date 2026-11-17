# Decision records

ADR format. Read this index before proposing changes. Record bodies are immutable —
reversals supersede, never edit. Add one row, newest first, with each new record (the `decisions` skill is gone, ADR 0132).

| #    | Title                                                                | Tier | Status     | Date       | Refs                    |
|------|----------------------------------------------------------------------|------|------------|------------|-------------------------|
| 0146 | Each new chat gets its own worktree, named from its first prompt | 2 | accepted | 2026-11-17 | ↑0135 ↑0142 ↑0143 ↑0145 →0019 →0097 →0144 |
| 0145 | A task lands on origin and local main together, or as a PR | 2 | accepted | 2026-11-12 | ↑0135 →0142 →0144 |
| 0144 | Swarm task sweep removes merged worktrees and keeps branches | 2 | accepted | 2026-11-11 | ↑0055 ↑0135 →0134 →0142 |
| 0143 | The main wall's shell part warns and never blocks | 2 | accepted | 2026-11-11 | ↑0135 →0040 |
| 0142 | A taken branch opens or reopens, and the sidebar lists the worktrees in use | 2 | accepted | 2026-11-11 | ↑0135 ↑0055 →0049 |
| 0141 | Reports live in one shared folder and pages have random IDs | 2 | accepted | 2026-11-11 | ↑0101 |
| 0140 | A subagent call is a row, and its work opens as a read-only chat | 2 | accepted | 2026-11-11 | ↑0099 →0104 →0120 |
| 0139 | The chat counts records no parser maps | 2 | accepted | 2026-11-11 | ↑0099 →0120 |
| 0138 | Swarm notifies when the app chair ends a turn | 2 | accepted | 2026-11-06 | ↑0044 →0077 |
| 0137 | A verb that becomes a skill leaves presets in the same change | 2 | accepted | 2026-11-05 | →0132 →0136 |
| 0136 | The plugin ships chosen standalone skills, each tested by skill-eval | 2 | accepted | 2026-11-05 | ↑0132 →0133 |
| 0135 | Agents edit only in task worktrees, and main is read-only for them | 3 | accepted | 2026-11-03 | ↑0019 →0040 →0042 →0046 →0055 |
| 0134 | Tests cover the main path and real bugs, not every edge case | 2 | accepted | 2026-11-03 | →0133 |
| 0133 | A small change takes a short route with one review seat | 2 | accepted | 2026-11-02 | →0132 |
| 0132 | The swarm skill is the one workflow skill, and the verbs are its presets | 2 | accepted | 2026-10-31 | →0080 →0111 →0113 |
| 0131 | Each chat runs on a PTY host, and the chat streams the current turn | 3 | accepted | 2026-10-29 | ↑0099 ↑0120 ↑0123 ↑0126 →0084 →0104 →0118 |
| 0130 | The chat shows a sent message only as the CLI shows it | 2 | accepted | 2026-10-28 | ↑0038 →0021 →0012 |
| 0129 | A building Codex seat reaches the package hosts, and only those | 2 | accepted | 2026-10-27 | →0128 |
| 0128 | Coding goes to a junior seat, with the lead as the senior who answers and takes over | 2 | accepted | 2026-10-27 | →0124 |
| 0127 | The HUD restart reloads the login item | 2 | accepted | 2026-10-26 | →0125 →0087 |
| 0126 | The chair's swarm pty hosts its children on PTYs it owns | 2 | accepted | 2026-10-26 | →0123 →0120 →0084 →0125 |
| 0125 | The HUD restart sends SIGTERM and lets launchd start the new app | 2 | accepted | 2026-10-26 | →0118 →0087 |
| 0124 | A project board in the binary: typed posts, fenced claims, and walls that swarm enforces | 3 | accepted | 2026-10-24 | →0011 →0040 →0041 →0044 |
| 0123 | A PTY shim answers the OSC 7501 probe inside each tmux-swarm pane | 2 | accepted | 2026-10-25 | →0086 →0120 →0070 →0012 |
| 0122 | A switch keeps the chat on another account, and a handoff carries the whole chat | 1 | accepted | 2026-10-25 | ↑0110 ↑0086 →0042 →0081 →0036 |
| 0121 | Claude permission answers use a short-lived hook handoff | 2 | accepted | 2026-10-24 | →0029 →0040 →0120 |
| 0120 | Swarm drives agent CLIs through one ACP-shaped contract over a raw terminal | 2 | accepted | 2026-10-24 | →0042 →0117 |
| 0118 | The page offers a newer swarm, and upgrades and restarts it in two asks | 2 | accepted | 2026-10-23 | →0079 →0087 →0095 |
| 0117 | Swarm prunes only its recorded trust for folders that are gone | 2 | accepted | 2026-10-23 | →0042 →0043 →0088 |
| 0116 | A plain folder project shows one row with its chats | 1 | accepted | 2026-10-23 | →0115 →0097 |
| 0115 | The sidebar lists every chat, and a tab only shows what is open | 2 | accepted | 2026-10-23 | ↑0093 ↑0096 →0085 →0098 →0054 |
| 0114 | The usage HUD shows one ledger table and the running models | 1 | accepted | 2026-10-22 | →0089 →0087 |
| 0113 | A preset may offer blocks, and the Skills page shows the plan front door and duty blocks | 2 | accepted | 2026-10-22 | →0111 |
| 0112 | The run popover shows notes, limits, and cost, and blocks a switch while the agent works | 2 | accepted | 2026-10-22 | →0110 →0109 |
| 0111 | The web app shows the kit's skills from the kit's own script | 2 | accepted | 2026-10-22 | →0076 →0109 |
| 0110 | A model switch resumes the same CLI session, and a CLI change asks what to bring | 2 | accepted | 2026-10-21 | ↑0017 →0107 →0086 →0071 →0033 |
| 0109 | The sidebar drops its usage meters | 1 | accepted | 2026-10-21 | →0076 |
| 0108 | swarm close asks the child for a wrap-up, and drain is gone | 2 | accepted | 2026-10-21 | →0041 →0007 |
| 0107 | Resume starts a chat's own CLI session again, and the import rebuilds native sessions | 2 | accepted | 2026-10-21 | →0106 →0083 →0042 →0036 →0008 |
| 0106 | swarm import conductor copies Conductor's chats and tabs once | 2 | accepted | 2026-10-21 | →0084 →0085 →0043 →0054 →0083 |
| 0105 | The HUD cast drops the office workers | 1 | accepted | 2026-10-21 | →0094 |
| 0104 | A Cmd+J panel gives each agent a terminal and each background shell a read-only view | 2 | accepted | 2026-10-21 | →0099 →0029 →0083 →0023 →0003 |
| 0103 | ⇧⌘W archives the open chat | 1 | accepted | 2026-10-21 | →0060 →0054 |
| 0102 | A notice click opens the app chat, and the app guides notice permission | 2 | accepted | 2026-10-21 | →0069 →0077 →0084 →0088 |
| 0101 | Agent pages go to a loopback shelf served by swarm pages | 2 | accepted | 2026-10-21 | →0088 →0040 |
| 0100 | Swarm.app opens a normal window, not full screen | 1 | accepted | 2026-10-20 | →0095 |

Records 0001-0099 are in [README-0001-0099.md](README-0001-0099.md).
