---
name: prove-it-works
description: Owner rule for what counts as proof that a change works. Apply after a task, a behavior, or a delegated result and before the word done. Check the real artifact, run the feature, read the actual value, not a proxy, a self-report, or "it compiles".
disable-model-invocation: true
---

# Prove it works

Verify against the real thing. A build, a green lint, a file timestamp, a cached screenshot, or a
worker's summary is a proxy, and a proxy has unknown correctness.

Choose expected results from the requested behavior, repository contracts, and applicable
standards before using the implementation as evidence. Include relevant failure and boundary
cases. Control clocks, randomness, and external responses where they would make a test unstable;
exercise real local boundaries when mocks would hide the behavior being claimed.

Run the smallest relevant check after each behavior change, while its cause is still clear.
The agent runs these checks and fixes failures within the authorized scope; it does not assign
the user a manual trial as a condition for completing the work.

Choose the check by the layer that the change touches.

| Layer | Check |
|---|---|
| Logic or a bug | A red/green test, which fails before the change and passes after it |
| UI | A screenshot loop plus runnable checks, such as an overflow test, a contrast check, or a lag limit |
| Unclear idea | A throwaway prototype |

For UI, the runnable checks decide done. Report taste as awaiting the user's review, and never as passed.

When changing model routes, rule loading, or standards selection, run existing loader checks and
retain a case from the failure being fixed. Selection tests prove selection only. A claim about
model compliance also needs an agent run against the expected behavior, under the active host
contract. Record the model, rule version, result, and limits in the existing task record. When an
edit changes how a skill, rule, or model route behaves, run the retained case on the old text and
on the new text, with the same model and host, and record both results.

Rank a claim of safety or success on this ladder, and say where it stopped:

1. You said so. Worthless on its own.
2. You pointed at the line, a real `file:line` or the library's own source.
3. You showed the bad case cannot happen, step by step.
4. You ran it. A script or test that calls the real code and fails loud if you are wrong.
5. You exercised it on the real surface, the running app, CLI, or pane.

Anything below step 4 is unproven. Say so, and do not write it up as settled.

For a delegated result, inspect the diff, the file, or the runtime behavior, never the delegate's
summary. When a check fails, suspect the observation first, then the system.

Example. A parser change claims to reject an empty phone number. Step 4 is `pytest
tests/test_parse.py -q` with the new case in it, and the reply shows `1 passed`. "The code path
returns early on an empty string" is step 3 and stays marked unproven.

Prefer a check that reruns as a script over a one-time eyeball, and keep its output in the task
record as the evidence line.
