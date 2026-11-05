# research cases

Each case is a question that an earlier research run answered and checked. Runs use solo mode: the
skill side follows the research steps but does the web step itself, because an eval pane may not
start seats. Both sides may use the web.

## case: agents-md
Source: four research runs where a web seat cited a dead or unsupporting link before the gate or
link check caught it (`2026-11-05-graph-skill-blocks-seats/03-gate.md:11,:14`,
`2026-10-21-dictation-asr/05-report.md:5`, `2026-10-21-todo-aggregator/05-report.md:5`,
`2026-11-03-agent-written-tests/04-report.md:22`). Known answer from `2026-11-05-graph-skill-blocks`.
Task: Does adding an AGENTS.md or similar repository context file help coding agents finish tasks? I need to decide whether my team should write one for each repo. Write a short report to report.md, with links to your sources.
Check (script): python3 -I <kit root>/presets/research/evals/check_links.py report.md
Expect:
- The report cites arXiv 2602.11988 (Evaluating AGENTS.md) or its page, and says that the context files gave no clear gain in task success.
- The report says that the context files raised cost or steps, with the paper's number (more than 20%).
- The recommendation follows from that evidence. It does not tell the team to write one file for each repo without a condition.
- Every link the report cites opens, and the page says what the report claims. The script result and the grader's own check agree.

## case: compiled-skills
Source: as for agents-md. Known answer from `2026-11-05-graph-skill-blocks`.
Task: Should an agent write its own skills from the task text before it starts work, instead of using skills that people wrote? I need to decide whether to build this into our agent kit. Write a short report to report.md, with links to your sources.
Check (script): python3 -I <kit root>/presets/research/evals/check_links.py report.md
Expect:
- The report cites SkillsBench (arXiv 2602.12670), and says that skills the model wrote itself did not match curated skills.
- The report gives a number from a source, such as SkillsBench's curated pass rate rising from 33.9% to 50.5%, or AIP's 0/15 for skills compiled from task text against 11/15 for human-written skills.
- The recommendation follows from that evidence, and keeps people-written skills as the default.
- Every link the report cites opens, and the page says what the report claims. The script result and the grader's own check agree.

## case: recent-compliance
Source: the 2026-11-05 full-mode run (`~/.claude/reports/2026-11-05-eval-agents-md`) and both solo
runs. No baseline found work newer than the famous paper, and the forward search
`reach.py paper 2602.11988 cites` found newer studies, such as arXiv 2611.04367 (policy wording
and compliance, 86 tasks) and "Zoro: Active Controls to Support Rule Following in Vibe Coding"
(2026-10-31).
Task: What has been published since 2026-09-01 about whether coding agents follow the instructions in AGENTS.md or CLAUDE.md files? I want the newest findings, not the well-known ones. Write a short report to report.md, with links to your sources.
Check (script): python3 -I <kit root>/presets/research/evals/check_links.py report.md
Expect:
- The report gives at least two findings whose own page dates them 2026-09-01 or later, and each measures or reports whether agents follow context-file instructions.
- The report does not present a source dated before 2026-09-01, such as arXiv 2602.11988 (2026-02), as a new finding. It may cite one as background.
- Every number the report gives matches its cited page, in value, measure, and scope.
- Every link the report cites opens, and the page says what the report claims. The script result and the grader's own check agree.

## case: origin-number
Source: the 2026-11-05 full-mode run, where the GPT seat gave "+4%" for developer-written files and
the dair.ai summary said "a 4% improvement" (`2026-11-05-eval-agents-md-seats/seat-gpt.md`,
`seat-claude.md`), and the solo Codex run that used old version 1 numbers. arXiv 2602.11988 v1 says
"an increase of 4% on average"; v3 says "improve agent performance by 2.4% on average (p=21%)",
with the benchmark renamed from AGENTbench to CTXbench.
Task: A blog says that in the ETH Zurich study "Evaluating AGENTS.md", developer-written context files raised task success by 4%. Is that the paper's number? Check the paper itself, say exactly where the number is, and say whether the gain is significant. Write a short report to report.md, with links to your sources.
Check (script): python3 -I <kit root>/presets/research/evals/check_links.py report.md
Expect:
- The report gives the current paper's number, 2.4% on average, and says it is not significant (p = 0.21).
- The report names where the number is in the paper (a section, a table, or a quoted sentence) and which version it read.
- The report explains the 4%, for example that version 1 of the paper said "an increase of 4% on average", or it says it could not find the source of the 4%. It does not call 4% the paper's current number.
- Every link the report cites opens, and the page says what the report claims. The script result and the grader's own check agree.
