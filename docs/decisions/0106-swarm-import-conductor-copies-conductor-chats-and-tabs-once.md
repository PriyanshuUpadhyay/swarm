---
status: accepted
date: 2026-10-21
deciders: [user]
related: ["0084", "0085", "0043", "0054", "0083"]
informed-by:
  - 'owner, 2026-10-21 (paraphrase): import all the conversations and the open and closed tabs from Conductor into swarm, with deduplication, as a feature in Settings'
  - 'owner, 2026-10-21: "do it in a different worktree" (approval of the proposed design)'
  - 'probe 2026-10-21 of a real ~/Library/Application Support/com.conductor.app/conductor.db: 19 sessions, 6 with messages; 4 Codex rollouts still in ~/.codex/sessions; the Claude transcript and one Codex rollout exist only as rows in session_messages'
---

# 0106. swarm import conductor copies Conductor's chats and tabs once

In the context of an owner who moves from Conductor to swarm, facing chats that live in
Conductor's private SQLite file, where a tab is a `sessions` row (`is_hidden` 0 is open, 1 is
closed) and a transcript is either the provider's own log or only Conductor's `session_messages`
rows, we chose a `swarm import conductor` command with a plan and a digest, as `swarm setup` has
(ADR 0043), and an "Import from Conductor" section in Settings › Setup that shows the plan and
applies its digest. The import reads a copy of the database and changes nothing in Conductor. Each
Conductor session with messages becomes one ended swarm chat with no adapter, as it has no pane to list, with the
provider's own log when it is on disk, else a Claude-format log that swarm writes under
`imports/conductor/` from Conductor's rows. An open tab joins the owner's `imported` set and the
workspace's tab strip (ADR 0085, ADR 0054); a closed tab is archived, so it shows in Recently
closed. Deduplication skips a session with no messages, a session whose provider id or log a
swarm chat already has, and, of two Conductor sessions with one provider id, the one with fewer
messages. The swarm id is a UUID v7 at the Conductor creation time, so "Continue chat" can link a
newer session to it. We neglected a loose shell script, because the app owns `swarm.db` and
`choices.json` under its lock, and copying Conductor's ids, because a continuation needs an older
id. We accept that an imported chat is read-only until "Continue chat" hands it off, that a chat
with a converted log shows as a Claude chat even when Codex ran it, because the log format names
the provider, and that diff comments, scripts, env vars and terminal tabs are not imported.
