---
status: accepted
date: 2026-09-29
deciders: [user]
supersedes: "0003"
related: ["0021", "0022", "0023", "0026"]
informed-by:
  - "User on 2026-09-29, after the first app /council: the horizontal scroll feels jittery; the UI should not be a tmux pane but like the chat UI"
  - "User answer on 2026-09-29: chat only, but inputs and questions must be answerable from the UI"
  - "User answer on 2026-09-29: every kind of prompt (approval, hook trust, any question) is answered by the owner; no timeout picks a default"
  - "User answer on 2026-09-29: take the hook permission at onboarding; build for any Mac, not only this one"
  - "Hook payload research on 2026-09-29: Claude, Codex, and AGY send the transcript path to every hook; none sends the choice list the TUI shows (code.claude.com/docs/en/hooks, learn.chatgpt.com/docs/hooks, antigravity.google/docs/hooks)"
  - "Live captures on 2026-09-29 (Claude Code 2.1.284, codex-cli 0.159.0, AGY 1.2.13) in tests/fixtures/screens"
  - "codex app-server 0.159.0 hooks/list on an empty CODEX_HOME: the trust hash is the SHA-256 of the hook identity as sorted JSON"
---

# 0029. Show each child agent as a chat and answer its prompts from the app

## Context and Problem Statement

ADR 0003 gave each agent a chat and a SwiftTerm pane, and ADR 0022 put the panes in a horizontal
strip. In the first `/council` run from the app, the owner found the strip jittery and a terminal
the wrong view for a seat. The owner still has to answer a seat's questions: a tool approval, a
folder or hook trust screen, or a question with choices. Swarm had no chat log path for a child,
and no hook payload carries the choices a CLI shows.

## Decision Drivers

- The owner reads each seat as a chat, as the chair's chat reads.
- The owner answers every question; nothing answers for the owner, and no timeout picks a choice.
- One answer path for Claude, Codex, and AGY.
- A new Mac with no provider config works after one consent.

## Considered Options

- Chat plus a live terminal per agent (ADR 0003, today).
- Chat only; answers go through a blocking permission hook.
- Chat only; answers go as keys after the screen proves the question (screen keys).

## Decision Outcome

Chosen: chat only, with screen keys.

- `swarm hook` stores the payload's transcript path in `agent.log`, and each child column reads
  that log with the chair's transcript views.
- The screen check of ADR 0021 also reads the question and its numbered or cursor choices into
  `prompt` in `swarm agents --json`. A column and the chair page show it as a card with one button
  per choice, in the CLI's words.
- `swarm answer <agent> <prompt_id> <choice>` reads the screen again, refuses when the question
  changed, and sends the choice's key through the adapter's `key` verb. It refuses a caller inside
  an agent pane.
- `swarm hooks setup` runs on the owner's consent at onboarding. It writes swarm's own Codex hook
  trust entries (group 1 of the session flags, so another tool's group-0 entries stay) into every
  Codex home and swarm's AGY group into `hooks.json`. `swarm launch` no longer writes AGY's file.
- The strip layout (ADR 0022) and drag resize (ADR 0026) stay. ADR 0023's terminal key routing no
  longer applies to child columns; `swarm attach` still opens a child's pane by hand.

### Consequences

- Good: a seat reads as a chat, and five chat columns scroll with no terminal redraws.
- Good: approvals, trust screens, and questions are answered from the app for all three CLIs.
- Good: a new Mac works after `swarm hooks setup`; the owner's other hooks stay as they are.
- Bad: screen patterns and digit keys follow each CLI's TUI, so they need captured fixtures and
  upkeep. AGY's command approval stays synthesized: with its terminal sandbox on, it never asked.
- Bad: a question shape with no fixture shows as "waits on a question the app cannot read".
- Bad: the Codex trust hash follows Codex's hash rule and the swarm binary path; either change
  makes Codex ask again, and `swarm hooks setup` must run again.
- Deferred: how a user grants hook trust in the long run (per hook, Codex's bypass flag after a
  warning, or sandboxed hooks) needs its own decision.

## Pros and Cons of the Options

### Chat plus terminal (ADR 0003)
- Good, because every TUI detail is visible and typeable.
- Bad, because terminals are the jitter suspect and read as a terminal, not a chat.

### Blocking permission hook
- Good, because Claude and Codex give an exact allow or deny.
- Bad, because it misses trust screens, "always allow" choices, and all of AGY, and it breaks
  ADR 0021's rule that a hook never holds a turn.

### Screen keys
- Good, because one path covers every prompt the screen shows, in the CLI's own words.
- Bad, because it depends on screen patterns, so the answer re-reads the screen before each key.
