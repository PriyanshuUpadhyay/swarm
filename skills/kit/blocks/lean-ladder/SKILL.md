---
name: lean-ladder
description: "Before you write code, climb the ladder: need it at all, already in the repo, standard library, platform feature, installed dependency, one line, then the least code. Use when you write or review new code."
license: MIT
metadata:
  kind: rule
  owns: "the order of choices before new code is written"
  serves: "build, review"
  needs: "a change that adds code"
  gives: "the rung that each new piece of code stopped at, and the marked shortcuts"
  conflicts: ""
  users: "implement, review, lean-review, planner"
  source: "ponytail pack: skills/ponytail/SKILL.md (the ladder, the ponytail: marker; MIT, Copyright (c) 2026 DietrichGebert); council 2026-10-30-skill-consolidation"
---

## Rules
- Read the task and the code that it touches first, and trace the real flow. The ladder makes the answer shorter, never the reading.
- Stop at the first rung that holds:
  1. Does it need to exist at all? A need that may come later is not a need now.
  2. Is it already in this repo? Reuse the helper, type, or pattern.
  3. Does the standard library do it?
  4. Does a platform feature do it, such as `<input type="date">`, CSS, or a database constraint?
  5. Does an installed dependency do it? Do not add a dependency for a few lines.
  6. Can it be one line?
  7. Only then, write the least code that works.
- A bug fix goes at the root cause. Search every caller of the function before you edit it, and fix it once where all callers pass.
- When two standard options are the same size, take the one that is correct on edge cases.
- Mark a shortcut that has a known limit with a comment `lean: <limit>, <when to upgrade>`, for example `# lean: one global lock, use per-account locks if throughput matters`. Older code may use `ponytail:` for the same marker.
- Never cut input checks at a trust boundary, error handling that prevents data loss, security, or accessibility basics. Keep one small runnable check for logic that branches or loops.
- Abstractions and extraction follow `blocks/when-to-extract/SKILL.md`.

## Example
Initial state: a task asks for a cache on an API fetch function in Python.
Action: rung 3 holds, because `functools.lru_cache` does it. Write `@lru_cache(maxsize=1000)` on the function.
Result: one line, no cache class. The step records "rung 3: lru_cache".

## Check (script)
- none: which rung holds is a question of meaning.

## Check (judge)
- For each new helper, class, or dependency, quote it and the rung where it stopped. Does a lower rung hold instead? Quote the function or feature, or write "none".
- Does each `lean:` marker name a limit and when to upgrade? Quote each one, or write "no marker".
