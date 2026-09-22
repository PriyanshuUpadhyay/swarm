# Council: price the transcript normaliser options (2026-09-18)

Artifact: branch `ui-bloom`, the transcript readers and views named below, the earlier verdict in
`docs/council/2026-09-18-hub-normaliser.md`, and the web search report at
`~/.claude/reports/2026-09-18-native-agent-renderer-web.md`.

Question: price five options in working days after the owner pushed back on the earlier verdict.

Owner's words: "If we depend on an external library that is maintained by loads of people, we would
not have to worry about the future updates that will come in and we would be able to update the
dependency and just and it would just work. Right now, building our own translation setup would cost
us management and time."

## Round 0

Folded into round 1 as a QUESTIONS line. All three voices answered NO QUESTIONS.

## Models used

- CLAUDE: route `council.claude`, claude-fable-5-1, xhigh, pane wBC:p27.
- GPT: route `council.gpt`, gpt-6-astra, high, pane wBC:p28.
- GEMINI: route `council.gemini`, gemini-3.8-flash-high, high, pane wBC:p29.

## Evidence pinned to every seat

Gathered by the chair on 2026-09-18, before the seats started.

- **E1.** Across 446 real Claude Code transcripts on this machine there are 22 distinct top-level
  record types. The Swarm reader handles two, `user` and `assistant`. Two types,
  `file-history-delta` and `pr-link`, first appear in 2026-09.
- **E2.** ACP v1 `schema.json` is 247,168 bytes with 170 definitions. `schema-v1.22.0` and
  `schema-v2.0.0-alpha.4` both shipped 2026-09-17. The repository has 4,275 stars, 389 forks and over
  100 contributors, Apache-2.0.
- **E3.** `quicktype --src-lang schema --lang swift --just-types` on the raw ACP schema stops at the
  JSON-RPC envelope and emits 96 lines. A wrapper referencing all 170 definitions emits 3,972 Swift
  lines, 142 structs and 23 enums, with 143 "cannot infer this type" warnings that each become `Any?`.
  The output was never compiled.
- **E4.** The author of `rebornix/acp-swift-sdk` dropped schema codegen and hand-wrote the types,
  because of polymorphic unions and JSON-RPC friction.
- **E5.** Verified against the GitHub API. `rebornix/Agmente` 543 stars, `wiedymi/swift-acp` 32,
  `paduh/ag-ui-swift` 20, `rebornix/acp-swift-sdk` 7, `agent-transcript-parser` 2 with no licence file.
  Every one has a single maintainer, and Agmente is a finished app rather than a package.
- **E6.** `agentic.nvim` drives four agents through one ACP UI with no per-provider code, and its users
  report optional fields, wrong event order and freezes. `CodeCompanion.nvim` still hand-writes its
  presentation layer.
- **E7.** No vendor-neutral `PermissionRequest` spec exists outside ACP's own extension.
- **E8.** Anthropic documents that raw session files change across releases, and Codex issue
  openai/codex#36804 shows unescaped tool stdout polluting the stream.

## Verdict: take option A (unanimous on the recommendation, converged at round 1)

Labels were GO from GEMINI and GO-WITH-CHANGES from CLAUDE and GPT, all three on the same
recommendation. No cross-examination was run, because the primary setups agree and the only
difference is the second choice.

## Price table

Planning estimates for one experienced Swift developer, not measured delivery. The three seats'
figures are given in order CLAUDE / GPT / GEMINI.

| Option | Days | Upkeep per month | Verdict |
|---|---|---|---|
| **A. Nothing vanishes** | 2 / 3 / 1.5 | 0.25-0.75 day | **Recommended, unanimous** |
| B. Generate Swift from the ACP schema | 12 / 12 / 12 | 1-2.5 days | Rejected as a first step, unanimous |
| C. Take a Swift ACP package | 4 / 7 / 8 | 0.5-2 days | Second choice for CLAUDE and GPT |
| D. Node sidecar over `agent-transcript-parser` | 10 / 14 / 14 | 1.5-3 days | Rejected, unanimous |
| E. Run agents through an ACP bridge | 20 / 18 / disqualified | 1 day | Breaks constraint 2, unanimous |

Option B lands on exactly 12 days from all three seats independently.

## Is the owner's premise true?

No, unanimously, for A, B, C and D. It holds only for E, which the visible-pane constraint forbids.

1. "Maintained by loads of people" describes the ACP *specification*, with over 100 contributors
   (E2). It describes no Swift library, because every one has a single maintainer (E5).
2. "Update the dependency and it just works" fails, because the thing that changes is Claude's own
   session file (E1, E8). The only listed dependency that reads that file has 2 stars and one
   maintainer (E5).
3. Codegen does not just work. It leaves 143 `Any?` fields (E3), and those are not `Sendable` while
   `AgentEvent` is (`AgentEvent.swift:545`, `:7-10`). The one Swift SDK author who tried it gave up
   (E4).
4. A working ACP client still hand-writes its presentation layer (E6), so the UI cost stays whatever
   the transport.
5. The owner is right on one point. A card per record type does cost real time at two new types a
   month (E1). Option A removes that cost, because a new type becomes visible with no code at all.

## What each option does when `pr-link` appears

- **A.** Shows as one collapsed row, "claude · pr-link", with no code change. A deny-list entry or a
  card is optional, about 0.25 day.
- **B.** Nothing. `pr-link` is a session-file record, not an ACP definition, so it still drops at
  `SubagentTranscript.swift:255`.
- **C.** Nothing, for the same reason.
- **D.** Waits for one maintainer, and a raw pass-through still needs A's row in Swift.
- **E.** The app stops reading session files, so the record appears only if the bridge maps it.

## Required changes

1. Before writing the opaque row, count lines per record type across the 446 transcripts, about one
   hour, and build the deny-list from that count.
2. Keep denied records out of the row cap. The reader's own note says `attachment` was half the lines
   in one capture (`SubagentTranscript.swift:88-90`), and the 500-row cap counts every appended row
   (`:84`, `:146-148`), so noise can push real turns out of view.
3. Fold a run of opaque rows into one line, because Codex emits over sixty background notifications
   per turn (`CodexTranslation.swift:23-28`).
4. Say plainly that "nothing vanishes" preserves unknown data and does not infer its meaning. It
   cannot safely replay a permission request (E7).
5. If C is ever taken, limit it to the `ACPModel` types and pin the version exactly
   (`ui/Package.swift:15`). `swift-acp` is a client that starts the agent on stdio with
   `client.launch(agentPath:)`, which constraint 2 forbids, and its last push predates schema v1.22.0.

## The five edit sites for option A

`SubagentTranscript.swift:255`, `:325-327`, `:152-160`, `InteractiveChatTranscript.swift:141-147`,
and `TranscriptRowView.swift:224-229`. No new event type is needed, because `.unknown` is already a
stored `.system` row (`AgentEvent.swift:573`, `:642-644`).

## Dissent / residual risk

The fallback splits. CLAUDE and GPT rank C second, limited to protocol types and reversible. GEMINI
ranks B second, to keep everything in-repo and avoid a single-author package. Nobody defends D.

All three name the same top risk, which is the opaque row flooding the chat before the deny-list
exists. All three also name the same likely error, which is E1's rate. Two first sightings in one
month on one machine do not prove two new types a month, and `pr-link` appears only when a session
opens a pull request, so a first sighting can be first use rather than first release. The rate sets
every upkeep figure above, but it does not change the ranking, because only A makes a new type free.

No seat ran a build, and the 3,972 generated lines in E3 were never compiled, so B's real cost could
be higher or lower than 12 days.

## Per-model trail

- CLAUDE: GO-WITH-CHANGES, A at 2 days. Found the noise hazard with a number, checked `swift-acp`'s
  tag against its README, and noted that `Any?` breaks `Sendable`.
- GPT: GO-WITH-CHANGES, A at 3 days. Warned that "nothing vanishes" must not be read as feature
  support, and priced B to D including A-equivalent work so the options compare.
- GEMINI: GO, A at 1.5 days. Ranked B second to keep the work in-repo, and disqualified E outright.

Round wall time: R1 about 10 minutes. Converged at round 1.

The decision to take option A stays with the owner.
