# To do

## swarm spawn checks the model and reports what went wrong

`swarm spawn` (and `launch`) should check that the routed model exists for the provider and
account before it opens a pane. When it does not, or when anything else fails at spawn, the
error goes back to the orchestrator during the spawn call, with the potential fixes that might
work (another account, another model, a trust entry, a missing adapter verb). If a suggested fix
does not work, the orchestrator writes to a file that the suggestion failed, so we can later
implement the fixes that worked and drop the ones that did not.

Seen 2026-09-22: two Codex seats exited at launch with only the profile line printed, and the
chair learned nothing until it read the panes by hand.

## Real screen captures for Codex and AGY agent states

The screen check (ADR 0021) reads Codex and AGY panes with patterns taken from their source code,
not from real screens (`tests/fixtures/screens/README.md`). Research the real Codex and AGY
screens for idle, working, approval, and failure, capture them under a private tmux server, and
replace the synthesized fixtures. Also research how to confirm a state for an adapter that has no
`screen` verb; today its hook state turns to unknown after 45 s.

## Main-thread stops with 12 live panes

With 12 streaming stress panes, the Animation Hitches instrument recorded hangs of 0.27 to 1.4 s,
but no hitch over 33 ms. In the one hang traced with Time Profiler, no Swarm thread ran at all, and
the machine load was 5 to 18 on 10 cores. Find out on a quiet machine whether the app or the
system causes them (`SWARM_PANE_STRESS=12`, see ui/README.md).


## A ring to a new pane can lose its Enter

Seen 2026-09-30: `swarm launch rc-8ca4dc1-fix3 review.deep` opened a Fable pane, and `swarm send`
typed the ring into its prompt a few seconds later. The text arrived, but the Enter did not, so the
seat sat idle with the ring in its input box. `swarm sweep` re-rang and typed the text a second
time, again with no Enter. Pressing Enter by hand (`herdr pane send-keys wGS:pH Enter`) started the
seat. The Enter probably lands before the new CLI's input box is ready. Fix it so that a ring to a
pane is known to be submitted, for example by checking the screen for the ring text after Enter and
pressing Enter again.

## Agent profiles: open design choice

- The profiles page group header is a Button with a turning chevron. The design named a
  DisclosureGroup. Decide which one stays.

## Agent profiles: known deviations

- The profiles page rows are a LazyVStack, so they have no arrow-key selection.
- Model ids show in full in a runner chip; they are not shortened.
- The profiles page has no Restore defaults, because swarm has no command for it.
- A launch with `--account` reads yelo twice (the probe and the account pick), so a stuck yelo
  costs 4 s, not 2 s. One read could serve both.
- A hung yelo's own child process can outlive the kill after the 2 s limit.
- `argv` stays in `bus.rs`, dispatching on the `Provider` enum, not in `providers.rs`.
- The profile schema lives in `src/config.rs`, not `src/profiles.rs` (ADR 0031 names it).
- `swarm roles save` takes the profile JSON as an argument, not on stdin.
- New Chat shows skipped runners in its chain row before launch, not in the progress line during
  launch.
- In dotfiles, `tests/orchestration-extraction/test_orchestration_contract.py:68` still reads
  roles.json, and ADRs 0009 and 0015 still name roles.json as the launch policy source.
- The editor measures its card list again only when the number of card lines drops. A card that
  swaps a taller line for a shorter one (a catalog error for a warning) keeps 7 pt of
  empty space or more, since a wrapped error line is taller, until the next change.
