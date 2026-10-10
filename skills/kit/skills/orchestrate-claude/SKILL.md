---
name: orchestrate-claude
description: Bind provider-neutral multi-agent workflow requirements to Claude's orchestration mechanics. Use when a Claude root session runs a portable skill that requests isolated workers, routed models, durable results, or parallel fan-out.
---

# Orchestrate from Claude

Enforce the workflow's behavior contract independently of the route's filesystem capability. A
report-only seat must not edit the repository, apply findings, or commit, even when its runner has a
writable sandbox. It may write only its attributed scratch result.

Translate a portable workflow's roles, seats, permissions, and completion
contract into the active execution environment. Do not reinterpret the
workflow or copy its prompts into this adapter.

## Contract operations

Provide every version-1 orchestration operation declared by a portable workflow:

- resolve semantic roles and exhaust only their declared fallbacks;
- validate visibility, isolation, permission, durable-result, and teardown capabilities;
- create uniquely named visible leaves owned by the current run;
- dispatch seat briefs under one stage deadline;
- accept only attributed durable results;
- relay the workflow's refusal or degradation disposition to the chair; and
- close only leaves owned by the current run.

Read the workflow's `orchestration.json` as requirements, not suggestions. The workflow owns
topology, prompts, rounds, result routing, and refusal semantics. This adapter executes those
requirements and fails closed when the active host cannot satisfy them.

## Worker contract

Every brief states what the seat may change, what it must verify and how, what it must leave to
another seat, where its result goes, and when it must stop. A brief missing one of these is not
dispatched. A non-Claude seat receives this text in the brief or as a digest-pinned copy in the run
dir. A report-only seat inspects and writes only its attributed scratch result; it does not edit a
repository, apply a finding or recommendation, commit, or dispatch a coding worker.

Include the applicable project-instruction and standards source paths, plus the expected behavior
and checks. The worker reads those sources before work; it must not assume it inherited the
chair's loaded rules. Keep rule text at its owner instead of copying it into every brief.

Every worker turn ends in one published result, complete, blocked, or failed. "No change,
because ..." is complete. Blocked means the brief is ambiguous, two instructions contradict, a test
and its specification disagree, or a named file or helper is missing. The worker publishes the
question and its evidence; it does not ask in the pane and does not guess.

At start, the worker confirms that the worktree, paths, and helpers the brief names exist, else it
publishes blocked. It works only in the tree and paths the brief names. When the sandbox denies a
tool cache outside the workspace, it points the cache inside the workspace and never requests a
wider sandbox.

## Bind the environment

1. Require an injected `[agent-host: ...]` contract before creating workers.
   If none exists, continue serially only when the workflow permits it;
   otherwise report the missing orchestration capability and stop.
2. Load the host reference the contract names (for Herdr,
   `references/host-swarm.md`). The host owns pane creation, transport,
   liveness, artifact collection, and teardown.
3. Resolve every requested workflow role with
   `swarm roles get <role>`. Preserve `runnerId`,
   `provider`, `mode`, `model`, `effort`, and `fallbackRunnerIds`. Reject an
   unresolved role or a route the active host cannot execute.
4. Give the host one leaf specification per seat: unique name, routed runner,
   cwd, permission level, input digest, prompt or brief, expected artifact,
   and deadline. The host selects the run dir under its bus root, because that
   path must be writable from every worker sandbox. Never brief a worker to
   write into a home-state path.
5. Accept only results validated by the host's durable completion channel. The
   expected artifact is the workflow's durable store path. Result acceptance is
   the swarm inbox message. Close only workers created by this run.
6. Dispatch returns after worker uptake, never on completion. The host owns
   uptake and the child's finish rings the chair pane.
   Follow `references/fan-out.md` for dispatch and chair work while seats run.

Native background subagents are allowed unless the host contract or the
workflow requires a visible pane. Never use a headless CLI. Every Workflow
`agent()` call pins `model: "sonnet"` or `model: "haiku"`, never Opus or Fable,
because a workflow fans out to many agents. Workers are leaves: they never spawn descendants or visible panes and never
notify the user. They report one consolidated result to the orchestrator.

## Task files

A skill that keeps a task file across `/clear` keeps it in `<repo-root>/tmp/<skill>/`, as
`~/.claude/references/run-folder.md` says. The reports folder is `~/.claude/reports/`; a step run
whose result is a report keeps its folder there.
`<kit>` is `~/.claude`, so the kit's references are in `~/.claude/references/`.

A Codex `search.web` seat for `web-search` gets
`-- -c features.network_proxy.allow_local_binding=true` at `swarm launch`, so its browser rows
reach the local `playwriter` relay. No other Codex seat gets this flag.
