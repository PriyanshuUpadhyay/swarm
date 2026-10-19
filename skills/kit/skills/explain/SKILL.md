---
name: explain
description: Explain how something works. Use when the user asks to understand a subject or says "explain this".
---

# Explain

For owner gates, apply `blocks/worker-brief/SKILL.md`. The default is to explain from the sources at hand and name each unresolved point.


The kit root, with its `blocks/` and `references/` folders, is two folders up from this skill's folder.

Read `blocks/voice/SKILL.md`, then `blocks/voice-teach/SKILL.md` for the order of ideas, and `references/explain-formats.md`
for the form of the answer. Apply both to the requested subject.

Run the front door in `skills/plan/SKILL.md` with the preset below.

## Preset

Stop: chat

| File | Needs | Holds | Block | Why |
|---|---|---|---|---|
| 01-read | none | what the sources at hand say, with paths, and each unresolved point | local-research | the explanation rests on the sources |
| 02-reply | read | the explanation in the form that `references/explain-formats.md` sets | voice-teach | the owner learns the subject |
