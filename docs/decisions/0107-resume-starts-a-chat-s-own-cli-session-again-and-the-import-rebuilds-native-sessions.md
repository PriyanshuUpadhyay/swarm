---
status: accepted
date: 2026-10-21
deciders: [user]
related: ["0106", "0083", "0042", "0036", "0008"]
informed-by:
  - 'owner, 2026-10-21 (paraphrase): Continue chat loses the context; add entries and paths to the conversations so Conductor-only chats can be used directly'
  - 'owner, 2026-10-21: "yes build it" (approval of native session files plus a Resume action)'
  - 'probe 2026-10-21: a Claude transcript and a Codex rollout rebuilt from Conductor rows resumed with `claude --resume` and `codex exec resume`, and each CLI quoted the first request'
---

# 0107. Resume starts a chat's own CLI session again, and the import rebuilds native sessions

In the context of an ended chat, where Continue chat (ADR 0083) gives a new agent only a summary
or the last 16 messages, and of a Conductor chat whose CLI no longer has its session, so that ADR
0106 wrote a log that only swarm could read, we chose a Resume action and native rebuilt sessions.
`swarm launch orchestrator <role> --resume` starts the chat's own CLI again in the same chat, with
`claude --resume <id>` or `codex resume <id>` from the session's `chair_provider` and `chair_id`,
and Codex in the account whose home holds the rollout. The chair takes back its ended agent row,
the chat leaves Archived, and an imported chat gets the app's adapter. The app shows Resume beside
Continue chat for an ended Claude or Codex chair, through `POST /api/chat/resume`. The import now
writes a session that no CLI has where that CLI looks for it, a Claude transcript under
`projects/<folder>` or a Codex rollout under `sessions/<y>/<m>/<d>`, named by the swarm id, and
records each file as a managed edit of kind `file` by writer `import.conductor` (ADR 0042), so
`swarm managed revert` removes it only while its text is still what swarm wrote. We neglected
writing the files under swarm's home, because neither CLI resumes a session from another folder,
and reusing Conductor's provider ids, because Codex's own index may still map the old id to a
missing file. We accept that a resume runs the role's current model, so a chat larger than that
model's context is compacted by the CLI, that a rebuilt Codex rollout has no reasoning and a
rebuilt transcript no subagent steps, because Conductor kept neither, and that AGY and pi chats
have no Resume.
