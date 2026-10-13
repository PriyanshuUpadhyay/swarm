---
name: deep-search
description: "Search past the famous answers: name them as a baseline, seed lesser-known options, split the question, dig below page one, and look for the opposite view. Use when a search must find what the user does not already know."
license: MIT
metadata:
  kind: rule
  owns: "how a search goes past the known answers"
  serves: "research, web search"
  needs: "a question and the answers the user already knows"
  gives: "sub-questions, a known list, seeds, and off-path ideas with sources"
  conflicts: ""
  users: "research, web-search"
  source: "skills/research/SKILL.md (Off the usual path, moved here 2026-10-13); skills/web-search/SKILL.md (Depth, a hard rule: sub-questions, known list, seeds, how to dig, moved here 2026-10-13)"
---

## Rules
- Search engines and answer engines rank the same famous names first, and the user already knows them. Name them as "known, baseline only". Report each in one line as a comparison, never as a finding.
- Seed the search with candidates you suspect are under-discussed, from prior reports and your own knowledge, so it starts below the surface. An empty seed list needs a reason.
- Split the question into three to six sub-questions before you search, because the first sources found are the easiest to find, not the most representative. Report per sub-question. A sub-question with fewer than two independent sources gets a gap note, never a stretched finding.
- Dig: sort by new, not top; read the comments, not only the post; follow a thread to the author's repo, model card, or paper; search the names of the technique, the format, and the runtime, not only the product; on GitHub and Hugging Face sort by recently updated and read repos with few stars; search non-English communities when the topic has them.
- Search off the usual path on purpose: the opposite of the usual answer, such as "we removed X" or "X considered harmful"; the same problem in another field or community; a way to drop the problem instead of solving it; old or abandoned tools whose idea is still good; people who tried the popular answer and moved away, with their reason.
- Keep each off-path idea that has a real source, even when it looks wrong at first. Write why it might work and why it might fail. It does not replace the main answer without evidence.

## Example
Initial state: the question is "which small model cleans up dictation text". A surface pass returns Whisper and Qwen.
Action: list those two as "known, baseline only", seed Moonshine, SenseVoice, and Granite, split the question into accuracy, speed, and on-device runtimes, and search for people who replaced a general model with a small purpose-built one.
Result: fifteen candidates that the surface pass missed, and one off-path idea, a rule-based cleanup that a team kept after it removed its model, with why it might work and why it might fail.

## Check (script)
- none: the search notes and the report live outside the commit range.

## Check (judge)
- Is each famous answer listed as "known, baseline only" and not counted as a finding? Quote the known list.
- Does each sub-question have two or more independent sources or a gap note? Quote the matrix or the notes.
- Does each off-path idea have a source, a reason it might work, and a reason it might fail? Quote one, or the searches tried when none held up.
