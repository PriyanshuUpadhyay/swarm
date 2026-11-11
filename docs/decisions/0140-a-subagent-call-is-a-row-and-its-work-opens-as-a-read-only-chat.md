---
status: accepted
date: 2026-11-11
deciders: [user]
amends: ["0099"]
related: ["0104", "0120"]
informed-by:
  - 'owner, 2026-11-11: "They are independent. Get them sorted out completely in parallel. We have to care about each and every kind of agent, be it claude pi AGY Codex."'
  - 'owner, 2026-11-11: approved plan tmp/flow/2026-11-11-ui-subagent-chat/plan.md on its Plannotator page with no notes, so each recommended default was accepted'
  - 'research 2026-11-11: T3 Code shows a subagent item and opens its child thread with a bar that says it runs on its own and has "Open parent" (ProviderSubagentBar.tsx)'
  - 'Claude Code writes each subagent log to <session dir>/subagents/agent-<id>.jsonl with agent-<id>.meta.json that holds toolUseId (checked by the chair on a local session)'
  - 'Codex docs and source: a child is its own rollout file whose session_meta has source.subagent.thread_spawn.parent_thread_id (hexdocs Codex.Protocol.SubAgentSource)'
  - 'AGY docs: the parent calls the invoke_subagent tool, and transcripts stay "readable in JSONL logs" with no path given'
  - 'pi docs: no built-in subagent; the official extension example runs a separate pi process'
---

# 0140. A subagent call is a row, and its work opens as a read-only chat

## Context and Problem Statement

A subagent call showed as a generic tool row, and the subagent's own work was not readable in the app.
ADR 0099 keeps the chat to what the CLI shows. Claude, Codex, and AGY each show a subagent view in their own CLI, so a subagent row and its work fit that rule. The log of the child is a file the chat did not read before.

## Considered Options

- A `subagent` row from the existing call and result events, and a child chat that reads the child's own log.
- A new `subagent` event in each parser.
- A terminal for each subagent.

## Decision Outcome

Chosen: the first option. The row is made in `src/view` from a call named `Agent` or `Task` (Claude), `spawn_agent` (Codex), or `invoke_subagent` (AGY), so no parser changes. The row shows the type, the task, the state, and the result.

A click opens the child chat. Its agent id is `<agent>~sub~<tool call id>`, so every route that takes an agent id (`/api/rows`, the stream, full tool output) reads the child's log. The server finds that log (`src/serve/subagent.rs`): for Claude by `toolUseId` in `agent-<id>.meta.json` and a check that the file stays in the session's `subagents` folder; for Codex by the id in the `spawn_agent` output and a check that the rollout names a `thread_spawn` parent. The child chat has no composer. Its bar says "Runs on its own" and has "Open parent".

AGY shows the row, but its click finds no log, because its docs give no path. pi gets no row and no child chat, because it has no subagent tool. Both are named gaps, not guesses.

### Consequences

- Good: no parser change, and one view serves the chair, a pane, and a child.
- Good: the child log path comes from the parent log and one folder check, never from the page.
- Bad: the Claude folder layout is not documented by Claude, so an update can move it; the row stays and its click shows "log is not available".
- Bad: the Codex `spawn_agent` argument names changed across versions, so the task text falls back through `description`, `message`, `prompt`, and the first `items` text, and the child id is the first UUID in the output.
- Bad: a child chat follows no stream; it reads its log again every 3 seconds while it is open.

## Links

- Amends: 0099
- Related: 0104, 0120
