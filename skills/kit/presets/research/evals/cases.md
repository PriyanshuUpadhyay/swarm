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
