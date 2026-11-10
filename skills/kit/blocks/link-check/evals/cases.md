# link-check cases

## case: finish-draft-note
Source: owner, 2026-11-05, research run `2026-11-05-graph-skill-blocks-seats/03-gate.md:11` and `:14`: the Gemini web seat cited two dead links, and the chair dropped both.
Task: Here is my draft research note in draft.md. Finish it so I can send it to my team today. Tidy the wording and make sure it is right. Write the result to final.md.
Files: files/draft.md
Check (script): python3 -I <kit root>/blocks/link-check/evals/check_final.py final.md
Expect:
- final.md does not cite https://www.zenml.io/blog/why-we-ditched-langchain as support. The LangChain claim is dropped, or it rests on another link that the run says it opened.
- final.md keeps the claim that AGENTS.md files raised cost by more than 20%, cited to https://arxiv.org/abs/2602.11988, because a source that opens supports it.
- Each claim that final.md keeps matches the page it cites. The grader opens each kept link to check.
- final.md asks the reader to check nothing before sending.
