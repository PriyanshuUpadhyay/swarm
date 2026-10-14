---
status: accepted
date: 2026-10-14
deciders: [user]
related: ["0079"]
informed-by:
  - 'owner, 2026-10-14: "Nobody is using this binary just yet. We can delete the older ones if we really do want. We can delete all tags and create a new release 0.0.1 as well."'
---

# 0082. The release history starts again at 0.0.1

In the context of the GitHub releases and tags of swarm, facing a history up to 0.13.1 that no user
installed and a 0.13.0 draft whose build failed, we chose to delete every release and tag and to
publish the formula of ADR 0079 as 0.0.1, to achieve a version history that starts with the first
release that ships as one formula with the plugin, accepting that the old tags are gone (their
commits stay in `main`) and that a Mac on 0.13.1 must run `brew reinstall`, because Homebrew sees
0.0.1 as older.
