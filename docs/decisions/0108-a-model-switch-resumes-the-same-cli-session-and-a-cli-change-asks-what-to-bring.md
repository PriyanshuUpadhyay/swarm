---
status: accepted
date: 2026-10-21
deciders: [user]
supersedes: ["0017"]
superseded-by:
related: ["0107", "0086", "0071", "0033"]
informed-by:
  - 'owner, 2026-10-21: "One run chip + one popover is sexy" (design A of the composer mock)'
  - 'owner, 2026-10-21: "ask everytime with a set default button and a setting entry in settings"; "pi as a provider is not there… need to add.. same with agy"'
  - 'probe 2026-10-21: Claude, Codex, pi, and AGY each resumed one session on a new model and recalled a word from the first turn; pi kept one session across claude-bridge and openai-codex models; no CLI config file changed'
---

# 0108. A model switch resumes the same CLI session, and a CLI change asks what to bring

In the context of the composer's model switch, where ADR 0017 handed every switch to a new chat
with a summary, so a Claude Opus to Sonnet switch lost the history that the CLI still had, we
chose to resume the chat's own CLI session with the new flags. A switch of model, thinking level,
or account inside the same CLI closes the chair pane and runs `swarm launch orchestrator chat
--resume` with `--model`, `--effort`, and `--account`: `claude --resume <id>`, `codex resume <id>`,
`pi --session-id <id>`, or `agy --conversation <id>`. Pi runs Claude and GPT models in one session,
so a pi switch across them is a resume too. A switch to another CLI cannot keep the session, so
the popover asks each time whether to bring a summary or start clean, with the owner's saved
default preselected, a "Make default" button, and the same entry in Settings. Pi and AGY join
Claude and Codex in the composer's one run chip and popover. We neglected a typed `/model` or
`/effort` in the pane, because a typed Claude level saves a default in the shared settings. We
accept that a Codex switch to another account stays a hand-off, because a Codex session lives in
one account's home, that the next reply re-reads the chat once, and that AGY refuses `--effort`
for some models, so the popover hides Thinking for them.
