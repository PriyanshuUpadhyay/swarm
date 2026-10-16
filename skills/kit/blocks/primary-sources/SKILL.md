---
name: primary-sources
description: "Go to the origin of a claim, the original paper, RFC, spec, design doc, or the author's code at the commit that introduced it, read it, and ground the answer in it. Use when a question is about how or why a technique, protocol, or implementation works."
license: MIT
metadata:
  kind: step
  owns: "finding and reading the origin of a claim, and the shape of an origin evidence row"
  serves: "research, web search"
  needs: "a question about a technique, protocol, format, or implementation"
  gives: "origin evidence rows, and a note for each claim whose origin could not be reached"
  conflicts: ""
  users: "research, web-search"
  source: "owner request 2026-10-16 (a separate block for original papers and original code); skills/kit/blocks/cite-sources/SKILL.md (the rule that a secondary page is not the evidence, extended here from docs to origins)"
---

## Rules
Find the origin
- Name the origin of each core claim before you answer: the paper that introduced the technique, the RFC or spec that defines the behavior, the design doc or proposal that made the decision, or the reference implementation. A blog, a tutorial, a textbook summary, or an answer from memory is a lead to the origin, not the origin.
- Walk back from a lead. Follow a paper's citations and a later paper's "related work" to the first paper. Open a spec's changelog or errata for the version that added the behavior. For code, take the reference repo and run `git log -S '<symbol>' --reverse` or `git blame` on the line, then read the commit and the pull request or issue it names. Read the author's own repo, not a fork.
- When two origins exist, such as a paper and the code that later drifted from it, read both and say where they differ. The paper owns the idea; the code at the commit owns what runs.
Read it
- Open the origin and read the part that carries the claim, not its abstract or its README. For code, read the function, not the commit message.
- Ground the answer in what the origin says. When a secondary source and the origin disagree, the origin wins and the disagreement goes in "What is contested".
- When the origin is long, read the section the claim lives in and the section that states its limits or assumptions. Keep both in the row.
Record it
- Each origin row holds three things: the source (title, authors or owner, year, version, and link), the exact location, and a quote you read there. The location is a section, page, or figure number for a paper or spec, and `path:line@commit` (the full or 10-digit commit) for code.
- Count origins as the kit block `blocks/cite-sources/SKILL.md` says, and cite and mark them as it says. This block adds the row shape and the search for the origin only.
- When the origin cannot be reached (paywall, dead link, private repo), try the author's page, an archive, and a preprint server, then mark the claim "(origin not reached)" and cite the best secondary page as `blocks/cite-sources/SKILL.md` says.
When it does not apply
- Skip this block for a question that has no origin to read, such as a current price, an opinion poll, which tool people prefer, or a how-to for one product's UI. Write `skipped: <reason>` in the step file.
- Skip it for a claim that the vendor's current docs own, such as a rate limit or a flag. The docs are the origin there, and `blocks/cite-sources/SKILL.md` covers them.

## Example
Initial state: the question is "why does a B-tree index keep keys in sorted pages, and how do real databases differ from the textbook form". A blog says "databases use B+ trees".
Action: the blog names Bayer and McCreight, so find the 1970 paper and read its section on page size and balance. Then take the PostgreSQL repo, run `git log -S '<symbol>' --reverse -- src/backend/access/nbtree` for a symbol the lead names, and read the first commit and the `README` in that folder, which names the concurrency paper it follows.
Result: three rows. Row 1: Bayer and McCreight, "Organization and Maintenance of Large Ordered Indexes", Acta Informatica 1972, section 2, quote of the page-fill rule. Row 2: Lehman and Yao 1981, section 3, quote of the right-link rule. Row 3: `src/backend/access/nbtree/README:<line>@<commit>`, quote of the line that says how the code follows that paper. The report says where PostgreSQL differs from the paper, and the blog is listed as the lead.

## Check (script)
- none: the origin rows live in the step file or the report, outside the commit range.

## Check (judge)
- Does each core claim have an origin row with a source, an exact location (a section or `path:line@commit`), and a quote? Quote two rows.
- Is any origin row only a secondary page, a README, an abstract, or a homepage? Quote it, or write "none".
- Where a paper and its code, or an origin and a later write-up, disagree, is the disagreement stated? Quote it, or write "no disagreement found".
- Is each claim whose origin was not reached marked "(origin not reached)" with the tried routes? Quote one, or write "none".
- If the block was skipped, is the reason a real "no origin to read" case? Quote the reason.
