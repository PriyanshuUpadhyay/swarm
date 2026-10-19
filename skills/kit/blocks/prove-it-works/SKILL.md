---
name: prove-it-works
description: "Prove a change works on the real thing: a check that fails before the change and passes after, with its command and output. Use when you test a change, judge a worker's result, or say done."
license: MIT
metadata:
  kind: rule
  owns: "what counts as proof that a change works"
  serves: "build, review, close"
  needs: "a behavior or a claim"
  gives: "a failing then passing check with its output, and each claim's ladder step"
  conflicts: ""
  users: "implement, review, checked-units"
  source: "blocks/prove-it-works/SKILL.md (rule moved from its former skill on 2026-10-13); blocks/red-first-check (merged here 2026-10-13)"
---

## Rules
- This block answers how a claim was proven. For source agreement, see `blocks/cite-sources/SKILL.md`; for the claim-to-proof map, see `blocks/done-when-evidence/SKILL.md`.
- Check the real thing. A build, a green lint, a file timestamp, a cached screenshot, or a worker's summary is a proxy, and a proxy does not prove the change.
- Choose the expected result from the requested behavior, the repository's contracts, and the standards that apply, before you use the code as evidence. Include failure and boundary cases.
- Control clocks, randomness, and outside responses where they would make a test unstable. Use the real local boundary when a mock would hide the behavior you claim.
- Run the smallest relevant check after each behavior change, while the cause is still clear. You run the checks and fix the failures inside the task's scope. Never make a manual trial by the user a condition of done.
- Choose the check by layer. Logic or a bug: a red/green test that fails before the change and passes after it. UI: a screenshot loop plus runnable checks, such as an overflow test, a contrast check, or a lag limit. Unclear idea: a throwaway prototype.
- For UI, the runnable checks decide done. Report taste as "awaiting the user's review", never as passed.
- A change to model routes, rule loading, or standards selection runs the existing loader checks and keeps a case from the failure it fixes. A selection test proves selection only. A claim about how a model behaves also needs an agent run against the expected behavior, under the active host contract. When an edit changes how a skill, rule, or model route behaves, run the kept case on the old text and on the new text, with the same model and host, and record the model, the rule version, both results, and the limits in the task record.
- Rank each claim on the ladder, and say where it stopped: 1 you said so, which is worthless on its own, 2 you pointed at the real `file:line` or the library's own source, 3 you showed step by step that the bad case cannot happen, 4 you ran a script or test that calls the real code and fails loud if you are wrong, 5 you used it on the real surface (the running app, CLI, or pane). Below step 4 is unproven; say so, and do not write it up as settled.
- For a worker's result, inspect the diff, the file, or the running behavior, never the worker's summary.
- When a check fails, suspect the observation first, then the system.
- Prefer a check that reruns as a script over a one-time look. Keep its output in the task record as the evidence line.

## Example
Initial state: a parser change must reject an empty phone number. Expected: `parse_phone("")` raises `EmptyPhone`.
Action: add the case to `tests/test_parse.py` and run `pytest tests/test_parse.py -q`: it fails, `1 failed`. Make the change. Run it again: `1 passed`.
Result: the evidence line reads `pytest tests/test_parse.py -q -> 1 failed` then `-> 1 passed`, ladder step 4. "The code returns early on an empty string" alone would be step 3 and stays marked unproven.

## Check (script)
- none: the FAIL and PASS lines live in the step file or task record, which holds this block's own Example lines; the commit range alone cannot show a check failed first.

## Check (judge)
- Does each behavior show a failing line from its check before the passing line of the same command? Quote both lines for each behavior.
- Does each UI behavior have a screenshot and at least one runnable check, such as an overflow, contrast, or timing check? Quote both.
- Does any claim cite only a build, a lint, or a self-report as proof? Quote it, or write "none".
- Does each claim name its ladder step, and is anything below step 4 marked unproven? Quote the claims.
- Do taste items read "awaiting the user's review"? Quote them.
