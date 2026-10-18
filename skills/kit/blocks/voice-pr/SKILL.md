---
name: voice-pr
description: "Draft a PR title and body from the repo's PR template and the author's voice file, for the owner to approve before anything is posted. Use when a task needs PR text."
license: MIT
metadata:
  kind: rule
  owns: "how PR titles and bodies read"
  serves: "close"
  needs: "the change, its tests, and their results"
  gives: "a PR title and body draft"
  conflicts: ""
  users: "planner"
  source: "docs/council 2026-10-13 block rules (V6: PR text is the author's text, apart from review comments); mined rows s4#14, s4#43"
---

## Rules
- Use the repo's PR template when one exists (`.github/pull_request_template.md`, or a file in `.github/PULL_REQUEST_TEMPLATE/`). Fill each of its sections, and drop none.
- Write in the author's voice. Apply `blocks/voice-review/SKILL.md` for the voice file path and its privacy rule. With no voice file, write short plain sentences.
- The title says the behavior that changed, not the files.
- The body says how the change was tested, with each command and its result, and names what is not verified.
- This is a draft. Opening or editing the PR is an outside write, so it waits for the owner's yes.
- Review comments belong to the `voice-review` block, not here.

## Example
Initial state: the repo has a template with Summary, Testing, and Risk, and the change adds dark mode.
Action: fill the three sections, with `npm test -> 42 passed` under Testing and "Safari not checked" under Risk.
Result: the title "settings page follows the system dark mode" and the body go to the owner as a draft.

## Check (script)
- none: the draft goes to chat, and no file in the commit range holds it.

## Check (judge)
- Does the body fill every section of the template, and give each test command with its result? Quote each section heading and one command with its result.
