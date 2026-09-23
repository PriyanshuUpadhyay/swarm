---
status: superseded
superseded-by: 0010
date: 2026-09-17
deciders: [user]
related: [0001, 0002]
informed-by: []
---

# 0009. Ship the UI as Swarm, with no outbound reporting

In the context of using the Bloom fork as our own app, facing Spatie's identity in the bundle and
its install ping, crash reports, feedback upload and auto-updater that send data to Spatie's
services, we chose to rename the app's identity to Swarm (`io.github.priyanshuupadhyay.swarm`),
remove the install ping, Flare crash reports, feedback and prompt upload, the Sparkle updater,
Spatie branding and Spatie's release tooling, and keep code names such as `BloomCore`, and
neglected a full rename of targets and types and keeping Spatie's services switched off by
configuration, so the app sends nothing to a third party and reads as ours, accepting that the app
starts with fresh data, that updates come only from building the repository, and that later merges
from Spatie's Bloom need care. The MIT licence notice for Bloom stays.
