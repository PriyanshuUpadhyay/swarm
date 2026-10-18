# Core-instructions pointer snippet

Paste this into the repository's always-loaded agent instruction file so future sessions read
prior decisions before proposing changes and know to record new ones. Keep it short; the
procedure lives in the on-demand `decisions` skill.

```markdown
## Decisions
Before proposing or making a change that touches a prior decision, read `docs/decisions/README.md`
and its relevant records.
Use the `decisions` skill for project decision records. Read its `SKILL.md` and
`references/capture-loop.md` for prior-decision lookup, capture, confirmation, flush timing,
and reversal rules. Its Storage rules govern the ban on changes to record bodies and the allowed
field changes.
```

When bootstrapping an existing repo, offer to add this and confirm the target instruction file
before writing it.
