---
name: research
description: Research one question from evidence and write a short report with checked links. It runs two web jobs, recent underrated finds and the origin of each core claim. Use when the user says research, look into, find out, compare, or what others do, or asks for recent writeups or where a claim comes from.
---

# Research

Answer one question from evidence, in a short report with links that open. The kit root is two
folders up from this skill's folder. `reach` below means
`python3 <kit root>/blocks/web-search/scripts/reach.py`.

## Steps

1. **Frame.** Write the question in one line and the decision it serves in one line. List the
   answers you already know. They are background, not findings.
2. **Local first.** Read earlier reports in `~/.claude/reports/` and the repository files that the
   question names, as `<kit root>/blocks/local-research/SKILL.md` says.
3. **Recent and underrated.** Set a window from the question, or 90 days. Search where
   practitioners write first.
   - `reach search <site> "<query>" --days N` for `hn`, `reddit`, `x`, `lobsters`, `github`,
     `youtube`, and `web`. Use two or three queries per site, one of them for the opposite view.
   - Forward search: `reach paper <arxiv id> cites` on the key paper finds newer work that builds
     on it.
   - Keep an item only when you opened it and its own page dates it inside the window.
4. **Origins.** For each core claim, find the paper, spec, or code that first made it. Read the
   exact section, and quote it with its location.
   - `reach paper <arxiv id> refs` walks back to older sources. `reach read <url>` opens a page that
     blocks a plain fetch.
   - Use the newest version of a paper. When versions give different numbers, say which version
     says what.
   - A blog, a summary, or an abstract is a lead, never the origin. Name it "(secondary)" when you
     cannot reach the origin.
5. **Check each number before you write.** Open the cited page again with `reach read` and compare
   the number, its unit, what it measures (odds or a rate), its scope (which files, models, or
   tasks), and the version. Fix or drop each claim that does not match.
6. **Write the report** as `<kit root>/blocks/report/SKILL.md` says: the answer in two or three
   sentences, then Evidence, What is contested, Off the usual path, and Sources newest first with
   dates. Keep process out of the report, so no search log, checklist, or run notes.
7. **Check every link** as `<kit root>/blocks/link-check/SKILL.md` says.

When `reach` prints a `blocked:` line, put that line in the report's gaps. Do not call a site
blocked without it.

## Example

Question: "A blog says LSM-trees were designed for SSDs. Is that right?" Step 4 finds the origin,
the 1996 paper by O'Neil and others, and reads its introduction, which frames the cost as disk arm
movement under high insert rates. The report quotes that passage with its section, and names the
blog as the lead.

## Full mode

When a swarm host can open visible panes and the question is about practice or opinion, run steps
3 and 4 with web seats as `<kit root>/blocks/web-search/SKILL.md` says. Step 5 still applies to every
number that the seats bring back.

## Guards

- Every claim cites the source that owns it, as `<kit root>/blocks/cite-sources/SKILL.md` says.
- Do not edit any repository. Write only the report and your own notes.
