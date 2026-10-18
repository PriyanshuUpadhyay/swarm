---
name: sources
description: "Keep the lead-versus-evidence rule that the source blocks share with each selected source part. Use when a step searches, reads an origin, cites a claim, or checks links."
license: MIT
metadata:
  kind: parent
  owns: "shared source rules"
  serves: "research, web search, report"
  needs: "a question or claims with sources"
  gives: "shared rules for the selected source parts"
  conflicts: ""
  users: "research, web-search"
  source: "blocks/cite-sources/SKILL.md (a blog post, a forum answer, or a summary points to the owning source; it is not the evidence); blocks/primary-sources/SKILL.md (secondary pages and an answer from memory are leads)"
  parts: "primary-sources, deep-search, cite-sources, link-check"
---

## Rules
- A blog post, a forum answer, a summary, or an answer from memory points you to the source that owns a claim. It is a lead, not the evidence.

Each part keeps its own checks.

## Example
Initial state: a forum answer says "the API allows 50 requests per minute", and a blog says "databases use B+ trees".
Action: open the vendor's rate-limit docs by `cite-sources`, and the 1972 Bayer and McCreight paper that the blog names by `primary-sources`.
Result: the limit cites the docs page and the tree claim cites the paper. The forum answer and the blog are named as leads, not as evidence.

## Check (script)
- none: the claims and their links live in the report, outside the commit range.

## Check (judge)
- none: the judge checks of `cite-sources` and `primary-sources` ask whether each claim rests on its owning source.
