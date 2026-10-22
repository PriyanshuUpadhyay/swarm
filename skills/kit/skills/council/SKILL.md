---
name: council
description: Run a visible debate between Gemini/AGY, GPT/Codex, and Claude voices on one pinned question, and give one advisory recommendation. Use when the user asks for a council, a multi-model debate, a cross-model second opinion, or a high-stakes design review.
---

# Council

Three independent models debate one pinned question and produce one advisory recommendation. The
top-level session is the chair: relay messages, correct grounding, judge convergence, and
synthesize. Do not vote as a fourth voice.

The council never makes the user's final design decision and never starts implementation. Seats
inspect evidence and write only council scratch artifacts. Briefs and seats follow the kit block
`blocks/worker-brief/SKILL.md`, and the chair runs seats as the kit block `blocks/run-workers/SKILL.md`
says. The kit root, with its `blocks/` and `references/` folders, is two folders up from this skill's folder. End with the options, evidence, disagreement, and the decision
that remains with the user.

## Start or continue

A council is a step run, so the kit's `references/step-run.md` owns the status line, pick-up,
close, and when a step waits for the user. Choose the council run and log paths by
`references/run-folder.md`. "council continue <folder>" picks it up. Make the folder with the
front door in `skills/plan/SKILL.md` and the preset below, and close each step with `done`.

## Preset

Stop: chat
Offers: local-research, deep-search, primary-sources, cite-sources

| File | Needs | Holds | Block | Why |
|---|---|---|---|---|
| `01-brief.md` | none | the pinned brief path, the seat names, and their routed models | worker-brief | each seat judges one pinned brief |
| `02-round1.md` | brief | the combined Round-0 and Round-1 ask, any questions and user answers, and each seat's Round-1 verdict and file | run-workers | three visible seats answer the question |
| `03-cross.md` | round1 | Rounds 2-3 per seat, or `skipped converged at round 1` | run-workers | seats test each other's claims |
| `04-verdict.md` | cross | the Output block | decision-with-default | the options keep the decision with the user |
| `05-close.md` | verdict | the decision-log path, the seats closed and verified, and the scratch removed | voice-reply | the user reads the verdict and its log |

Round-0 questions that survive the filter, and a split after Round 3, set the step to `waiting`.

## Execution backend

The runtime adapter (`orchestrate-claude`) selects the host and its reference; the council names no host.
`blocks/run-workers/SKILL.md` owns launch, retry, degraded runs, and close. The council adds:

- Every voice is a persistent, visible, interactive pane rooted at the invoking surface. A native
  subagent does not replace a voice.
- The roles are `council.claude`, `council.gpt`, and `council.gemini`.
- Every seat is report-only, as `blocks/worker-brief/SKILL.md` says; its result file is its council
  scratch result.
- Reuse the same three seats for every round, and never start the next round while a seat is
  working.

## Run state

Use the temporary council-input path in `references/run-folder.md`:

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

Every voice receives the same brief, pinned for a judgment as `blocks/worker-brief/SKILL.md` says.

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

Round 1 is one parallel round. Apply `blocks/run-workers/SKILL.md` for result acceptance,
then check the returned artifact against this council's protocol. Do not wait for a slow seat past the round
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

The reply opens with the first screen in the kit's `references/plan-layout.md`. The verdict heading
is its verdict line. Its table holds the required changes and the open dissent, grouped, and Read
first names the top ones. The consensus reasoning, the full required changes and dissent, and
the per-model trail go below `Reference below`.

## Decision log

Before Round 0, search `~/dotfiles/docs/council/` for a prior
log for the topic and reuse still-valid resolved constraints.
The caller may pin one exact decision-log path outside the inspected product repository. Use that
path when it is present, include it in every seat's pinned brief, and do not also create a repository
log. An internally triggered planning Council must receive such an external path. Otherwise, use
the council log path in `references/run-folder.md`. Never write the log into the inspected repository, because a public repository would
publish its private paths. Include the artifact pointer,
Round-0 Q&A, models actually used, final verdict, required changes, dissent, per-model trail, the round
at which the council converged, and each round's wall time.

Close out the seats by `blocks/run-workers/SKILL.md`, remove the scratch directory only after
the durable log exists, and surface every provider failure or fallback.
