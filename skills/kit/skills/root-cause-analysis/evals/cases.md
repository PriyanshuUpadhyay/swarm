# root-cause-analysis eval cases

Each case runs on a clone of swarm at `a0322094`. `data/swarm.db` in the clone is a planted,
read-only copy of a production swarm store, with the real schema and synthetic rows. The pane
writes its RCA into `replies.md` beside the clone, and changes no tracked file. `data/` and `tmp/`
are in the clone's `.git/info/exclude`. The baseline is no skill.

## case: data-is-there
Source: owner (develop): "We already have the debug worker ... We don't have to second guess
everything."; owner (develop): "We don't need a deployment for encoder."
Task: From the team chat: "In session `s-rv` I sent an ask to `rv-2` at 14:02 UTC on 2026-11-04,
and it never answered. Find why." A read-only copy of the production store is at `data/swarm.db`.
Files: `data/swarm.db`. `rv-2` went to `waiting` with `state_kind = 'permission'` at 14:01:30.
Ask seq 40 to `rv-2` has `rings = 2`, `delivery = 'unconfirmed'`, and `seen_at` NULL, and
`rv-2` sent the report `unconfirmed:40`. `rv-1` got and acked its ask.
Check (script): `git status --porcelain` in the clone prints nothing; `replies.md` is outside it.
Expect:
- The RCA names the cause from the data: `rv-2` waited on a permission prompt from before the ask,
  so it never saw seq 40.
- The RCA quotes at least one query on `data/swarm.db` with the rows it returned.
- The RCA asks for no new logging, rebuild, or repro before it reads the data.

## case: code-is-a-lead
Source: owner (develop): "verify looking at the data"; owner (fix-chat-history-cache): "dont
assume."
Task: From the team chat: "The app shows no cost and no tokens for the Codex agents in session
`s-b`, but the Claude agents there show cost. Codex cost parsing must be broken. Find the root
cause." A read-only copy of the production store is at `data/swarm.db`.
Files: `data/swarm.db`. In `s-b`, Claude agents `c-1` and `c-2` have a `log` and a cost, and Codex
agents `x-1` and `x-2` have `log`, `cost_usd`, and `tokens` NULL. In `s-a`, Codex agent `x-9` has a
`log`, `cost_usd = 0.42`, and tokens.
Check (script): `git status --porcelain` in the clone prints nothing; `replies.md` is outside it.
Expect:
- The RCA says that the Codex agents in `s-b` have no `log`, and that `update_usage` in
  `src/store/usage.rs` returns before it reads usage when `log` is NULL.
- The RCA does not confirm "Codex cost parsing is broken", and it cites `x-9` in `s-a`, a Codex
  agent with a log and a cost.
- The RCA gives the scope from the data: 2 of 2 Codex agents in `s-b`, and no Codex agent with a
  log.

## case: what-broke-first
Source: owner (develop): "I first need to understand what is actually broken here."; owner
(fix-chat-history-cache): "Can you give an example of the issues that we are facing? Right now
the explanation is all over the place".
Task: From the team chat: "swarm is broken for us: panes stall all the time, the HUD shows the wrong
cost, and messages arrive twice. Session `s-c`. Find the root cause and what to fix." A read-only
copy of the production store is at `data/swarm.db`.
Files: `data/swarm.db`. In `s-c`, agents `w-1`, `w-2`, and `w-4` read and ack each ask. `w-3` reads
5 asks (`seen_at` set), and each time its turn ends before it acks: 4 acks come late, and the
fifth has no `read_mark` row. All 5 `stall:unacked:<seq>` reports come from `w-3`. No message is stored twice, and the costs are ordinary.
Check (script): `git status --porcelain` in the clone prints nothing; `replies.md` is outside it.
Expect:
- Before any fix, one sentence says what broke, with a real instance: `w-3` reads its asks but does
  not ack them before its turn ends, so all 5 stall reports in `s-c` are about `w-3`. (Made exact
  before round 1: `stalls()` reports one unacked message at a time.)
- The RCA says that the data does not show the wrong-cost complaint or the twice-delivered
  complaint, with the query that checked each.
- Fix options come after the cause.
