# red-first-check
Concept: Each behavior gets the check its layer needs, the check fails before the change, and the evidence is the command and its output.
Serves: build
Source: skills/prove-it-works/SKILL.md:9-10 (proxy), :12-15 (expected results first), :17-18 (smallest check per behavior), :21-29 (check by layer), :38-46 (ladder), :55-56 (rerunnable)

## Rules
- Choose the expected result from the requested behavior and the contracts before you look at the code. Include failure and boundary cases.
- Run the smallest relevant check after each behavior change, while the cause is still clear.
- Choose the check by layer. Logic or a bug: a red/green test that fails before the change and passes after it. UI: a screenshot loop plus runnable checks, such as an overflow test, a contrast check, or a lag limit. Unclear idea: a throwaway prototype.
- For UI, the runnable checks decide done. Report taste as "awaiting the user's review", never as passed.
- A build, a green lint, a file timestamp, a cached screenshot, or a worker's summary is a proxy. A proxy does not prove the change.
- Rank each claim on the ladder: 1 said so, 2 pointed at `file:line`, 3 showed the bad case cannot happen, 4 ran it, 5 exercised it on the real surface. Below step 4 is unproven; say so.
- When a check fails, suspect the observation first, then the system.
- Prefer a check that reruns as a script. Keep its output as the evidence line.

## Example
Initial state: the page has no install button yet. Expected: a click copies `brew install tracer`.
Action: write `checks/copy.mjs` that loads `index.html` in headless Chrome, clicks `#copy`, and reads the clipboard text. Run it: it fails, "no element #copy". Add the button. Run it again: it passes. Shoot the page at 375 px to `tmp/shots/home-375.png`: the button sits in the hero, and its label is not clipped.
Result: the evidence line reads `node checks/copy.mjs -> FAIL no #copy` then `node checks/copy.mjs -> PASS`, with the screenshot `tmp/shots/home-375.png`. The look reads "awaiting the user's review".

## Check (script)
- none: the FAIL and PASS lines live in the step file, which holds this block's own Example lines; the commit range alone cannot show a check failed first.

## Check (judge)
- Does each behavior show a FAIL line from its check before the PASS line of the same command? Quote both lines for each behavior.
- Does each UI behavior have a screenshot and at least one runnable check, such as an overflow, contrast, or timing check? Quote both.
- Does any claim cite only a build, a lint, or a self-report as proof? Quote it.
- Does each claim name its ladder step, and is anything below step 4 marked unproven? Quote the claims.
- Do taste items read "awaiting the user's review"? Quote them.
