# Swift chat building blocks

Date: 2026-10-03. Research only. No repo code changed. "Last commit" is the GitHub `pushed_at` date (`gh api repos/<o>/<r>`).

Adopt Textual (gonzalezreal/textual) first, because it gives native selection across blocks, code blocks and tables as SwiftUI views, with one small dependency. Copy ideas, not code, from LiYanan2004/MarkdownView for streaming, and from macai for a Highlightr code view. Skip the text engines, SwiftTerm, and every agent-chat app, because none has a transcript, tool-card, or composer view that Swarm lacks.

## What Swarm has today

- Platform: `.macOS(.v26)`, Swift tools 6.2, no third-party packages (`ui/Package.swift:2-8`, targets at `:10-21`).
- Markdown: own block parser in SwarmCore (`ui/Sources/SwarmCore/Transcript/TranscriptMessageBlocks.swift:32`), inline text through `AttributedString` markdown (`:236`). Blocks draw as SwiftUI: lists, code, tables (`ui/Sources/Swarm/Transcript/TranscriptMessageView.swift:105`, `:167`, `:222`).
- Selection: one `.textSelection(.enabled)` on the block `VStack` (`TranscriptMessageView.swift:34`). Code has a copy button (`:167-200`). Code has no syntax colors in this file (no highlighter found with `rg`).
- List: `ScrollView` + `LazyVStack` (`ui/Sources/Swarm/Transcript/TranscriptView.swift:133-134`).
- Tool card: output view `TranscriptOutputView` (`TranscriptToolCard.swift:142`), bounded text view (`TranscriptBoundedTextView.swift:16`, selection at `:62`).
- Diff: a `WKWebView` (`DiffWebView`, `TranscriptToolCard.swift:284`) with vendored diff2html and highlight.js (`ui/Sources/Swarm/Resources/DiffViewer/`, `VENDOR.md`, licenses in same folder).
- ANSI: no ANSI handling found in `ui/Sources` (`rg` for escape codes).
- Composer: SwiftUI `TextField(..., axis: .vertical)` (`ui/Sources/Swarm/Composer/ComposerView.swift:172`).
- Swarm license: MIT (`LICENSE:1`). MIT and Apache-2.0 code can be copied with notices. GPL cannot.

## Markdown

| Candidate | License | Last commit | macOS min | View type | Fit for Swarm | Risk |
|---|---|---|---|---|---|---|
| gonzalezreal/textual 0.5.0 | MIT | 2026-06-15 | 15 | SwiftUI `Text` pipeline | Replaces `TranscriptMessageView` blocks, code, tables. Selection across blocks, light/dark, two presets | Pre-1.0 (0.5.0). Pulls swiftui-math and concurrency-extras |
| gonzalezreal/swift-markdown-ui 2.4.1 | MIT | 2025-12-28 | 12 | SwiftUI | Same job, mature | README says maintenance mode, new work is in Textual. Pulls NetworkImage, swift-cmark |
| LiYanan2004/MarkdownView 3.0.0 | MIT | 2026-09-29 | 13 | SwiftUI, plus `MarkdownText` (RichText) for selection | Has `StreamingMarkdownReader` with incremental parse; dark theme names; "continuous selection" | Pulls swift-markdown, Highlightr, SwiftMath, RichText. Two render paths |
| apple/swift-markdown 0.9.0 | Apache-2.0 | 2026-10-03 | not set in Package.swift (tools 6.2) | none, parser only | Could replace the hand parser in SwarmCore | Needs own renderer. Pulls swift-cmark (gfm branch) |

- Textual: README says "Native text selection", syntax highlighting, tables, and that code blocks keep their own selection context, so selecting in a code block clears document selection. Streaming is not documented in the README (not verified).
- Selection across a long LazyVStack of many messages is not shown by any README. Test it in a scratch harness before adopting.

## Code highlight

| Candidate | License | Last commit | macOS min | View type | Fit for Swarm | Risk |
|---|---|---|---|---|---|---|
| appstefan/HighlightSwift v1.1.0 | MIT | 2024-11-27 | 13 | `AttributedString` + SwiftUI `CodeText` | Colors `CodeBlockView` text; theme follows dark mode | Stale (2024). highlight.js through JavaScriptCore |
| JohnSundell/Splash 0.16.0 | MIT | 2024-05-27 | not set | `NSAttributedString` output | Swift code only | Swift-only grammar. Stale |
| ChimeHQ/Neon 0.6.0 | BSD-3-Clause | 2026-08-27 | 10.15 | none, a highlight-state engine for a text view | Not needed for read-only code | Needs tree-sitter packages and a text view that Swarm lacks |
| raspu/Highlightr 2.3.0 | MIT | 2026-02-13 | 10.11 | `NSAttributedString` | 185 languages, 89 themes. Used by macai and MarkdownView | JavaScriptCore start cost. Not SwiftUI |

## Text engine

| Candidate | License | Last commit | macOS min | View type | Fit for Swarm | Risk |
|---|---|---|---|---|---|---|
| krzyzanowskim/STTextView 2.4.1 | GPLv3 or paid commercial (`LICENSE.md`) | 2026-09-26 | 14 | AppKit (SwiftUI wrapper exists) | Could replace the composer or bounded text | GPLv3 conflicts with MIT unless Swarm buys the commercial license. Skip |
| simonbs/Runestone 0.5.2 | MIT | 2026-03-25 | none, `.iOS(.v14)` only | UIKit | None | No macOS support. Drop |
| CodeEditApp/CodeEditSourceEditor 0.15.2 | MIT | 2026-04-20 | 13 | AppKit editor | A full code editor, more than Swarm needs | Heavy dependency tree (tree-sitter, custom-dump). Skip |

## ANSI

| Candidate | License | Last commit | macOS min | View type | Fit for Swarm | Risk |
|---|---|---|---|---|---|---|
| migueldeicaza/SwiftTerm v1.19.0 | MIT | 2026-10-02 | 11 | AppKit terminal view | A terminal emulator, not an `AttributedString` converter | Wrong tool for static tool output. Adds a large package |
| Small ANSI-to-AttributedString package | none found | n/a | n/a | n/a | n/a | `gh search repos` for "ansi attributedstring swift", "ansi swift", "ansi parser swift" returned no usable package |

Swarm has no ANSI code today, so a short own SGR parser (about 60 lines) in SwarmCore is the realistic path.

## Diff

| Candidate | License | Last commit | macOS min | View type | Fit for Swarm | Risk |
|---|---|---|---|---|---|---|
| afitzgerald/DiffKit | MIT | 2026-10-02 | 14 | SwiftUI (`PatchView.swift`, `DiffParser.swift`) | Native replacement for the WKWebView diff | Created 2026-09-27, 0 stars, not reviewed. Too new |
| jamesrochabrun/PierreDiffsSwift | MIT | 2026-08-09 | 14 | SwiftUI wrapping `WKWebView` | None | Same web view approach Swarm has |

Swarm's diff2html web view already works. Keep it.

## Agent-chat apps

| Candidate | License | Last commit | macOS min | Views | Fit for Swarm | Risk |
|---|---|---|---|---|---|---|
| jamesrochabrun/AgentHub | MIT | 2026-08-19 | 14 | `UI/MarkdownView.swift` (MarkdownUI wrapper), `UI/GitDiffView.swift`. No transcript row, tool card, or composer view found by file name | Nothing to copy | Terminal panes, not a chat transcript. Uses a SwiftTerm fork |
| jamesrochabrun/ClaudeCodeSDK | MIT | 2025-12-27 | not checked | SDK, no UI | None | No UI |
| amantus-ai/vibetunnel | MIT | 2026-08-05 | 14 (`mac/Package.swift`) | Session list and terminal (`mac/VibeTunnel/Presentation/Views/SessionDetailView.swift`) | None | No chat rows |
| Renset/macai | Apache-2.0 | 2026-08-16 | not checked | `UI/Chat/BubbleView/MessageContentView.swift`, `CodeView/CodeView.swift`, `BottomContainer/MessageInputView.swift`, `BubbleView/TableView.swift` | Code view over Highlightr is a pattern to read | Plain chat bubbles, no tool cards. Uses AttributedText, Highlightr, SwiftMath |
| AugustDev/enchanted | Apache-2.0 | 2026-07-07 | not checked | `Chat/Components/ChatMessages/ChatMessageView.swift`, `CodeBlockView.swift`, `MessageListVIew.swift`, `macOS/Chat/Components/InputFields_macOS.swift` | None | MarkdownUI plus Splash. No tool cards |

## Recommendation

1. Try Textual in a scratch target first. Measure selection across 200 messages, streaming of a growing string, and parse time on a 100 KB reply. If it passes, replace `TranscriptMessageBlocks` rendering and keep the SwarmCore parser only for search and tests.
2. For code color, use Highlightr (proven in macai and MarkdownView) only if Textual's built-in highlighting is not enough.
3. Write a small own ANSI-to-`AttributedString` function for `TranscriptOutputView`. Keep diff2html.

## Gaps

- Textual's streaming behavior and its highlighter dependency were not read in source.
- Selection across a long list was not tested for any candidate.
- macOS minimum for macai, enchanted, ClaudeCodeSDK, Splash, and apple/swift-markdown was not found in a Package.swift read.
- "Last commit" is `pushed_at`, which can include non-default branches.

## Sources

- Swarm: `ui/Package.swift`, `LICENSE`, files named above (repo `chat-ui`).
- Package.swift, README, LICENSE read from `https://raw.githubusercontent.com/<owner>/<repo>/HEAD/<path>` for: gonzalezreal/textual, gonzalezreal/swift-markdown-ui, gonzalezreal/swiftui-math, LiYanan2004/MarkdownView, apple/swift-markdown, appstefan/HighlightSwift, JohnSundell/Splash, ChimeHQ/Neon, raspu/Highlightr, krzyzanowskim/STTextView (`LICENSE.md`), simonbs/Runestone, CodeEditApp/CodeEditSourceEditor, migueldeicaza/SwiftTerm, afitzgerald/DiffKit, jamesrochabrun/PierreDiffsSwift, jamesrochabrun/AgentHub (`app/modules/AgentHubCore/Package.swift`), amantus-ai/vibetunnel (`mac/Package.swift`).
- License, `pushed_at`, stars: `gh api repos/<owner>/<repo>` on 2026-10-03. Tags: `gh api repos/<owner>/<repo>/releases/latest` or `/tags`.
- File lists: `gh api repos/<owner>/<repo>/git/trees/HEAD?recursive=1` for AgentHub, vibetunnel, macai, enchanted. Dependencies of macai and enchanted from `<name>.xcodeproj/project.pbxproj` on raw.githubusercontent.com.
- Searches: `gh search repos` queries "ansi attributedstring swift", "ansi swift", "ansi parser swift", "diff swiftui"; `gh search code "unified diff" --language Swift`.
