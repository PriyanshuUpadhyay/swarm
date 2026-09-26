# Saved edit fixtures

`claude-edit.jsonl` retains the shape of a saved Claude Code 2.1.281 JSONL
Edit result inspected locally on 2026-09-26. This was read from a project
transcript file, not from `claude -p` stdout. The hunk ranges and line prefixes
are unchanged. Paths, IDs, timestamps, text, and file contents were replaced
with neutral values; unrelated record metadata was removed.

The observed result has `filePath` and `structuredPatch`. Each hunk has
`oldStart`, `oldLines`, `newStart`, `newLines`, and prefixed `lines`. The
message's `tool_result.tool_use_id` identifies the result. The fixture proves
this saved format for this version; it does not establish other providers'
support or live rate-limit and child-agent event availability.

## Codex and AGY verification

`codex-exec-edit.jsonl` retains a saved Swarm session's `custom_tool_call` and
`custom_tool_call_output` shapes, inspected on 2026-09-26. The call is `exec`;
its JavaScript contains `tools.apply_patch`. The original output consists of
two `input_text` blocks ending in `{}`. The script, paths, IDs, and timestamp
were replaced with neutral values; unrelated metadata was removed. This is
an edit request with an opaque result, not an applied patch event.

`agy-edit.jsonl` retains a saved AGY `replace_file_content` call and its next
`GENERIC` result from `transcript.jsonl`, inspected on the same date. Argument
values are strings. The result includes `[diff_block_start]`, a unified hunk,
and `[diff_block_end]`. Step indices, hunk ranges, line prefixes, and the source
warning about partial file context are retained. Paths, text, argument values,
and timestamps were replaced; the result's introductory sentence was shortened.
The provider version was not recorded in these source records.

The Swift process tests run both fixtures through the real Zig executable and
check full input/result preservation, matching tool IDs, and the absence of a
structured diff row. AGY's textual patch is not yet extracted as `tool_diff`.
Some inspected AGY results have `truncated_fields: ["content"]`; their full-log
counterparts are longer. A future extractor must account for this before
claiming a complete patch. These fixtures do not establish all provider versions
or prove that an edit succeeded merely because its wrapper completed.
