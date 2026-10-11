---
status: proposed
date: 2026-10-09
deciders: [chair]
related: ["0036", "0042", "0043", "0057", "0063"]
informed-by:
  - "Owner layer model on 2026-10-06: bundled default plus owner overlay, Modified and Reset to bundled"
  - "Owner L4 on 2026-10-06: login runs the provider's CLI, tokens stay in its store or keychain"
  - "Chair decisions in swarm messages 1655 and 1659 on 2026-10-09; the owner can overturn this proposed record"
  - "Chair answer in swarm message 1644 on 2026-10-09: accounts.toml in the swarm home, Reset clears metadata only, credentials and homes are never touched"
---

# 0065. Account Reset clears Swarm metadata only

## Context and Problem Statement

Settings uses a bundled layer and an owner overlay. Accounts also use native CLI homes and
credential stores. A Reset control needs a clear boundary so it cannot erase credentials or
remove a CLI home. ADR 0057 puts owner choices in the Swarm home.

## Considered Options

- Store nonsecret account metadata in accounts.toml in the Swarm home and reset only that overlay.
- Reset account metadata, native homes and credentials together.
- Keep account choices in UserDefaults or yelo.

## Decision Outcome

Proposed from chair answer 1644: accounts.toml under the resolved Swarm home owns nonsecret
account metadata. It has an empty bundled default and an owner overlay marked Modified.
Reset clears only Swarm metadata. Native CLI homes, token files and keychain items remain
untouched. Login leaves credential writes to the CLI. Rejected: erase homes or credentials
because Reset is a layer action, not logout; UserDefaults or yelo because ADR 0057 and L5 put
owner choices and account ownership in Swarm. Chair 1655 accepts version = 1 and [[accounts]] entries with provider, name and absolute home
references. It accepts native home allocation and metadata registration before pane open.
The owner can overturn these chair choices in this proposed record before build.

Use the existing linked-file, revision, lock and atomic-write rules. Discovery can still show
a native account after Reset; the file layer controls metadata, not whether a CLI is signed in.

### Consequences

- Good: Reset is bounded, and Swarm files can sync without a secret.
- Bad: Reset cannot sign an account out, and native accounts can remain visible after it.
