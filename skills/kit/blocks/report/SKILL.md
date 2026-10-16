---
name: report
description: "Write a research answer as a short plain report: the answer first, then evidence with links, what is contested, off-path ideas, and dated sources. Use when a question ends in a written report."
license: MIT
metadata:
  kind: step
  owns: "the shape of a saved research report"
  serves: "report, close"
  needs: "findings with sources"
  gives: "a report file and a reply that holds it"
  conflicts: ""
  users: "research"
  source: "skills/research/SKILL.md (05-report and 06-close, moved here 2026-10-13); references/plan-layout.md (first screen)"
---

## Rules
- Write in plain words. Aim for about 800 words, and go longer when the evidence needs it. Never cut a claim, a caveat, or a source only to meet the number.
- Use this order:
  1. **Answer**: the answer first, in two or three sentences.
  2. **Evidence**: each claim with its link to the source that owns it.
  3. **What is contested**: where the sources disagree.
  4. **Off the usual path**: each off-path idea with its source, why it might work, and why it might fail. If none held up, name the searches you tried.
  5. **Sources**: newest first, with dates.
- Apply the session's agent instructions to the reply. Give the report's file path in one line.

## Example
Initial state: a question has five checked sources and one off-path idea.
Action: write the answer in three sentences, then five evidence lines with links, one contested point, the off-path idea with why it might work and fail, and the sources newest first.
Result: a 700-word report. The reply shows the whole report, then its path.

## Check (script)
- none: the report lives outside the commit range.

## Check (judge)
- Does the report open with the answer in two or three sentences, and keep the five sections in order? Quote the section headings.
- Does the reply follow the loaded agent instructions? Quote the applicable instruction and the reply lines that meet it.
