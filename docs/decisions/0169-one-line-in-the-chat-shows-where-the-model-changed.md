---
status: accepted
date: 2026-11-26
deciders: [user]
amends: ["0099"]
related: ["0168"]
informed-by:
  - 'owner, 2026-11-26: "Yes, defaults okay." (one line in the chat shows where the model changed, as an exception to ADR 0099).'
---

# 0169. One line in the chat shows where the model changed

## Context and Problem Statement

ADR 0099 says the chat shows what the agent CLI shows, and nothing more. The app adds no row of
its own to the transcript. When one chat holds more than one model or CLI (ADR 0168), the owner
cannot see in the transcript where the change happened.

## Decision Outcome

- The line where the model changed is the one app line in the transcript. ADR 0099 stays the
  rule for every other row.
- The line shows for a switch in the same CLI and for a switch to another CLI. The web builds it
  from the log (`session_info model`) and the session chain. A new table is needed only if the
  route cannot join the chain.
- The line is one quiet row. A hook, a cwd or a usage notice is not shown; those stay hidden.

## Consequences

- Good, because the owner sees where the model changed, and the rest of the transcript stays as
  the CLI shows it.
- Bad, because the exception can grow. Each new app row needs its own record.
- The line is built in run 4.
