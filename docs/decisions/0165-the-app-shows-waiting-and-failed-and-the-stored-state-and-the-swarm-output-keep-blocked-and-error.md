---
status: accepted
date: 2026-11-26
deciders: [user]
amends: ["0021"]
related: ["0054", "0164"]
informed-by:
  - 'owner, 2026-11-26: "2 3 4 5 6 recommended", the answer to the Align question 4 of the build run, which is about the names of the states. No owner quote names the words "waiting" and "failed" themselves.'
  - 'owner, 2026-11-24: "yep looks good" (the child badge "3 · 1 waiting" on the title line).'
---

# 0165. The app shows `waiting` and `failed`, and the stored state and the `swarm` output keep `blocked` and `error`

## Context and Problem Statement

ADR 0021 (its note of 2026-11-18) names the agent states after the Program Status Protocol 0.4
(OSC 7501): `working`, `blocked`, `done` and `error`. The web app shows these words today, for
example "Blocked" in `web/src/react/status.tsx`. ADR 0054 already writes the tab text as
"3 · 1 waiting". Design B says `waiting` and `failed`, because they say what the owner sees.

## Decision Outcome

The words the owner reads change. The stored words do not.

| Stored and wire value | Shown text, CSS class and new code |
|---|---|
| `blocked` | `waiting` |
| `error` | `failed` |
| `working`, `done`, `idle`, `ended` | same |

- The column `agent.state`, the OSC 7501 word, `swarm agents --json`, `swarm working --json`
  (`swarm.working.v2`) and the other `swarm` output keep `blocked` and `error`.
- One function in the web core maps the stored word to the shown word. Visible text, CSS classes
  (`.glyph.blocked` becomes `.glyph.waiting`) and new code use the shown word.
- No migration, no wire change and no Swift change.

## Consequences

- Good, because no stored data and no script that reads `swarm` output changes.
- Good, because the owner sees one word for one state in the tab, the row and the status line.
- Bad, because two words name one state. A builder must read the table above.
- Run 1 covers the state mark and its label. The web files that test `blocked` in logic keep the
  wire value. The sidebar text follows in run 3 under ADR 0166.
