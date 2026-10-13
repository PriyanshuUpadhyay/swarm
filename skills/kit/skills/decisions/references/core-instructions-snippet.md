# Core-instructions pointer snippet

Paste this into the repository's always-loaded agent instruction file so future sessions read
prior decisions before proposing changes and know to record new ones. Keep it short; the
procedure lives in the on-demand `decisions` skill.

```markdown
## Decisions
Architecture/design decisions live in `docs/decisions/` (ADR format). Read the README
index there before proposing or making changes that touch a prior decision. Use the
`decisions` skill to record significant decisions and reversals — it batch-asks for the
"why" before commits. Records are immutable: reverse a decision by superseding it, never
by editing or deleting the original.
```

When bootstrapping an existing repo, offer to add this and confirm the target instruction file
before writing it.
