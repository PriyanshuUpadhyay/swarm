---
status: accepted
date: 2026-09-16
deciders: [user]
related: [0005]
informed-by: []
---

# 0006. Show every account in Bloom's menu bar usage meters

In the context of usage tracking in Bloom, facing a separate UsageHUD app that already shows every
account, we chose to feed Bloom's existing menu bar meters with every account from `swarm usage` and
to show usage left beside each account in the launch picker, and neglected showing usage only in
the picker and replacing UsageHUD with a floating window, to see usage where agents are launched
without new window chrome, accepting that UsageHUD and Bloom show the same numbers in two places.
