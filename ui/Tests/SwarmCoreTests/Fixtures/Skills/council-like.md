---
name: council
description: Orchestrate a visible multi-model debate between configured Gemini/AGY, GPT/Codex, and Claude voices to produce one advisory recommendation. Use for council, multi-model debate, cross-model second opinions, or high-stakes plan/design review.
---

# Council

Three independent models debate one pinned question and produce one advisory recommendation. The
top-level session is the chair: relay messages, correct grounding, judge convergence, and
synthesize. Do not vote as a fourth voice.

The council never makes the user's final design decision and never starts implementation. Seats
inspect evidence and write only council scratch artifacts. Workers follow the active runtime
adapter's `Worker contract` section. End with the options, evidence, disagreement, and the decision
that remains with the user.

## Start or continue

A council is a step run, so the kit's `references/step-run.md` owns the status line, pick-up,
close, and when a step waits for the user. The run folder is the decision-log folder below,
`<log>/<YYYY-MM-DD>-<slug>/`, next to the log file. "council continue <folder>" picks it up. Make the folder with
`python3 <kit>/references/step_run.py start <folder> <this SKILL.md>`, and close each step with `done`.

| File | Needs | Holds |
|---|---|---|
| `01-brief.md` | none | the pinned brief path, the seat names, and their routed models |
| `02-round1.md` | brief | the combined Round-0 and Round-1 ask, any questions and user answers, and each seat's Round-1 verdict and file |
| `03-cross.md` | round1 | Rounds 2-3 per seat, or `skipped converged at round 1` |
| `04-verdict.md` | cross | the Output block |
| `05-close.md` | verdict | the decision-log path, the seats closed and verified, and the scratch removed |

Round-0 questions that survive the filter, and a split after Round 3, set the step to `waiting`.

## Execution backend

The runtime adapter (`orchestrate-claude`) selects the host and its reference; the council names no host.
The kit's `references/fan-out.md` owns fan-out policy.

These rules hold on every backend, and the selected reference only says how to satisfy them:

- Every voice is a persistent, visible, interactive leaf seat rooted at the invoking surface. Never
  replace a seat with a headless or hidden process.
- Resolve `council.claude`, `council.gpt`, and `council.gemini` through the router before launch.
  Never hardcode a model, effort, sandbox, or approval value.
- Every seat brief is report-only: it forbids orchestration, spawning, repository edits, applied
  recommendations, commits, and user-facing notification, and permits only the assigned council
  scratch result.
- Reuse the same three seats for every round, and never start the next round while a seat is
  working.
- If a seat cannot launch or stay interactive, retry once, then replace it with another visible
  provider pane and tell the user the council is degraded. Never silently substitute a headless
  process.
- At completion, close only the identifiers this run captured, and verify their removal.

## Run state

Use a unique scratch directory:

```sh
COUNCIL_RUN_ID="${CODEX_THREAD_ID:-${CLAUDE_CODE_SESSION_ID:-council-$(date +%s)}}"
COUNCIL_DIR="/tmp/councils/${COUNCIL_RUN_ID}"
COUNCIL_WORKSPACE="$(pwd -P)"
mkdir -p "$COUNCIL_DIR"
~/.claude/scripts/ensure-council-access.py \
  --council-dir "$COUNCIL_DIR" --workspace "$COUNCIL_WORKSPACE"
```

Persist the selected backend, every captured pane/surface/thread identifier, voice outputs, and one
append-only `transcript.md`. Relay prompt-file paths and concise answers; do not copy raw tool
streams into the chair context.

## Pinned brief

Every voice receives the same:

1. Verbatim artifact/question.
2. Human-resolved constraints.
3. Exact file paths, URLs, commits, or other coordinates it may inspect.
4. A strict “do not wander outside these coordinates” guardrail.
5. Required output shape.

Voices are leaf reasoners. They may inspect the pinned material but may not spawn their own agents.

## Protocol

### Round 0 — clarify

Send Round 0 and Round 1 in one ask to all three voices. A voice with direction-changing
questions writes only its questions and stops. Each question says which answer would change
the analysis. A voice with none writes `NO QUESTIONS` and its Round-1 position in one artifact.
Filter out anything already answered, research-answerable from the pinned sources, cosmetic,
safely assumable, or based on invented scope. Put surviving questions to the user and record
the answers verbatim as resolved constraints. When a user answer changes a constraint, every
voice does Round 1 again. Otherwise, voices that stopped write their Round-1 positions.

### Round 1 — independent positions

Include the same pinned brief and the following output shape in the combined ask, before voices
see one another.

```text
VERDICT: GO | GO-WITH-CHANGES | NO-GO
TOP REASONS: at most 3, ranked
BIGGEST RISK
WHAT WOULD CHANGE YOUR MIND
```

Round 1 is one parallel round with artifact-authoritative completion: a seat is done when its
artifact is accepted, never when its screen looks idle. Do not wait for a slow seat past the round
deadline plus the backend's one bounded resume. When two artifacts are accepted and the third seat
is still degraded, continue with two positions and say so in the output; never re-prompt a seat
by hand.

**Convergence check.** When all three verdicts share a bucket, the recommended setups or options
agree, and no voice names an objection the others left unanswered, stop here and write the output
as unanimous with the round-1 reasons. A shared verdict label with different recommended setups
is not convergence; run cross-examination. Cross-examination is for a split, a setup difference,
or an open objection, not a ritual.

### Rounds 2–3 — cross-examination

Run only when the convergence check fails. Round 2 and 3 asks send only the brief path, the other
seats' latest position files, and the list of points still split. The path lets a resumed seat
re-read its coordinates. Do not resend the brief text. After each round, the
chair writes settled points as `Resolved:` lines in `03-cross.md`. Each line cites the position
file of each seat that agrees, or the user's decision. A point that any seat still contests stays
in the split list. Each next ask names the resolved points as
settled, not to be re-argued. Ask each voice to address the other positions by name, concede where
they are right, refute with evidence where wrong, and restate its current verdict plus strongest
reason. Every CONCEDED and REFUTED bullet must cite a file:line inside the pinned coordinates or a
URL; the chair rejects an artifact whose concession or refutation has no citation and re-prompts
the seat once for the citation. Preserve each provider's own thread/conversation continuity.

Stop as soon as all three share a verdict bucket and no unresolved objection remains. After round 3,
present the split and ask the user to break the tie. Send no fourth round. Do not manufacture consensus.

## Output

```text
## Council verdict: GO | GO-WITH-CHANGES | NO-GO (unanimous | 2–1 | human-decided)

Consensus reasoning: 2–4 sentences.
Required changes: all accepted changes, when applicable.
Dissent / residual risk: anything still contested.
Per-model trail: one line each for GEMINI, GPT, and CLAUDE.
```

The reply opens with the first screen in `~/.claude/references/plan-layout.md`. The verdict heading
is its verdict line. Its table holds the required changes and the open dissent, grouped, and Read
first names the top ones. The consensus reasoning, the full required changes and dissent, and
the per-model trail go below `Reference below`.

## Decision log

Before Round 0, search `~/dotfiles/docs/council/` for a prior
log for the topic and reuse still-valid resolved constraints.
The caller may pin one exact decision-log path outside the inspected product repository. Use that
path when it is present, include it in every seat's pinned brief, and do not also create a repository
log. An internally triggered planning Council must receive such an external path. Otherwise, write
runs invoked directly by the user to `~/dotfiles/docs/council/<YYYY-MM-DD>-<slug>.md`, from any repo
or none. Never write the log into the inspected repository, because a public repository would
publish its private paths. Include the artifact pointer,
Round-0 Q&A, models actually used, final verdict, required changes, dissent, per-model trail, the round
at which the council converged, and each round's wall time.

Run the selected backend's close-out, remove the scratch directory only after the durable log
exists, and surface every provider failure or fallback.
