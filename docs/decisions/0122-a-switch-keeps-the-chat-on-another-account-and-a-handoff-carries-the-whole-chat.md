---
status: accepted
date: 2026-10-25
deciders: [user]
amends: ["0110", "0086"]
related: ["0042", "0081", "0036"]
informed-by:
  - 'owner, 2026-10-25: "There is no fucking reason of blocking the user right now"; "just importing that particular chat to a different account so that we can continue there"; "If there is no cache available … even if we summarize, we would get the same cost"'
  - 'owner, 2026-10-25: "go ahead" (copy the Codex session into the other account, record the write, run one live test)'
  - 'probe 2026-10-25: codex-cli 0.162.1 resumed a rollout copied from ~/.codex into a second account's Codex home and recalled the code word from its first turn'
  - 'live 2026-10-25: a failed usage read left the 100%-left account stale, and auto fell back to a fresh account with 0% left'
---

# 0122. A switch keeps the chat on another account, and a handoff carries the whole chat

In the context of a chat whose account is spent, where ADR 0110 sent a Codex account change to a
new chat and a handoff without a summary carried only the last 16 messages, we chose to keep the
chat in every same-CLI switch and to carry the whole chat in every handoff. A Codex resume that
names another account first copies the session's rollout to the same place in that account's
home, through the record of writes outside the swarm home (ADR 0042, writer `launch.codex_move`),
and the copy written last is the chat's session, so Auto stays where the chat last ran and a move
back replaces the older copy. A handoff whose old chair gives no summary sends the whole chat: in
the first message when it fits in 12,000 characters, else as `<swarm home>/handoffs/<session>.md`
that the new agent reads and summarizes. A pi chat on `default` records the model pi's settings
name, and a pi chat on `openai-codex/…` gets a Codex account through `PI_CODEX_HOME` and
`PI_CODEX_PROFILE`, which replaces ADR 0086's rule that pi has no accounts. Auto takes a stale
account with room left before a fresh account under the minimum. We neglected a separate summary
model, because the model the owner switches to reads the chat at about the same cost. We accept
that each move leaves a copy in the old home, that the pi Codex account works only with the
owner's `codex-profile.ts` pi extension, and that a stale reading can be out of date.
