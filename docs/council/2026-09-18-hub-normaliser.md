# Council: one hub normaliser instead of a card per agent feature (2026-09-18)

Artifact: branch `ui-bloom`, `ui/Sources/SwarmCore/Transcript/`, `ui/Sources/SwarmCore/Agent/`,
`ui/Sources/Swarm/Views/Transcript/`, `docs/bus-contract.md`, `docs/decisions/`, and the prior
council log `docs/council/2026-09-17-swarm-ui-approach.md`.

Question: should the UI stop hand-building a SwiftUI card per agent feature and adopt a hub
normaliser, one shared document that every CLI decodes into, with the SwiftUI layer mapping only
that document plus an opaque fallback for unknown types?

Owner's words: "We cannot go around and pick up every single element and build a parity for the
same. There has to be solutions out there that already do this for us. They should not be this
complex. We just do a one-to-one mapping of the components."

## Round 0

Folded into round 1 as a QUESTIONS line. All three voices answered NO QUESTIONS.

## Models used

- CLAUDE: route `council.claude`, claude-fable-5-1, xhigh, pane wBC:p10.
- GPT: route `council.gpt`, gpt-6-astra, high, pane wBC:p23.
- GEMINI: route `council.gemini`, gemini-3.8-flash-high, high, pane wBC:p22.

The first GPT seat (`gpt-csl-hub`, wBC:p21) exited without a summary on the Codex directory trust
screen. It was respawned once as `gpt-csl-hub2`, the trust screen was answered, and the seat ran to
completion. The council was not degraded.

## Verdict: GO-WITH-CHANGES (unanimous, converged at round 1)

Consensus reasoning: the hub the owner asks for already exists in this repository and does not need a
port, a sidecar, or a new protocol. `AgentEvent` is the shared document
(`ui/Sources/SwarmCore/Agent/AgentEvent.swift:545-573`), Codex already pours into it in one place
(`ui/Sources/SwarmCore/Agent/Codex/CodexTranslation.swift:5-11`), Grok does the same
(`ui/Sources/SwarmCore/Agent/Grok/GrokTranslation.swift:98`), and the one-to-one component map is one
`switch row.kind` (`ui/Sources/Swarm/Views/Transcript/TranscriptRowView.swift:95-235`). The real
defect is silent dropping, not a missing architecture: unknown lines and blocks vanish at
`SubagentTranscript.swift:255` and `:325-327`, Codex non-message entries vanish at
`InteractiveChatTranscript.swift:141-147`, and a `.system` row that is not init or a wake draws
nothing at `TranscriptRowView.swift:224-229`. That is why `/compact` has no home today.

### Q1 — tmux screen scraping

Not viable as the source of UI structure, unanimous. Panes are launched with
`TERM=xterm-256color COLORTERM=truecolor` (`ui/Sources/SwarmCore/Agent/AgentKind+Interactive.swift:21`),
so the pane is a painted character grid with no message boundaries, tool ids, or JSON payloads.
Claude Code repaints through Ink many times a second, so a capture catches torn frames.
`tmux capture-pane` cannot reach history in alternate-screen mode
([tmux manual](https://man.openbsd.org/tmux.1#capture-pane)). Permission and `AskUserQuestion` never
reach the screen at all, because they arrive out of band on the hook
(`AgentKind+Interactive.swift:51-63, 78-84`). Agent Code, the one prior art that reads screen state,
still pairs it with JSONL transcripts. Keep the pane as the embedded view and for direct input, which
constraint 2 already grants.

### Q2 — architecture

Keep the native Swift hub. Unanimous on all four options.

- **Swift hub (recommended).** Cost is near zero, because `AgentEvent` and `TranscriptRow`
  (`ui/Sources/Swarm/State/TranscriptModel.swift:10-43`) exist. Failure mode: a CLI format change
  needs a Swift patch, which the opaque fallback turns from a vanished row into a visible raw row.
- **Node sidecar over `agent-transcript-parser`.** Rejected 3-0. Cost is a Node 20.19+ runtime inside
  a native app, IPC, process supervision, and a versioned bridge. The package has 97 commits, 2 stars,
  one maintainer, no CLI or stdio mode, no Gemini decoder, and no hook handling, so permission and
  `AskUserQuestion` still need Swift. "Upstream changes land free" is shared maintenance, not free
  compatibility.
- **AG-UI vocabulary.** Rejected 3-0 as a base. Its ~16 events are built for cloud HTTP and SSE
  runtimes and have no approval event ([events](https://docs.ag-ui.com/concepts/events)), so
  permission, `/compact`, and subagent nesting (`parent_tool_use_id`,
  `SubagentTranscript.swift:178-180`) all fall into `Custom` or `Raw`. Borrow only the `Raw` idea.
- **Borrow ACP names.** Unanimous. The Agent Client Protocol defines `diff {path, oldText, newText}`,
  four permission option kinds, tool kinds, and structured elicitation
  ([tool calls](https://agentclientprotocol.com/protocol/tool-calls)). The repo already decodes ACP
  for Grok (`ui/Sources/SwarmCore/Agent/Grok/GrokEvent.swift:3, :73-79, :207`). Take the field shapes
  when a new field is needed. Do not adopt ACP as a transport, because it runs the agent in stdio mode
  and that conflicts with visible panes. Do not turn this into a rename project.

### Q3 — minimum row vocabulary

The three lists were 10, 7 and 8 types and agree on the substance. The union, mapped onto
`MessageKind` (`ui/Sources/SwarmCore/Model/Models.swift:559-583`):

1. `userTurn` — prompt text, attachments, review chips.
2. `assistantProse` — rendered markdown.
3. `thinking` — collapsible reasoning trace.
4. `toolCall` — input paired with its result, status, duration, and blocks of text, code, diff,
   attachment or unknown. One generic card; tool-specific presenters stay optional decoration.
5. `interactionAsk` — permission request and `AskUserQuestion` in one type, with options and a
   pending, resolved or expired state.
6. `localCommand` — name, args and stdout, collapsed by default, which is where `/compact` lands.
7. `systemNotice` — session start and end, compact boundary, rate limit, error and retry.
8. `opaque` — raw JSON plus a provider and type label.

Every entry needs a stable source identity and order. Model, usage and session state belong in
document metadata, not in a row.

### Q4 — how an unknown type degrades

No `default:` and no `continue` may drop a row. Keep the raw record before classification. Anything no
decoder claims becomes one `opaque` row with a collapsed one-line header, such as
"claude · system/compact_boundary", that expands to a bounded raw JSON view with a copy action.
Preserve unknown fields inside known rows too. Hiding a row requires a named entry in a deny-list, and
`TranscriptNoise` (`ui/Sources/Swarm/Views/Transcript/TranscriptNoise.swift:13-17`) already owns that.
Never draw opaque data as assistant speech, never execute raw content, and never replay a saved
approval control. Offer the live pane when an action cannot be answered from saved history.

### Q5 — a better base

None found, unanimous.

- [Clarc](https://github.com/ttnear/Clarc), Apache-2.0, Swift 6.2, macOS 15, 365 stars, packages
  `ClarcCore` and `ClarcChatKit`. Claude only, no multi-provider hub. Read it for card design, do not
  adopt it as a base.
- [ACP](https://agentclientprotocol.com), Apache-2.0, JSON-RPC, SDKs in Rust, TypeScript, Python,
  Kotlin and Java, plus the third-party native [swift-acp](https://github.com/wiedymi/swift-acp)
  (MIT). Best vocabulary, wrong transport for visible panes.
- [agent-transcript-parser](https://github.com/Juliusolsson05/agent-transcript-parser), MIT, Node.
  Good hub idea and a good opaque-data rule, no Swift bindings, no hook support.
- [agent-code](https://github.com/Juliusolsson05/agent-code), MIT, Electron and React. Out of scope
  under constraint 1.

## Required changes

1. Stop the silent drops. Decode an unrecognised line or block into an opaque event instead of
   returning `nil` or `[]` at `SubagentTranscript.swift:255` and `:325-327`, and instead of
   `else { continue }` at `InteractiveChatTranscript.swift:141-147`.
2. Add the final `else` to the `.system` branch at `TranscriptRowView.swift:224-229`, drawn with the
   existing `ExpandableRow` and `DetailCodeBlock`.
3. Move the `chairSystemPrefixes` list (`SubagentTranscript.swift:152-160`) from hide to a collapsed
   `localCommand` row, which gives `/compact` its home with no new card.
4. Gate noise through a named deny-list in `TranscriptNoise`, and fold a run of opaque rows into one
   line, because Codex emits over sixty background notifications per turn
   (`CodexTranslation.swift:23-28`).
5. Keep live asks separate from saved history. A permission card is valid only while its hook marker
   exists (`AgentKind+Interactive.swift:259`), so a shared display document must carry that rule.

## Next step put to the owner

One session, "nothing vanishes". Make the three silent drops produce opaque rows, add the generic
collapsed row to the view, move the prefix list from hide to `localCommand`, and add one test where a
line of `"type":"future_thing"` gives exactly one row and a hook line gives none. Pass when `/compact`
shows as one collapsed line and a second local command needs no new card. Do not migrate approvals in
this trial.

## Dissent / residual risk

No dissent on the verdict. Residual risks, all raised by more than one seat: the opaque fallback can
flood the chat without the deny-list and the collapsed row; a shared display format is not a promise
of automatic support, because an unknown *reply contract* still needs a Swift adapter or terminal
input; and borrowing ACP names can drift into a rename project. CLAUDE flagged one unverified item,
which is which log line carries the bare `/compact` message, so a real log must be checked first. No
seat ran a build, so runtime cost and performance stay unverified.

## Per-model trail

- CLAUDE: GO-WITH-CHANGES. Found the existing hub and the existing `switch`, named the four drop
  sites, proposed ten row types, warned about the sixty-notification flood.
- GPT: GO-WITH-CHANGES. Same architecture, stressed that live actions must stay distinct from saved
  history, priced the sidecar as shared maintenance, proposed seven types with a block sub-vocabulary.
- GEMINI: GO-WITH-CHANGES. Same architecture, added the Ink repaint and `capture-pane` evidence
  against scraping, proposed eight types, named `swift-acp`.

Round wall time: R1 about 9 minutes. Converged at round 1, so no cross-examination was run.

The decision to accept the one-session trial stays with the owner.
