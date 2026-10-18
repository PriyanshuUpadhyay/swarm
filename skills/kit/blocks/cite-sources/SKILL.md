---
name: cite-sources
description: "Tie each claim to the source that owns it, quote only text you read, count repeated sources once, and mark weak or snippet-only sources. Use when you write a finding or a report that cites evidence."
license: MIT
metadata:
  kind: rule
  owns: "how a claim cites its evidence"
  serves: "research, web search"
  needs: "claims with their sources"
  gives: "claims that each cite their owning source"
  conflicts: ""
  users: "research, web-search"
  source: "skills/research/SKILL.md (03-web owner sources, 05-report conversion check, Guards, moved here 2026-10-13); skills/web-search/SKILL.md (Yield source rules, method, Guards)"
---

## Rules
- This block answers how well sources agree. For proof strength, see `blocks/prove-it-works/SKILL.md`; for the claim-to-proof map, see `blocks/done-when-evidence/SKILL.md`.
- Follow each claim back to the source that owns it: the official docs, the source code, the spec, or the first-party API. A blog post, a forum answer, or a summary points you to that source; it is not the evidence. When you cannot reach the owner, cite the secondary page and mark the claim "(secondary)".
- Two sources that cite the same origin count as one source. A homepage or a product page is not evidence for a specific claim.
- Never invent a quote. Quote only text you read.
- Mark "(snippet only)" when a page could not be opened and only its search snippet was there. Never present a snippet as a read page.
- When you turn a page, a PDF, or a table into text, compare its headers, units, and values with the source, and keep the page or cell location in the citation. If the conversion lost structure, cite the original.
- For a search finding, label source agreement `high` when two independent primary sources agree, `medium` when evidence is mixed or secondary, and `low` when it has one source or inference. This scale does not replace visual provenance labels or the proof ladder.
- Prefer sources from the last two years. Say so when the best source is older.

## Example
Initial state: a forum answer says "the API allows 50 requests per minute".
Action: open the vendor's rate-limit docs, find the number there, and cite that page.
Result: the claim cites the docs page. Had the docs been unreachable, the claim would cite the forum answer and read "(secondary)".

## Check (script)
- none: the claims and their links live in the report, outside the commit range.

## Check (judge)
- Does each claim cite the source that owns it, or carry "(secondary)"? Quote two claims with their links.
- Is any quote not found on its cited page? Quote it, or write "none".
- Is any snippet presented as a read page? Quote it, or write "none".
