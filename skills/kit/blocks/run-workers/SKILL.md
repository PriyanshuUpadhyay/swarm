---
name: run-workers
description: "Start, brief, collect, and close worker seats on the host the session runs in, and stop when the host cannot do it safely. Use when a skill asks for seats, a council, a review, or a parallel search."
license: MIT
metadata:
  kind: rule
  owns: "how a root session runs worker seats on its host"
  serves: "every step that uses seats"
  needs: "a skill's seat list or orchestration.json, and an injected agent-host contract"
  gives: "visible seats with accepted results, closed at the end"
  conflicts: ""
  users: "orchestrate-claude, orchestrate-codex, orchestrate-agy, council, web-search, review-check"
  source: "skills/orchestrate-claude/SKILL.md, skills/orchestrate-codex/SKILL.md, skills/orchestrate-agy/SKILL.md (the shared text, moved here 2026-10-13); skills/council/SKILL.md (Execution backend)"
---

## Rules
- The skill owns its seats, prompts, rounds, result routing, and refusals. Read its `orchestration.json` as requirements, not as advice. This block runs those requirements and stops when the host cannot meet them. It does not change the workflow or copy its prompts.
- Before the first worker, require an injected `[agent-host: ...]` contract. With none, go on alone only when the skill allows it; else report the missing capability and stop.
- Load the host reference that the contract names. The host owns pane creation, sending, liveness, result collection, and close.
- Resolve every seat role with `swarm roles get <role>` before launch. Keep `runnerId`, `provider`, `mode`, `model`, `effort`, and `fallbackRunnerIds`. Use only the declared fallbacks. Refuse a role that does not resolve or that the host cannot run. Never hardcode a model, an effort, a sandbox, or an approval value.
- Give the host one entry per seat: a unique name, the routed runner, the working folder, the permission level, the input digest, the brief, the expected result file, and the deadline. The host picks the run folder under its message root, because every worker sandbox can write there. Never brief a worker to write into a home-state path.
- Never use a headless CLI or a detached process. When the skill or the host contract asks for visible seats, every seat is a visible, interactive pane owned by this run. Otherwise a native subagent of the runtime is allowed; the runtime adapter names which.
- If a visible seat cannot launch or stay interactive, retry it once. Then start a visible seat on the role's next declared fallback runner, and tell the user the run is degraded. With no fallback left, go on without that seat only when the skill allows it.
- Send the briefs of one stage under one deadline. Sending returns when the worker took the task, not when it finished; the worker's finish rings the chair. The kit's `references/fan-out.md` owns what goes out in parallel and the chair's work while seats run.
- Accept only a result that the host's durable channel confirms and that names its seat. Pass the skill's refusal or degraded state to the chair as the skill says.
- At the end, close only the seats this run started, and check that they are gone.

## Example
Initial state: a council needs three seats, `council.claude`, `council.gpt`, and `council.gemini`.
Action: run `swarm roles get` for each role, launch three visible panes with unique names, send one brief to each in the same turn, and end the turn. The GPT pane fails to launch twice, so a visible pane on the role's fallback runner replaces it, and the chair tells the user.
Result: three result files, each confirmed by an inbox message from its seat; at the end `swarm close` runs for the three names this run started, and `swarm agents` no longer lists them.

## Check (script)
- none: seats and their results live in the host's state and the chair's run folder, not in the commit range.

## Check (judge)
- Was each seat's role resolved with `swarm roles get` before launch? Quote the routed model of each seat.
- Was any seat headless or detached, or hidden when the skill asked for visible seats? Quote the launch line, or write "none".
- Were all seats this run started closed and checked gone at the end? Quote the close and the check.
