---
name: base-rules
description: "Keep the few rules every step keeps: facts only from the source of truth, real over drawn, no extra scope, evidence for done, example first. Use when any workflow step runs."
license: MIT
metadata:
  kind: rule
  owns: "the rules every step of every workflow keeps"
  serves: "every step"
  needs: "source_of_truth"
  gives: "steps that keep the base rules"
  conflicts: ""
  users: "flow"
  source: "blocks/prove-it-works/SKILL.md (rule 1, a proxy does not prove; last rule, a rerunnable check); html-plan pack: skills/html-plan/SKILL.md:82 (real over drawn); skills/review-check/references/lens.md:28 (no speculative generality)"
---

## Rules
- Facts come only from {{source_of_truth}}. If it does not say it, do not write it.
- Real over drawn. Cite the file and line for a fact or a code claim.
- Do the requested outcome at the intended scope. Add no speculative feature, layer, or option.
- Every claim of done has evidence: a command and its result, a screenshot path, or a file:line.
- Explain with one concrete example first, then the rule. Use plain words.
- If two rules disagree, say which two and which one you followed.

## Example
Initial state: a step wants to write "the agent retries three times". Action: find the line in {{source_of_truth}} that says so and cite it. Result: the claim carries its `path:line`, or it is cut.

## Check (script)
- none: each rule needs meaning, so its checks are judge checks.

## Check (judge)
- Does every fact in the result trace to a line in {{source_of_truth}}? Quote each doubtful fact and its source line, or write "no source line".
- Does each claim of done name its evidence? Quote the claim and its evidence.
