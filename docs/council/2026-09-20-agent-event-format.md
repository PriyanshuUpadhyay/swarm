# Council: adopt an existing agent-event format, or keep the hand-written layer (2026-09-20)

Artifact: branch `ui-bloom`, the mapping layer under `ui/Sources/SwarmCore/Transcript/` and
`ui/Sources/SwarmCore/Agent/`, the new `ui/Sources/Swarm/Views/Center/Panes/InteractiveChatView.swift`
and `ui/Sources/Swarm/Design/SmokeChat.swift`, the prior council log
`docs/council/2026-09-18-hub-normaliser.md`, and the research report
`~/.claude/reports/2026-09-18-native-agent-renderer-web.md`.

Question: should Swarm keep hand-writing its own per-provider agent-event mapping, or adopt an
existing normalised format (ACP, AG-UI or another)?

Owner's words: "Get council to opine and understand how unnecessary it is to build our own setup
when there would be already existing solutions that map these values to a particular format so that
it's easier to implement."

The seats were asked to test the 2026-09-18 verdict rather than repeat it, and to say for each of
this week's four chat faults whether an existing schema would have prevented it.

## Round 0

Folded into round 1. No seat asked a question.

## Models used

Routed through `council.claude`, `council.gpt` and `council.gemini`: Claude (claude-fable-5-1,
xhigh, pane wBC:p2F), GPT (gpt-6-astra, high, pane wBC:p2G) and Gemini (gemini-3.8-flash-high,
high, pane wBC:p2H). All three ran to completion. The council was not degraded.

## Verdict: NO-GO (unanimous, converged at round 1)

Consensus reasoning: no existing schema reads what Swarm reads. Codex writes `rollout-*.jsonl` and
Claude writes `projects/*.jsonl`, and ACP specifies a stdio JSON-RPC transport rather than a
transcript file format, so the log readers, the noise lists and the role rules stay hand-written
under any schema. The hub the owner asks for already exists in the repository, because Codex is
translated into Claude Code's stream-json envelope in one place
(`CodexTranslation.swift:278-289`) and one `switch` draws every row
(`TranscriptRowView.swift:95-243`). Adopting ACP as the stored vocabulary costs four to six
sessions, needs a migration of every stored `messages` payload, and buys field names.

**The four faults, scored 0 of 4.** All three seats agree that an existing schema would have
prevented none of them.

1. Bare Codex type names in the chat. A hand-kept skip list is needed under any schema
   (`InteractiveChatTranscript.swift:154-162, 228-232`), and an ACP client hand-lists too
   (`GrokEvent.swift:86-88`).
2. The refused send. A pane address is not an event
   (`InteractiveChatView.swift:12-13`).
3. The restart that made a new conversation. Resume is a launch flag and an identity rule
   (`AgentKind+Interactive.swift:64, 71`).
4. The Codex update question. A TUI question asked before any log or hook exists is invisible to
   every schema (`AgentKind+Interactive.swift:72-74`).

CLAUDE adds that the 2026-09-18 verdict stands on architecture but answered a narrower question
than the one that hurt, because three of the four faults sit in the process-lifecycle layer that no
event schema touches.

## Required changes

1. [major] `InteractiveChatView.swift:180-184` give "Starting the chat" a deadline. `SmokeChat.swift:159-174`
   already treats a `SessionStart` hook write after the launch as proof of life; move that rule into
   the pane, and past the deadline say "The CLI has not reported in" and offer the terminal toggle
   (`InteractiveChatView.swift:132-134`). It covers the whole class of fault 4, which is the trust
   screen, a login, and the next update question.
2. [major] `InteractiveChatTranscript.swift:154` make the Codex deny-list explicit by name
   (`event_msg`, `session_meta`, `turn_context`, `world_state`, `token_usage_record`), so a record
   nobody has coded for degrades to an opaque row instead of an implicit drop.
3. [minor] Keep borrowing ACP field shapes when a new field is needed, and do not start a rename
   project. Do not adopt ACP as a transport, because it would remove the visible pane.
4. [minor] `SmokeChat.swift:99` assert that the resumed provider session id equals the one before
   the restart, so a silent new conversation fails the check.

## Dissent and residual risk

No dissent on the verdict. Residual risks, each named by more than one seat: a record Codex adds
next month can vanish while the deny-list is implicit; an on-disk format change breaks ingestion
silently; and the lifecycle layer's only end-to-end check is run by hand
(`SmokeChat.swift:7-17`), so the next fault of that class reaches the owner the same way.

Nothing outside changed since 2026-09-18. The newest ACP releases are dated 18 September and add a
session-notice capability, not a transcript file format and not a Swift package
(https://github.com/agentclientprotocol/agent-client-protocol/releases).

## What stays hand-written, whichever way this goes

Launch flags and hook injection per CLI (`AgentKind+Interactive.swift:33-99`), the permission hook
bridge and its marker files (`:180-182, 233-290`), busy-screen patterns (`:128-145`), log path
discovery (`InteractiveChatTranscript.swift:297-336`), per-provider noise lists
(`SubagentTranscript.swift:200-219`), tool naming (`CodexTranslation.swift:62-74`), resume identity
rules, and the SwiftUI rows.

## What would change the answer

A Swift package with more than one maintainer that reads both `projects/*.jsonl` and
`rollout-*.jsonl` into one shape with a curated noise list; a versioned transcript file schema
published by Anthropic or OpenAI; or a constraint change that lets the agent run under ACP stdio
with the pane as a mirror. Under that last one, faults 3 and 4 move into the protocol and the
answer flips to GO.

## Per-model trail

- CLAUDE: NO-GO. Scored the four faults 0 of 4, priced the ACP migration with the stored-payload
  migration, and proposed the liveness deadline as the first step.
- GPT: NO-GO. Held that shared field rules cannot read logs, and asked for the resume assertion in
  the smoke check.
- GEMINI: NO-GO. Showed that ACP is stdio JSON-RPC rather than a disk format, and proposed the
  named Codex deny-list.

Round wall time: R1 about 12 minutes, 15:37 to 15:49. Converged at round 1, so no cross-examination
was run.

The decision stays with the owner.
