# Council: agent blockers in the chat (2026-09-17)

Question (owner): blockers an agent CLI raises (trust, permission, plan approval, questions, login,
usage limits, updates, crashes) must reach the owner in the chat view so they can be answered there;
each CLI handles them differently; reuse existing solutions, build only what is missing.

Proposal P judged: detect a waiting state from each CLI's own structured signal first, fall back to the
pane screen, show a card in the chat, answer by keys through swarm, and pre-empt startup blockers.

Round 0 folded into round 1; all voices answered NO QUESTIONS.

Models: CLAUDE route `council.claude` (claude-fable-5-1 xhigh; the pane showed Opus 5 earlier today),
GPT gpt-6-astra high, GEMINI gemini-3.8-flash-high. Rounds ran 22:30 to 22:42.

## Verdict: GO-WITH-CHANGES (unanimous, converged at round 3)

Consensus reasoning: Claude Code and Codex already expose a `PermissionRequest` hook that runs before
the CLI draws its own prompt and can return the decision, and the app already installs it
(`ui/Sources/SwarmCore/Agent/AgentKind+Interactive.swift:41-76`) but throws the payload away. Answering
through that hook avoids menu-order key guessing. Screen reading is needed only where no hook exists, and
Herdr's maintained per-CLI rules can classify a captured tmux screen with
`herdr agent explain --file <capture> --agent <label> --json`, with no Herdr daemon or pane.

Required changes:
1. Pre-empt folder trust, update and onboarding screens in each CLI's config before launch; never a card.
2. Claude Code and Codex permissions: the hook keeps its payload, waits up to about 120 s for an answer
   file keyed by a per-invocation token (and `tool_use_id` when present), and returns allow, deny, or
   `ask` to hand off to the terminal. Raise the current 3 s hook timeout. On timeout, return no decision
   and let the CLI prompt. Measure the exact decision JSON on each installed CLI before shipping.
3. A raw-key bus verb (bare Enter, arrows, Space, Esc): today `swarm type` rejects empty text and always
   adds Enter (`src/main.rs:591`, `adapters/herdr.conf:5`). Keys answer `agy`, unhooked prompts and
   terminal handoff.
4. One card envelope bound to session, agent, pane, process generation and the token; drop stale or
   duplicate answers; clear a card only after an observed state change; never keep both channels open.
5. Screen classification with `herdr agent explain --file`; when `herdr` is absent, the app's busy check
   raises a generic attention card with no answer buttons.
6. A "Needs attention" alert for pending work on an idle or exited agent; usage limits come from the
   app's quota poll, not a screen rule.

First slice: measure the decision shape, then one Claude Code permission card answered through the hook
file with allow, deny and ask, drawn with the existing permission card. Test Allow, Deny, timeout,
terminal handoff, process replacement and a late click, plus offline classification of a captured Codex
trust screen.

Dissent / residual risk: the decision field names differ between the docs the seats read (Claude
`hookSpecificOutput.decision` shape, Codex `decision.behavior`), so measure first; `agy` has no hooks
in the app (`AgentKind+Interactive.swift:44-78`) and stays keys-only; Codex may not send
`tool_use_id`; screen rules are hints, not proof.

Per-model trail:
- GEMINI: R1 keys answer channel and own screen regexes; R2 kept keys, proposed porting Herdr's engine;
  R3 conceded the hook answer file and `herdr agent explain`.
- GPT: R1 keys with stale-answer binding and a raw-key verb, agy is Antigravity; R2 conceded native hook
  replies, proposed a manifest evaluator; R3 conceded `herdr agent explain`, added invocation tokens.
- CLAUDE: R1 hook as answer channel over HTTP, reuse Herdr manifests, stall rule; R2 dropped HTTP for an
  answer file, conceded the raw-key verb; R3 added the `ask` handoff value and measured `explain` with no
  daemon.

The decision to build the first slice stays with the owner.
