---
status: proposed
date: 2026-10-09
deciders: [user]
related: ["0027", "0036", "0042", "0043", "0048", "0057"]
informed-by:
  - "Owner choices L1-L3, accepted 2026-10-06, closed flow 2026-10-06-unify-swarm/03-contracts.md:391-406"
  - "Assigned brief tmp/redesign/briefs/p5-skills-plan.md, slice 2 / build-order step 27 / G-10"
  - "Chair answer 1641 on 2026-10-09 clarifies the bundled copy refresh and skill-only links"
  - "Chair answer 1648 on 2026-10-09 settles the vendor link set, refresh verb and first-run Skills prompt"
  - "Chair answer 1651 on 2026-10-09 keeps refresh failure in Setup's error slot, following Phase 4's Settings-only error pattern"
---

# 0062. Swarm bundles the kit, and Setup records each skill link

## Context and Problem Statement

The app bundles its helper but no skills. The old install script links two swarm skills into the
three CLI roots without a managed-edit record. agent-kit has its own history and two vendor
submodules; the release workflow checks out without submodules. Dotfiles currently owns the kit
links and its skill projection rules, so two installers would compete for the same destinations.

ADRs 0027 and 0057 keep each build's data and owner choices apart. ADRs 0036, 0042 and 0043 require
proof of ownership, a recorded write, a visible plan and exact-match undo. This decision extends
those rules to skill links; it does not reverse their ownership or consent policy.

## Considered Options

- Import agent-kit with full history, bundle its files and record CLI links to a refreshed home copy.
- Copy the kit without history and keep the old install script.
- Link CLI skill roots directly into the signed app bundle.

## Decision Outcome

We chose the history-preserving subtree under skills/kit, a bundled default copy in this build's
swarm home, and managed links from each CLI root. The owner settled this choice as L1-L3 on
2026-10-06; this file remains a proposed ADR draft for the owner's final review.

- Import the kit without squash. Turn vendor/taste-skill and vendor/emil-skills into plain tracked
  files at their pinned commits, retain their licenses and note the source commits.
- Bundle the kit's skills, references, scripts, contracts, vendor tree and licenses with the two
  swarm skills. Leave Git metadata out of the app.
- Copy the bundle beneath the home that paths::root_dir resolves, at skills/. A release uses
  ~/.swarm/skills; a branch build uses its branch home. CLI links never point into the signed app.
- Refresh replaces the bundled copy whole when the bundle content version changes. It never
  merges owner changes. Edits to this disposable copy are not an owner overlay and a refresh
  replaces them. Slice 4 defines the separate overlay, Modified and Reset controls later.
- Stage and validate a complete copy before replacement, keep the prior copy on failure, and
  refresh before the app asks for setup. Copying inside swarm's home does not grant link consent.
- Dotfiles hands over first in one owner-run commit. Setup then links only skill folders, with
  writer skills and managed-edit kind symlink. It refuses all unrecorded destinations, including
  an identical or broken link. It does not claim links by their target text.
- Plan and undo inspect the link itself. Undo removes only the recorded exact target and never
  follows it into the skill files. Managed Changes shows each destination and its live state.
- Replace scripts/install.sh and its instructions; keep no migration or competing installer.
- Kit paths under ~/.claude/references, ~/.claude/scripts and private ~/.flow stay with dotfiles.
  Bundling the kit does not promise that every workflow runs on a fresh Mac without those inputs.

The chair settled three added details in answer 1648, with the owner free to overturn them here.
Use the kit README's vendor install set (taste-skill, animate and break-ui); keep other vendor
files bundled and unlinked. Add swarm skills refresh with no flags, so one tested writer reads the
sibling Contents/Resources/Skills and its content ID. A bare source build reports missing source.
This follows AGENTS.md's code-owned rule principle. Include Skills in first-run setup with its own
decline flag and reset, following ADR 0057 and the Phase 4 HooksSetupSheet pattern.
Run the separate refresh call after the app lock and notice service start and before Skills setup
reads. A refresh failure appears only in the Settings Setup error slot; it does not change the
app lock, notices, chats or other setup groups (chair answer 1651, Phase 4's error limit).

### Consequences

- Good: one app carries the kit, each link is visible and reversible, and kit history remains available.
- Good: app replacement does not break skill targets, and branch refreshes do not change the release copy.
- Bad: a release refresh discards direct changes to the bundled copy; owner editing waits for slice 4.
- Bad: foreign or another-build links need an owner handover before setup can use their destinations.
- Bad: the app carries vendor files, and hard-coded dotfiles support paths still limit fresh-Mac workflows.
