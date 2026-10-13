---
status: accepted
date: 2026-10-05
deciders: [user]
related: ["0005", "0014", "0020", "0037"]
informed-by:
  - "User answer on 2026-10-05 (flow 01-frame): slice A, a step-run view, goes first"
  - "User answer on 2026-10-05 (flow 03-contracts, tmp/flow/2026-10-05-extensible-swarm): accepted all recommendations, Q1 A (Uses names written at start), Q2 A (Revision: HEAD line, the app asks git), Q3 A (workspace tmp/ only)"
  - "~/.claude/references/step-run.md: line 1 status, line 2 Uses, revision by file hash"
  - "Seven real runs on 2026-10-05: flow steps have an empty Uses line until take"
---

# 0046. Show step runs as a graph in the sidebar

## Context and Problem Statement

Agents run skills such as flow, research, and review-walk as step runs: a folder of `NN-<name>.md`
files with a status on line 1 and the needed steps on line 2. The owner reads these files by hand to see
which step is active, which waits for an answer, and which is stale.

## Considered Options

- A sidebar view, or a tab in the main area.
- Read the files in the app, or add `swarm runs --json`.
- Take the need graph from the `Uses:` lines, from the skill's step table, or from a graph file.
- Take flow's build revision from a `Revision: HEAD` line, from line 1's `done <rev>`, or from a
  flow rule in the app.

## Decision Outcome

Chosen: a sixth sidebar view, Runs, that lists the runs under the workspace's `tmp/<skill>/` and draws
one run as a top-down graph, with a status glyph, todo count, stale mark, and the waiting question on
each node. A node opens its file in the read-only preview. SwarmCore reads the files itself and polls
every second while the view shows. The edges come from the `Uses:` lines, which the kit's `start` fills
with the need names; a step that names `Revision: HEAD` takes its revision from git. An older run with
an empty `Uses:` line gets an assumed edge from the previous step. Rejected: a main-area tab, because it
hides the chat; a CLI verb, because Swarm's Rust code has no step-run concept and a 1 s poll would start
a process each second; the skill's table, because the skill need not be on this Mac; line 1's revision,
because it hides a new commit and a hand edit.

### Consequences

- Good: the owner sees a run beside the chat, without opening files.
- Good: no new CLI verb and no new dependency; a fresh Mac needs no kit to see a run.
- Bad: the app keeps a copy of the step-run rules; a change in step-run.md needs a matching change and
  fixture in SwarmCore.
- Bad: runs outside the workspace, such as research reports in `~/.claude/reports/`, do not show yet.
- Bad: a run started before the kit wrote `Revision: HEAD` shows its review as stale against the build.
