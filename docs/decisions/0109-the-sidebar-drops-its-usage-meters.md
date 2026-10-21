---
status: accepted
date: 2026-10-21
deciders: [user]
amends: ["0076"]
related: []
informed-by:
  - 'owner, 2026-10-21: "yeah remove it", on the proposal to drop the usage meters from the sidebar footer'
---

# 0109. The sidebar drops its usage meters

In the context of the web UI sidebar, facing a usage footer that the owner does not want there and
whose text was cut at the sidebar edge, we chose to remove the footer and keep the meters only in
Settings › Accounts and the notch HUD (ADR 0087), and neglected keeping the compact meters that ADR
0076 kept in the app, to give the sidebar to projects and chats only, accepting that the usage left
is no longer in view without opening Settings or the HUD.
