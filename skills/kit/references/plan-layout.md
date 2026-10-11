---
description: Section order and layout rules for a substantial execution plan or a long reply that carries one, and the first screen of a long result. Read when AGENTS.md or a skill points here.
---

# Plan layout

One owner file for the shape of a substantial execution plan and of a long reply that carries one.
Short plans keep the `pair` skill's 3-7 behavior list. Research reports, reviews, and council
verdicts keep their own section order under the first screen below. Codex and AGY seats never load
this file, so a brief that needs it quotes it.

## Opening

The first one to three sentences state the outcome and the next action. Any decision that blocks
action appears here with its owner and the work it affects. An unanswered choice is never treated
as accepted. Nothing comes before the opening. Do not repeat it as a Goal section.

## Sections, in this order, H2 only

1. **Context** (optional). What changed from the prior revision, and a mental model in one to four
   sentences. No code.
2. **Decisions.** Numbered. Only the necessary ones, each with one line of why. A rejected option is
   named only when it was really considered.
3. **Overview** (optional). An action-only numbered list of the steps, one line each, used when the
   embedded code in Steps is long enough to hide the order.
4. **Steps.** One ordered, numbered list, grouped into H3 phases by real dependency. Each step starts
   with a verb and names its target `file:line` or command. Its code, SQL, or interface contract
   sits directly under it in a fenced block, never in a separate section. State the expected result
   in its own sentence. Name prerequisites or parallel groups explicitly, `[P]` for a step that
   can run alongside the previous one. About seven steps per phase is guidance, not a cap.
5. **Verification.** A table or a numbered checklist. Every requirement and every accepted
   constraint has a row with requirement, check, and evidence. Evidence reads "not run" until it
   is obtained. Separate a partial check from a completed proof.
6. **Assumptions and open questions.** Each line names who decides.
7. **Out of scope.** What the plan does not do.

## Layout rules

- One idea per bullet. Tables only for real mappings such as task, dependency, status, or
  requirement to evidence, never for prose.
- H2 for sections, H3 for phases, no deeper. No bold text used as a heading.
- Detail, rejected options, and file inventories come after the steps or are dropped. No design
  essay before the steps.
- A worker brief keeps its own owner (`orchestrate-claude`) and is not a plan.
- A diagram or a local HTML page follows `explain-formats.md` in this folder.

## First screen of a long result

A long review, council verdict, or report opens with a first screen of about 20 lines, before its
own sections. A plan keeps the opening above.

1. The verdict or result, with its counts.
2. **Read first**, at most five items, ranked by risk.
3. One table of at most seven rows, grouped.

Each status line starts with one symbol, then the key noun, and the path last. The symbols differ
in shape, so they read without color.

| Symbol | Meaning |
|---|---|
| ⛔ | blocker |
| ⚠️ | finding that does not block |
| ❓ | open question or decision |
| ✅ | done, agreed, or no finding found |
| ➖ | not checked or deferred |

Then the line `Reference below`, then the rest in the result's own section order.

Sources: Codex plan template, Spec Kit tasks, Google style guide procedures, GOV.UK structure
guidance, NN/g scanning research, SwarmForge role prompts; council log
`~/.claude/council-log/2026-09-11-plan-layout-frameworks.md`.
