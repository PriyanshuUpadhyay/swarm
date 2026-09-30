---
status: accepted
date: 2026-09-30
deciders: [user]
supersedes: "0018"
related: ["0030", "0031"]
informed-by:
  - "User answer on 2026-09-30 (flow 01-frame): chat is a profile and is the first one in the list; the picker revamp comes later"
---

# 0032. Chat is a profile

In the context of starting a chat from the app, facing a picker whose choice lived in UserDefaults
with effort fixed at `medium`, we chose to make `chat` a profile like every other, first in the
list, so New Chat opens on the runner the chat profile would launch and a chat gets the profile's
effort and fallbacks, while a different pick in the sheet is a one-off with no fallback that is not
saved, and neglected keeping the provider and model picker as the only chat setting (ADR 0018), to
give chat the same effort and fallback control as every other role, accepting that a model picked
once must be picked again next time.
