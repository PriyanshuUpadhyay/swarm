---
status: proposed
date: 2026-10-09
deciders: [chair]
related: ["0046", "0062", "0042", "0057"]
informed-by:
  - "owner, 2026-10-09: save bundled skill edits in the selected swarm checkout; without it, remain read-only"
  - "owner, 2026-10-09: edit the step table and each step's own section; preserve all other SKILL.md bytes"
  - "owner, 2026-10-09: use the Runs sidebar pattern from ADR 0046 and unify-swarm/01-frame.md:260"
  - "chair, message 2026, 2026-10-09: git owns the source edit record and revert; direct app writes have no managed-edit row"
  - "chair, message 2026, 2026-10-09: only existing numbered step headings have editable text"
  - "chair, message 2028, 2026-10-09: a skill's own script owns its step IDs and locks structural edits"
  - "tmp/flow/2026-10-09-redesign-editor/01-frame.md through 04-impact.md"
---

# 0066. Swarm edits bundled skills in the owner's checkout

## Context and Problem Statement

The owner wants to change a bundled skill's steps and instructions beside the chat.
For example, they change research's 01-question section and save its SKILL.md in their swarm checkout.
The signed bundle cannot hold owner edits, and ADR 0062 refresh replaces the copy under swarm home.

ADR 0046 provides a sidebar graph and direct app file reads.
ADR 0057 puts owner settings in files in the build's home.
ADR 0042 puts external setup writes through managed edits, so source writes need an explicit scope exception.

## Considered Options

- Keep an overlay under ~/.swarm, with Modified and Reset to bundled controls. This permits immediate local changes but adds a second source and reset policy.
- Edit the owner's swarm checkout. Git holds the change and revert, and the next release bundles the commit; this requires a checkout and a later release.
- Support both checkout and overlay edits. This serves both uses but needs precedence and two write paths.
- Add a swarm skill-save verb, or let SwarmCore write source files directly. A CLI verb centralizes writes but adds a second step model and process calls.

## Decision Outcome

We chose the owner's checkout as the only editable source (owner, 2026-10-09).
This file remains a proposed draft for final review.
Skills sits beside Runs in the sidebar and lists every bundled skill.
A supported File/Needs/Holds table supplies its graph; a skill without that table stays read-only with a reason.

The checkout field lives in Prefs through OwnerChoicesStore in choices.json.
Without a valid checkout, the graph stays read-only and Set checkout opens Settings Skills.
Save writes only <checkout>/skills/kit/skills/<skill>/SKILL.md.
It keeps front matter and all bytes outside the first supported table and the step's own section ranges unchanged.
It does not commit, change live runs, refresh installed skills, or write into the bundle or disposable copy.

The app writes source directly and rejects a save when the loaded file hash changed on disk.
It keeps the draft, states the conflict and offers Reload.
The chair approved an explicit exception to ADR 0042 for this owner-selected git source tree in message 2026.
Git owns its record and revert, so the save has no managed-edit row.
Managed edits continue to own writes into CLI roots and other non-git places.
The accepted ADR 0042 body stays unchanged; this draft states the scoped exception.

Text editing requires an existing exact numbered step heading, such as ## 01-question.
Research has these sections; flow, council and web-search show no step section in SKILL.md for their node text.
The chair approved this limit in message 2026.
The chair also said in message 2028, "this skill's script owns its step ids".
For a skill with its own step script, such as flow, Add, Remove, Rename and Reorder stay locked.
Needs, Holds and any existing step text remain editable.
Generic step_run.py skills permit all table edits; rename and reorder update bound heading names and dependency identities.

### Consequences

- The owner gets a reviewable source change with git history and revert, and one release source remains authoritative.
- The app reuses the Runs graph presentation and the existing settings owner without adding a CLI verb or runtime overlay.
- The owner needs a local checkout, and saved changes reach installed skills only after a commit and release.
- The app keeps a Swift copy of the kit table rules, so saved fixture output must be checked with step_run.py.
- Some skills expose only table cells, and script-owned skills lock structural changes until their scripts can support them.
- The source writer needs path, byte-preservation and revision checks; atomic replacement alone does not detect a stale draft.
