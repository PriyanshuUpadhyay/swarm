---
description: Section order and layout rules for a substantial execution plan or a reply that carries one. Read when preparing an execution plan.
---

# Plan layout

This file owns the shape of a substantial execution plan and of a reply that carries one.
Short plans keep the 3-7 behavior list of `implement` in `pair` mode. Reports, reviews, council
verdicts, and other results follow the loaded session reply rules and their task's required content.

The checked `plan.md` file made by `blocks/planner/SKILL.md` uses the section order that block owns,
and the owner reads it as the page that `blocks/plan-chart/SKILL.md` draws. This is the exception to
the opening and section order below. The reply that presents the plan still follows this layout.

## Opening

The first one to three sentences state the outcome and the next action. Any decision that blocks
action appears here with its owner and the work it affects. Apply
`blocks/decision-with-default/SKILL.md` from the kit root for open choices. Nothing comes before
the opening. Do not repeat it as a Goal section.

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
5. **Verification.** Put the evidence map here, by the kit block
   `blocks/done-when-evidence/SKILL.md` from the kit root.
6. **Assumptions and open questions.** Each line names who decides.
7. **Out of scope.** What the plan does not do.

## Layout rules

- One idea per bullet. Tables only for real mappings such as task, dependency, status, or
  requirement to evidence, never for prose.
- H2 for sections, H3 for phases, no deeper. No bold text used as a heading.
- Detail, rejected options, and file inventories come after the steps or are dropped. No design
  essay before the steps.
- A worker brief keeps its own owner (the kit block `blocks/worker-brief/SKILL.md`, from the kit root
  one folder up from this file) and is not a plan.
- A table, a diagram, or a local HTML page follows `explain-formats.md` in this folder.

Sources: Codex plan template, Spec Kit tasks, Google style guide procedures, GOV.UK structure
guidance, NN/g scanning research, SwarmForge role prompts; council log
`~/.claude/council-log/2026-09-11-plan-layout-frameworks.md`.
