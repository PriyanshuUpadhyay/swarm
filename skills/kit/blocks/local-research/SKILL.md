---
name: local-research
description: "Answer a question from local sources first, the repo, its docs, prior reports, and council logs, with paths, contrary evidence, and named gaps. Use when a question may already be answered on this machine."
license: MIT
metadata:
  kind: step
  owns: "what local sources say about a question"
  serves: "research, contracts"
  needs: "a question"
  gives: "findings with paths, contrary evidence, and gaps"
  conflicts: ""
  users: "research"
  source: "skills/research/SKILL.md (02-local, moved here 2026-10-13)"
---

## Rules
- Read the local sources as the kit's `references/fan-out.md` says. When they disagree, trust them in this order: the repository at hand, its docs, prior reports under `~/.claude/reports/`, then council logs under `~/dotfiles/docs/council/`.
- For each decision, track the open question, the primary evidence, the contrary evidence, and the gap that remains.
- Read the source's actual method or code before you accept its headline.
- Count sources as the kit block `blocks/cite-sources/SKILL.md` says. A page count or agents that agree do not make a claim correct.
- When the evidence is thin, name the gap; do not make the claim stronger. Do the deeper pass yourself; the user does not need to ask for it.
- Each finding cites its `path:line` or its report path.

## Example
Initial state: the question is "why does flow start stop in a repo with no profile?".
Action: read `flow_run.py` first and find the profile check at its `path:line`; then the flow skill's start rule; then a prior report that names the same stop.
Result: three findings, each with a path; the report's claim that "start makes the profile" is contrary evidence, because the code only prints a note; the gap is "no test covers the note".

## Check (script)
- none: the findings live in the step file or the report, outside the commit range.

## Check (judge)
- Does each finding cite a `path:line` or a report path? Quote two findings.
- Is contrary evidence recorded where a source disagrees, or written "none found"? Quote it.
- Is a claim that rests on thin evidence written as a gap, not as a finding? Quote the gap, or write "no gap".
