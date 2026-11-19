---
status: accepted
date: 2026-11-19
deciders: [user]
related: ["0075", "0134"]
informed-by:
  - 'owner, 2026-11-19: "cool let''s do 1 and 2 then" (code colours with @pierre/highlights, the Changes diff with @pierre/diffs)'
  - 'owner, 2026-11-19: "yeah move it" (tool-card and Markdown diffs on Pierre too)'
---

# 0152. Pierre paints code colours and every diff in the web app

## Context and Problem Statement

The web app showed code blocks without syntax colours. It drew diffs with two of its own renderers: one for the Changes panel and one, on `virtua`, for tool cards and ```` ```diff ```` fences. Neither had syntax colours or changes marked inside a line. Pierre's Apache-2.0 packages give both: `@pierre/highlights` colours code with a WebAssembly tokenizer, and `@pierre/diffs` draws unified and split diffs with line selection, annotations, and a virtual multi-file view.

## Decision Outcome

- `@pierre/highlights` colours each labelled Markdown code block. It loads in its own chunk, and a failure leaves plain code.
- `@pierre/diffs` draws the Changes diff and file view, and every patch in a tool card, a shown diff, or a Markdown fence. `CodeView` keeps a long patch to the rows in view.
- The server still sends the display lines. The client rebuilds a unified patch from them, so comments and Find point at the same lines that Pierre draws. Text that the server marks only with line kinds stays plain coloured text.
- Pierre loads in its own chunk the first time a patch or the Changes panel shows. Until then a patch shows as plain text.
- The page CSP allows `'wasm-unsafe-eval'`, so WebKit compiles the tokenizer. JavaScript `eval` stays blocked.
- Find reaches rows inside Pierre's open shadow roots, and each shadow root gets the page's Find highlight colours.

## Consequences

- Good: code and diffs get syntax colours and changes marked inside a line, from one maintained library.
- Bad: Shiki's per-language grammar chunks make `web/dist`, which the binary embeds, about 10 MB larger.
- Bad: the server's `split` rows are no longer read by the web app.
- Neutral: the main bundle carries no Pierre code.
