# simplest-design eval cases

Each case runs on a clone of swarm at `7b1752f3`, on the branch `main`, with the tags of the real
repository. The pane proposes a design and writes no code. It writes its reply into `replies.md`.
The baseline is no skill.

## case: value-already-there
Source: owner (cache-hits-across-llm-calls): "Usage is already the total of it. We
don't need total usage."; owner (debug-ui): "we don't need the cache key here."
Task: When a research seat runs `skills/kit/blocks/web-search/scripts/reach.py`, it can't tell
which route gave the results, so it can't say where a hit came from. Propose a design for this
before any code.
Files: none
Check (script): none
Expect:
- The design names the `reach: <route> ok` line that `ladder()` already prints to stderr.
- The design adds no new field to the JSON lines, no new flag, and no new file.

## case: fixed-list-no-config
Source: owner (hono-route-trees): "can it not part the setup that we are doing ... so
that we don't need to wrap it every single time"; owner (tmux-has-no-focus-issue):
"dnd bypass is not needed".
Task: `skills/kit/presets/implement/scripts/size_gate.py` counts lock files such as
`package-lock.json` and `Cargo.lock` toward the line limit, so a dependency bump leaves the small
route. Propose a design so lock-file lines don't count, before any code.
Files: none
Check (script): none
Expect:
- The lock-file patterns are a constant in `size_gate.py`. No config file, repo file, flag, or
  environment variable sets them.
- The design changes only `size_gate.py` and its test file.
- A lock file under `tests/` is subtracted from the line count once, not twice. (Added after
  round 1, from the grader's finding.)

## case: unreleased-rename
Source: owner (fix-chat-history-cache): "dont need a migraiton coz that's not in prod
yet".
Task: The `pair-programming` skill writes a `Base:` line in its task file, and the `implement`
preset uses `Base:` in `05-build.md` for another value. Rename the pair one to `Upstream:`.
Propose a design before any code.
Files: none
Check (script): none
Expect:
- The design keeps no fallback that still reads `Base:` in a pair task file, and no migration of
  old task files.
- The design says in a sentence that no release holds the field yet. A quoted command such as
  `git tag --contains` with no such sentence does not pass. (Made exact after round 1.)
