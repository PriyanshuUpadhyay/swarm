---
status: accepted
date: 2026-10-05
deciders: [user]
related: ["0021", "0029", "0034", "0036"]
informed-by:
  - "Council 2026-10-05 (GPT, Gemini, Claude seats), unanimous at round 2: NO-GO on rulesync, swarm owns the hook runner"
  - "User answers on 2026-10-05: a guard that cannot run blocks the call, a reminder lets it through; more CLIs are likely; swarm may own hook translation; hooks stay personal, with no hook files in project repos"
  - "rulesync src/features/hooks/antigravity-hooks.ts:167-168 replaces the whole global AGY hooks.json, so it would remove swarm's own group"
  - "codex app-server 0.159.0 hooks/list on 2026-10-05: a config.toml hooks table next to hooks.json loads both but warns 'prefer a single representation for this layer'"
  - "Hand proof on 2026-10-05 (Claude Code 2.1.289, codex-cli 0.159.0, AGY 1.2.17), receipts in the owner's ~/.claude/receipts/2026-10-05-swarm-guard/"
---

# 0040. `swarm guard` runs one rule list for every CLI

In the context of tool-call guards that must work the same way on Claude Code, Codex CLI, and AGY,
facing three hook payload shapes, three reply formats, a Codex trust hash for every hook, and
guards that let a call through when they crash, we chose a compiled runner, `swarm guard <provider>
PreToolUse`, that reads one rule list (`~/.swarm/guards.json`), gives every rule one plain JSON
call, and answers in each CLI's own format, with `swarm hooks setup` writing one fixed
registration per CLI. We neglected rulesync as the registration writer, ai-hook as the runtime, and
moving the checks out of hooks, to keep one owner for the registration, the payload, and the
trust. We accept that a bad swarm release can block every tool call on every CLI.

## Context and Problem Statement

The owner's guards were two scripts: one for Claude, and one for Codex and AGY with its own copy
of the policy. Each knew each CLI's payload and reply. Both let the call through when they
crashed. swarm already wrote AGY's hook group and the Codex trust hashes (ADR 0029, 0034).

## Considered Options

- swarm runs the rules and writes the registrations.
- rulesync writes the registrations, and a runtime such as ai-hook runs the rules.
- rulesync only for project hook files.
- No hooks: a PATH shim, an OS sandbox, or an MCP server.

## Decision Outcome

Chosen: swarm runs the rules and writes the registrations.

- Rule list: `{"rules": [{name, event, tools, command, timeout}]}`. Every rule is a guard.
  `tools` lists tool names, matched without case; absent means every tool.
  `command` is an argv, with `~/` as HOME and no shell.
- The list is `~/.swarm/guards.json` for every build and every SWARM_HOME, because the hooks
  that read it are global to each CLI; `SWARM_GUARDS` names another file for tests.
- A rule reads `{provider, event, tool, input, cwd, session_id}` on stdin and runs in the session's
  folder. Exit 0 allows. Exit 2 denies, with stderr (at most 64 KiB) as the reason. Any other
  exit, a timeout, or a program that does not start is a failure, and the call is blocked.
  Reminder hooks stay in each CLI's own files, where a failure already lets the call through; a
  reminder kind comes back only when a reminder moves into the runner. A missing or unparsable
  list blocks, and so does a rule whose event is not a guard event
  or whose `tools` list is empty, because it could never match. A panic in the runner exits 2.
- The runner has one 8 s deadline under the 10 s registration timeout, because a CLI that times a
  hook out lets the call through.
- Replies: Claude gets `permissionDecision: deny` JSON, Codex gets exit 2 and stderr, AGY gets
  `{"decision":"allow"}` or `{"decision":"deny"}`, never `{}`.
- Registration runs `swarm guard <provider> PreToolUse` from PATH, not a session's link (ADR
  0034), because a chair that the owner started by hand has no swarm session. Setup writes it only
  when the list exists: a group in each Claude `settings.json`, a group in each Codex home's
  `hooks.json` with its trust in `config.toml`, and an AGY group named `swarm-guard`. It keeps every
  other group (ADR 0036). A guard handler of the owner's with no timeout, or one under 10 s, is a
  conflict with its fix, because the CLI would time it out before the runner answers.
- A `.swarm` folder that holds only `guards.json` counts as empty for the home claim, because the
  owner's list can arrive before swarm first runs.

### Consequences

- Good: one rule file is the policy for three CLIs, and a rule edit changes no registration, so
  the Codex trust stays valid.
- Good: a crashed, slow, or missing rule blocks the call and names the rule and the list.
- Bad: one bad swarm release can block every tool call on every CLI; only the pinned reply tests
  guard against that.
- Bad: if `swarm` is missing from PATH, Claude and Codex let the call through and AGY refuses
  every tool.
- Bad: removing the list while the registrations stay blocks every call. `hooks status` then
  reports `guard: false`, and the owner removes the registrations by hand.
- Bad: only PreToolUse is a guard event. Hooks on events AGY lacks (UserPromptSubmit,
  SessionStart) stay in each CLI's own files.
- Bad: a Python rule adds a process start (about 40 ms) to each matched tool call.

## Pros and Cons of the Options

### rulesync and a runtime
- Good, because rulesync knows 25 or more CLIs.
- Bad, because its global AGY writer replaces the whole file, and it writes no Codex trust.

### Project hook files only
- Bad, because chair sessions start by hand in any folder, and the owner keeps hooks personal.

### No hooks
- Good, because a sandbox limits what a command can reach.
- Bad, because a PATH shim misses non-shell tools, and a sandbox cannot judge what a command means.
