---
status: accepted
date: 2026-11-11
deciders: [user]
amends: ["0135"]
related: ["0040", "0135"]
informed-by:
  - 'owner, 2026-11-11: decision 2a, keep the edit-tool wall and the Codex OS profile; the shell part warns and never blocks'
  - 'audit 2026-11-11 (~/.claude/reports/2026-11-11-worktree-landing-practice.md)'
---

# 0143. The main wall's shell part warns and never blocks

## Context and Problem Statement

The audit found about 59 false blocks among 72 real denials since 3 November. All false blocks
came from reading shell text. These included `git -C $W …`, BSD `sed -i '' …`, Worktrunk help,
and owner-requested writes of ignored build output or `git pull` in main. The edit-tool wall had
no false hits and caught both real stray edits.

## Decision Outcome

- `main_wall` denies edit and write tools whose target is in main, with the same paths and
  exceptions as ADR 0135.
- A shell call that `shell_writes_main` flags passes this wall. Claude and Codex receive a short
  warning through `hookSpecificOutput.additionalContext` for `PreToolUse`. The warning says main
  is read-only for agents and points to `swarm task start <name>`. It sets no permission decision.
- AGY's guard reply has only an allow or deny decision. The pi extension discards output from a
  successful guard call. These providers pass the shell call silently.
- A warning still runs the worker wall and the owner's rules. Those guards can still deny a
  shell call for their own reasons.
- The Codex OS profile and the `reference-transaction` hook stay unchanged. Main remains
  read-only for agents; the shell warning does not give permission to write there.
- The shell parser stays best effort. Fixing its false matches is outside this change.

### Consequences

- Good: shell-text mistakes no longer stop a call at the main wall.
- Good: edit tools keep the wall that caught the real stray edits.
- Bad: Claude and AGY shell writes have less protection. The main wall no longer blocks them.
- Bad: a false shell match can still produce a warning on Claude and Codex.

This record amends ADR 0135's shell-denial rule. Its edit-tool wall, Codex OS profile, ref hook,
and worktree rules stay in effect. Run `swarm task start <name>` to edit in a task worktree.
