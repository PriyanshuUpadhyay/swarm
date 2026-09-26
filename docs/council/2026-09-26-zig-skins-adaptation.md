# Council on adapting ai-coding-skins ideas to the Zig translator

## Question and evidence

The user asked, “get council to opine on the fact that if our zig translation package can adapt some of the things from here”.

The reference is [ai-coding-skins at 110db918](https://github.com/Glitch-Cat-Club/ai-coding-skins/tree/110db918234d9bd13742e494b083972cfb5f3d0f). Seats inspected its README, STREAM.md, SCENE.md, LICENSE, bridge/events.py, bridge/app.py, bridge/session.py, and skins/lib/stream.js. Local evidence was the current, uncommitted `main` checkout: `packages/transcript`, `ui/Sources/TranscriptTool`, `ui/Sources/SwarmCore/Transcript`, and their tests. Prior Council logs from September 18 and 20 supplied history, not proof of the current implementation.

This was advisory work. The brief preserved the native app, visible agent panes, saved-log input for Claude/Codex/AGY, unknown-event fallback, and the single movable sidebar. ADR 0012 is superseded; the visible-pane constraint came from the brief.

## Verdict

**GO-WITH-CHANGES, agreed by both participating voices. The three-model Council was incomplete.**

Reuse selected designs. First obtain a public or owner-approved, sanitized saved Claude Edit record that proves whether `toolUseResult.structuredPatch` is present. If confirmed, add optional structured diff facts to Zig while retaining the existing tool result's status, content, and identity. Do not adopt the Python bridge or change transport.

## Accepted conditions and ownership

1. **Zig extracts saved facts.** The current event union already covers text, tools, questions, decisions, and usage (`packages/transcript/src/root.zig:104`). Unknown input retains its raw payload (`:250`). This does not mean every known record is lossless: named noise is ignored and image data is reduced.
2. **Structured diffs require source proof.** Upstream extracts hunks and omitted-line counts in [events.py](https://github.com/Glitch-Cat-Club/ai-coding-skins/blob/110db918234d9bd13742e494b083972cfb5f3d0f/bridge/events.py#L414). No inspected Swarm fixture proves `structuredPatch` in saved logs. Existing `toolUseResult` fixtures cover answers (`root.zig:1140`). A new diff must supplement the tool update, preserving status and content (`:494`), and must not start a Git diff engine project.
3. **Swift owns display changes.** Labels, shorter display paths, and bounded previews belong in the presentation layer (`ui/Sources/SwarmCore/Transcript/TranscriptRowBuilder.swift:148`). Preserve source IDs and unknown raw data. State when a preview omits content. An output cap does not bound memory allocated while reading a large input line (`root.zig:609`); an input-size limit needs a separate oversized-record policy.
4. **Keep existing usage semantics.** Zig extracts saved usage; `ChatUsage` replaces cost snapshots and clears stale context (`ui/Sources/SwarmCore/Transcript/ChatUsage.swift:15`). Do not port upstream's process-reset cost accumulator. Rate-limit windows and full provider-child lifecycle remain unverified in saved sources, not proven absent.
5. **Keep state and controls with their owners.** Swift/session code owns cross-event joins and replay application; the live session layer owns permission answers and process actions. Saved approval history must not become an active control. Provider child agents and visible Swarm workers are distinct.
6. **Test new contracts, not duplicate coverage.** Partial-line delivery, adjacent same-ID folding, unknown blocks, and repeated usage already have checks (`root.zig:1306`, `:1058`; `TranscriptRowBuilderTests.swift:23`; `ChatUsageTests.swift:14`). For a verified diff addition, test success, failure, missing patch, tool-ID joins, retained status/content, and explicit preview truncation. Chunk folding is not general duplicate suppression; multiple blocks can share one record UUID. A separate missing integration check could compare ordered batch output against an initial window ending inside a record followed by an append.

## Rounds, disagreement, and model trail

Round 0 produced no questions from GPT or Gemini. Claude could not answer because its weekly account limit was reached. Its route declared no fallback; no paid-credit switch or hidden replacement was used.

Round 1 produced two GO-WITH-CHANGES positions but different first steps. Gemini proposed immediate diff extraction, string caps, and path rewriting. GPT proposed contract tests and deferred new event facts. Round 2 made diffs conditional and moved display changes to Swift. GPT withdrew its broad test list after confirming existing coverage. Round 3 corrected Gemini's unsupported absence claim and duplicate-test proposal. Both accepted the same recommendation; no design disagreement remained.

- GPT ran `gpt-6-astra`, high, via `council.gpt`, pane `wEW:pJ`. It supported conditional diff facts and source proof, and identified data-preservation and duplicate-test issues.
- Gemini ran `gemini-3.8-flash-high`, high, via `council.gemini`, pane `wEW:pK`. It proposed structured diffs and accepted the source-proof, tool-result preservation, and display-only limits after examination.
- Claude launched `claude-fable-5-1`, xhigh, via `council.claude`, pane `wEW:pH`, but returned no opinion because of its weekly limit.

The Herdr session was `01a0dc29-384b-75f2-a42b-5d4c6978b21b`. Source review round times were 189 seconds, 130 seconds, and approximately 87 seconds, followed by a citation correction. Convergence occurred in round 3. Run artifacts were held at `/tmp/councils/swarm-zig-skins-20260926` until this durable record existed.

Automatic approval review first blocked pane closure pending explicit confirmation. The user then approved closing skins-claude, skins-gpt, and skins-gemini. All three close commands completed, and `swarm agents --json` confirmed that each pane was null while the orchestrator pane remained alive. The run scratch directory was then removed after this durable record existed.

## Limits and decision remaining

This was source inspection only. No implementation, build, test run, or private transcript read was performed. The user still decides whether to start the saved-fixture investigation and conditional diff feature. Copied substantial source portions would require the upstream MIT notice. No dependency adoption or feature implementation was approved by this Council run.

## Authorized implementation follow-up

The user later asked to implement the changes. A scoped inspection of this project's saved Claude
Code 2.1.281 transcript confirmed `toolUseResult.structuredPatch`. The sanitized source fixture
and its provenance are in `packages/transcript/src/fixtures/`; no private source text was retained
in the fixture. The temporary unsanitized copy was removed.

Zig now emits `tool_diff` after the original tool update. Failed edits emit no successful diff;
malformed patches or records with ambiguous tool identity retain their result and an unknown
raw record. Swift shows a collapsible Changes row through the existing diff2html viewer. The
display copy is limited to 200 lines and 400 characters per line, with an omission notice. The
event's complete patch remains unchanged. Tool labels use short filenames while their detail
keeps the full input. No Python dependency, live transport, child lifecycle, or rate-limit mapping
was added.

Verification passed: 104 Zig tests; 38 selected Swift tests, including an actual Zig-process
to Swift-decoder to row/preview check; release build; source-boundary lint; and diff whitespace
check. A live isolated app check opened the saved diff, read its line ranges and added text,
expanded the retained original tool result, verified the 205-line patch's five-line omission
notice, and verified that collapsing the preview removed its web view. Preview tests also cover
long Unicode lines, quoted/newline paths, malformed events, and unchanged canonical data.
Applicable engineering guidance A4 and A7 was checked by judgment: unknown event fallback remains
available, and line counts name their unit. Standards tally: pass 0 / fail 0 / judgment 2 / not checked 0.

The signed app was installed at `/Users/priyanshu/Applications/Swarm.app`; all 15 installed file
hashes matched the verified stage. The previous app remains at
`/Users/priyanshu/Applications/Swarm.backup-20260926-141203-1e5103.app`. The user's running app was
not restarted. Evidence logs are `/tmp/swarm-structured-diff-tests.log`,
`/tmp/swarm-structured-diff-build.log`, `/tmp/swarm-structured-diff-ui-proof.txt`, and
`/tmp/swarm-structured-diff-installed.json`. Source checks remain limited to the confirmed Claude
saved Edit format; other providers continue to use their existing tool results.

## Other-provider verification follow-up

The user then asked to verify the other providers. Source inspection was restricted to saved
sessions that refer to this Swarm repository. In 22 recent Codex sessions, edit requests appear
inside `exec` JavaScript, with tool outputs matched by `call_id`. No direct `apply_patch` call or
saved structured patch event was found in this sample. An additional 44 older Swarm sessions in
the standard Codex session directory also used `exec`, not direct patch calls. This does not
establish absence in every Codex host or version. One recent wrapped request had no paired output.

In 40 AGY sessions, 90 `write_to_file` and 138 `replace_file_content` calls had matching `GENERIC`
results. All 138 replacement results contained textual diff blocks. Of the 228 calls, 95 had
`tool_calls` listed in `truncated_fields`; nine replacement results listed `content` there.
Those nine still had both diff markers, and each corresponding `transcript_full.jsonl` result
was longer. Markers alone therefore cannot prove a complete saved patch. No AGY textual patch
extractor was added during this verification task.

The real Zig executable preserved the saved input, matching tool identity, and complete available
result text for 198 paired Codex requests and all 228 AGY edits. It emitted no `tool_diff` for
these records. This verifies existing text handling, not edit success or provider-wide preview
support. Temporary raw samples were removed after creating neutral fixtures.

Two sanitized provider fixtures and a parameterized Swift test now exercise the actual Zig
process, Swift decoding, and transcript row construction. `TranscriptDiffTests` passed all five
tests (six cases), including the existing Claude preview checks. Evidence is in
`/tmp/swarm-provider-verification-tests.log` and `/tmp/swarm-provider-verification-results.json`.
Only fixtures, tests, and documentation changed in this follow-up; the installed app is unchanged.
No new API contract or numeric unit was introduced. Standards tally: pass 0 / fail 0 /
judgment 0 / not checked 0. Codex and AGY structured diff previews remain unimplemented.

## Authorized transcript UI follow-up

The user approved the transcript UI and parallel implementation and testing. Three visible Herdr
workers implemented grouping, message rendering, and independent saved-log fixtures. The root
session inspected and integrated their work, fixed the findings, and ran the final app checks.

Tool calls now share one card with matching result updates and Claude patches. Cards retain the
command, input, output, and file path. Recorded failure opens the card. Finished means that the
tool returned; it does not mean that tests passed. Missing, ambiguous, and reused identities keep
separate source rows. Messages use native headings, lists, code, and simple tables. Complex table
pipes retain source text. Only HTTP(S) message links are active, and remote images are not loaded.

Long output has an explicit preview limit and a full-output action. Copy keeps the complete source.
Search opens matching cards and full patches, including text beyond the preview limit. Diff display
reuses the existing offline diff2html library. AGY logs can now be read when their path is known;
automatic AGY discovery and title extraction remain outside this change.

Final checks passed: 219 Swift tests in 32 suites, 104 Zig tests, release build, source-boundary
lint, and diff whitespace check. Seven independent fixture files passed the real Zig parser,
including tail replay. The isolated app exercised the actual release executable and Zig reader
with synthetic saved logs and a test bus. GUI checks passed for message/code rendering, copy,
unsafe links, failed and interrupted results, all three providers' appended result updates,
long-output search/copy, full-patch search/copy, unified/split diffs, raw events, draft retention
across workspace changes and restart, Changes-to-Workspaces return, scroll position, Jump to latest,
and resumed following at the end. A compact 980 by 800 window retained cards and composer controls.
Reveal file selected the exact test file in Finder.

The final signed app was installed at `/Users/priyanshu/Applications/Swarm.app`. All 15 installed
file hashes matched the release stage. Both executable files matched the GUI-tested app after
removing bundle-specific signatures from comparison copies. The previous app remains at
`/Users/priyanshu/Applications/Swarm.backup-20260926-153526-transcript-ui.app`. The owned test app
exited normally; the user's personal app was not restarted. Evidence is under
`/tmp/swarm-transcript-ui-run/`, including `tests-final.log`, `qa-evidence.json`,
`final-compact.png`, and `installed.json`.

No fresh provider request was sent in these GUI tests. New provider formats, live approval controls,
rate limits, child lifecycle, and Codex/AGY structured patch extraction are not verified by this run.
The user approved closing ui-data-0926, ui-markdown-0926, and ui-qa-0926. All three close
commands completed, and `swarm agents --json` confirmed null panes for those workers while the
orchestrator remained alive.
