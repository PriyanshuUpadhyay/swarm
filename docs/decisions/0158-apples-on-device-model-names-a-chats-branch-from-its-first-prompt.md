---
status: accepted
date: 2026-11-23
deciders: [user]
amends: ["0146", "0149"]
related: ["0134"]
informed-by:
  - 'owner, 2026-11-23: "Can we use Apple Foundation model to provide an appropriate title as well as a good branch name for it?"'
  - 'owner, 2026-11-23: "If we are getting the title from the harness itself, we don''t have to create it using AFM. [...] We can do the 1A2A.", then "Wait for the model there, up to 3 s"'
---

# 0158. Apple's on-device model names a chat's branch from its first prompt

## Context and Problem Statement

ADR 0146 and ADR 0149 named a chat's branch from the first five words of its first prompt that
are not filler. A long or chatty prompt gave a poor name: "In Swarm right now we are creating
branches..." gave `swarm-right-now-creating-branches`. Apple's on-device model (FoundationModels,
macOS 26) can read the prompt and name its task, with no network call. The framework is
available only from Swift, and the server is Rust.

## Decision Outcome

The helper `SwarmName`, shipped in `libexec` beside `SwarmNotify.app`, takes the first 2,000
characters of the prompt as its one argument and prints one kebab-case name. It returns a
structured reply (`@Generable`), and it exits 1 when the model is not available or refuses.

`prompt_branch` runs the helper with a 3 s deadline and kills it after that. The name is model
text, so it passes the same `slug`, length cap and `swarm/` prefix as each branch name. A missing
helper, a model that is off, a refusal, a passed deadline, or a name with no ASCII word gives the
word rules' name of ADR 0146. The `-2` to `-9` names and `untitled-<id>` are unchanged.

- The first send in a `chat-<8 hex>` worktree no longer waits for the name. The rename runs in
  the background, and a failed rename only logs; the next send tries again while the branch is
  unnamed.
- The ⌘N start waits for the model under its worktree lock, because the folder takes the name
  when it is made and cannot change later.
- The composer card names no branch or folder, so typing never runs the model.
- The chat title stays the agent's own title; the model does not make one.

## Consequences

- Good: a branch reads as the task, such as `fix-sidebar-crash` for a long bug report.
- Good: the prompt stays on the Mac.
- Bad: a ⌘N start can take up to 3 s more. The first call after a boot loads the model, and a
  call past the deadline gets the word rules' name.
- Bad: for up to 3 s after the first send, the branch keeps its `chat-<8 hex>` name. An archive
  in that time can still drop the worktree as unused, as it did before the send; a worktree with
  changes stays.
- Bad: an Intel Mac, a Mac with Apple Intelligence off, and a local build without the helper get
  the word rules, with no notice.
