---
status: superseded
superseded-by: 0018
date: 2026-09-16
deciders: [user]
related: [0005]
informed-by: []
---

# 0004. Auto picks the account with the most usage left

In the context of launching an agent from the Bloom UI, facing several signed-in accounts per
provider, we chose to let the user pick the role (for example `code.complex`) while "Auto" picks the
account with the most usage left, the answer `yelo profile pick` gives, and neglected auto-picking
the role from the task text and falling back to another provider's runner when usage is low, to
keep the role an explicit choice and spread load across accounts, accepting that a provider whose
accounts are all low still launches on the least-bad account.
