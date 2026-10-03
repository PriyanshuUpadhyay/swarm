# Council: chat UI direction

Date: 2026-10-03. Artifact: the eight-item chat UI proposal for flow `chat-ui`
(`tmp/flow/2026-10-03-chat-ui/01-frame.md`), with `docs/research/2026-10-03-claude-injected-records.md`,
`docs/research/2026-10-03-swift-chat-building-blocks.md`, and two web search merges.

Models: CLAUDE opus xhigh, GPT gpt-6.1-sol xhigh, GEMINI gemini-3.8-flash-high. No fallback.

Round 0: all three answered NO QUESTIONS (asked in the same message as round 1).

Converged at round 2. Wall time: round 1 about 12 minutes, round 2 about 3 minutes.

## Verdict: GO-WITH-CHANGES (unanimous)

The row fixes are needed now. The frame asks for compact rows with full detail on demand, so parser
tags, row-builder pairing, and small native view changes deliver it. Each disputed extra (a color
engine, an automatic fold, a ring with no capacity, a markdown package) changes row height or adds a
dependency that the frame does not ask for.

## Required changes

1. Put `parentUuid` and `sourceToolUseID` on event metadata (Zig `Meta` at `root.zig:5-9`, Swift
   `TranscriptEvent.swift`, decoded if present), so no event case changes. Pair rows in the row
   builder's join pass (`TranscriptRowBuilder.swift:71-95`). An output whose input is outside the
   loaded window stays a plain output row. Scope links by session and conversation.
2. `[Request interrupted by user…]` becomes a notice that ends the turn. User text whose
   `origin.kind` is present and not `human` becomes System. Add `<cross-session-message>`. A typed
   block next to a `<system-reminder>` block stays "You". Test `<b>` prompts, lookalike tags, mixed
   blocks, duplicate IDs, and parents outside a loaded page.
3. A command chip with a linked skill body sets `startsTurn`; `/model` and `/clear` do not. These
   rows stay `Kind.system` and carry the parser kind, because `ConversationBoundary.swift:31-35`
   drops the `/clear` preamble only for system rows.
4. Shell rows decode only `&amp; &lt; &gt;`, once, never inside `<persisted-output>`. Read
   `<bash-exit-code>` when present; with none, show a neutral glyph, because stderr is not failure.
   Output folds at 50 lines (`codex-rs/tui/src/exec_cell/render.rs:44`) through a line parameter on
   `TranscriptTextPreview`. v1 strips CSI, OSC, and C0 controls except `\n` and `\t`, also in
   `/compact` output. Color comes later.
5. Tool headers are one line: description first, else a middle-truncated first command line. The
   open card shows the full command above the output. Agent tool output folds at 5 lines. All five
   tool states stay visible.
6. The fold of 3+ finished tools is built only if the owner picks it in `02-design.md`. Then it
   folds only successful tools in ended turns, keeps child IDs, opens on a find hit, and never folds
   a row already on screen.
7. The composer meter gets a typed count and an optional capacity, not the formatted `usageLabel`.
   A native `Gauge` shows only with capacity (Codex); Claude shows a locale-aware compact count such
   as `236K`. Keep the usage-details action.
8. Textual is out of this flow. A separate spike tests it against the present renderer (200
   messages, a growing 100 KB reply, selection, paging, row height, VoiceOver, latency) and, if it
   passes, gets its own ADR as Swarm's first package.

## Dissent and residual risk

None open. Residual risk is that moving rows out of `.user` hides a typed message or makes a turn read
idle, because `ChairTurn` and find key on row kind and id.

## Repos

- Copy from: HizTam/codex-history-viewer (MIT, `src/chat/claudeTerminalOutput.ts`),
  daaain/claude-code-log (MIT, `claude_code_log/factories/user_factory.py`), matt1398/claude-devtools
  (MIT, `src/renderer/utils/toolRendering/toolSummaryHelpers.ts`), openai/codex (Apache-2.0,
  `codex-rs/tui/src/exec_cell/render.rs`, `model.rs`).
- Learn from: manaflow-ai/cmux (GPL, rules only), delexw/claude-code-trace (MIT),
  rderaison/bromure (MIT, `ClaudeTranscriptView.swift`), ttnear/Clarc (Apache-2.0), charmbracelet/crush
  (rules only), safzanpirani/ruddr (MIT), Lore-Hex/QuillCode (Apache-2.0, for color later),
  GetStream/stream-chat-swift-ai (Apache-2.0).
- Skip: gonzalezreal/textual in this flow, AttilaTheFun/agent_ui `ContextRing` (native `Gauge`),
  TopScrech/ANSI (needs ScrechKit, fixed palette), afitzgerald/DiffKit, SwiftTerm, STTextView (GPL).

## Per-model trail

- GEMINI: r1 GO-WITH-CHANGES (spike Textual, pair in Swift, short header, own ANSI converter); r2
  conceded to CLAUDE on all five.
- GPT: r1 GO-WITH-CHANGES (IDs through events, no invented exit glyph, keep tool states, full command
  always shown, Textual trial); r2 retracted the always-shown command and conceded the rest.
- CLAUDE: r1 GO-WITH-CHANGES (found the turn bug from `.user` shell and interrupt rows, strip ANSI,
  fold at 50, Gauge only with capacity, Textual out); r2 widened the strip to OSC and C0.
