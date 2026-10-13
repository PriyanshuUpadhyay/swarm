---
name: scratch-home
description: "Run a trial, a setup test, or a test build in a temporary home and state folder, so the owner's live settings never change. Use when a check installs, configures, or writes tool state."
license: MIT
metadata:
  kind: rule
  owns: "where trial and test state lives, apart from the live home"
  serves: "build, review, research"
  needs: "the tool and the places it writes"
  gives: "a scratch root, and proof that no live file changed"
  conflicts: ""
  users: "tool-trial, prove-it-works, planner"
  source: "mined rows s2#4, s3#6, s3#7, s3#13, s4#42; blocks/tool-trial/SKILL.md (scratch place); docs/progress.md A4 (a scratch test of plugin installs)"
---

## Rules
- Make one root with `mktemp -d`, and name it in the step's file.
- Before the run, list each place the tool writes: its home, config, cache, and any socket or service. Find the option or variable that moves each place in the tool's help or docs, such as `HOME`, `XDG_CONFIG_HOME`, `CODEX_HOME`, or a `--config-dir` flag. Set each one under the root.
- A place that no option moves is live state. Ask the owner before the run touches it.
- Test each state that the change meets in its own root: a fresh home, a home with the old setup, and a home with a conflicting setup.
- Before and after the run, list the live places with a hash of each file. No live file may change.
- Copy the evidence into the step's file, then remove the root.

## Example
Initial state: a test must install the plugin `swarm` in Codex.
Action: make `/tmp/tmp.Ab12`, set `CODEX_HOME=/tmp/tmp.Ab12/codex`, hash the files under `~/.codex`, install, run the test, and hash them again.
Result: the plugin lists in the scratch home, the two hash lists of `~/.codex` match, and the root is removed.

## Check (script)
- none: the scratch root is outside the commit range, and the step removes it.

## Check (judge)
- Does the step name each place the tool writes and the option that moved it, and show the live hash lists from before and after as equal? Quote each place with its option, and both hash counts.
