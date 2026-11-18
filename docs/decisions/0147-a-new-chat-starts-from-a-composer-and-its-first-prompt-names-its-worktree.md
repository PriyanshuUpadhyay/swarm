---
status: accepted
date: 2026-11-18
deciders: [user]
amends: ["0146"]
related: ["0097", "0134", "0142"]
informed-by:
  - 'plan ~/.swarm/reports/2026-11-18-conductor-new-chat/plan.md, mock-ups in its site/ folder'
  - 'owner answers 1b 2a 3b 4a 5a, options A + D + C, E later, 2026-11-18'
  - 'owner, 2026-11-18: "2a 3 not sure. cmd t and cmd n doing the same thing?? cmd t should just create a new chat in the same worktree / 4 accepted. but branch name be more task friendly"'
---

# 0147. A new chat starts from a composer, and its first prompt names its worktree

## Context and Problem Statement

Under ADR 0146 a new chat on a hub's main got a worktree on `swarm/chat-<8 hex>`, and the first
prompt renamed the branch later. The owner typed the prompt only after the chat had started.
Conductor, Codex and Claude Code Desktop take the prompt first and then make the worktree. The
New workspace dialog was a second way to make a worktree, with its own branch, PR and issue lists.

## Decision Outcome

⌘N opens one composer. With no chat open, it is the start page; with a chat open, it is an
overlay over the chat. The owner types the first prompt, keeps or changes the project, the place
and the run, and presses ⌘Enter. Enter adds a line. ⌘/ opens the run menu, ⌘⇧/ cycles the
thinking level, ⌘P picks the project, and Esc closes the overlay. The sidebar "New chat", a row
"+" and the tab "+" open the composer on their project.

⌘T starts a chat at once in the open worktree, with no composer and no new worktree, also on a
hub's main.

The place chip of a git project offers a new worktree (the default), an existing branch, a pull
request, an issue, and the folder itself. A new worktree comes from the project, also when it is
no worktree hub, as the New workspace dialog's new branch did. A plain folder shows "In folder". The
New workspace dialog is removed; its lists live in this menu. The run chip starts on the chosen
profile's first runner.

`POST /api/chat/new` takes `prompt` and `place` (`new`, `here`, `branch`, `pr`, `issue`) and
replaces `here: true` with `place: {kind: "here"}`. An absent place keeps ADR 0146: a hub's main
makes the chat's own worktree, and any other folder starts the chat in itself. It checks the request and the
folder, then answers at once with a start id. The worktree, the setup script, the chair and the
first send run in the background. Each step goes out as a `chat-start` frame
`{start, step, state}` with step `worktree`, `setup`, `start`, `send` and last `done`; `done`
carries the session and its folder, or the error. Setup output keeps its `setup` frames. The
app shows a starting tab with these steps at once, and opens the chat when `done` comes. The
server keeps the last 64 `done` frames, and `GET /api/chat/start?id=` returns `{done}`, or
`{running: true}` while the start runs, or `{}` for an id it does not know, so a page whose
stream reconnected reads a `done` it missed and stops waiting on a start the server lost. Text typed in that tab goes after the
first prompt; a send that fails shows that text in a notice. The app waits 10 s for the start id; then it
shows a notice and keeps the composer, because the server may have started the chat. A failed
start offers Edit and retry, which opens the composer again with its prompt, folder and place. A model switch waits for its chat as before.

With a prompt, the server names the worktree when it makes it: `swarm/` and the first five
ASCII words of the prompt that are not filler, lowercase and joined by `-`, at most 38
characters, so a `-2` to `-9` still fits the 40 of a workspace name. Filler words such as "the", "a", "please" and "make" go, so "Fix the flaky smoke
test" gives `swarm/fix-flaky-smoke-test`. A prompt of filler only keeps its words. A taken name
gets `-2` to `-9`, as in ADR 0146; with all nine taken, or with no prompt, the worktree starts
on `swarm/chat-<8 hex>` and the first send renames it as ADR 0146 says. The rename uses the same
words and changes only the branch. A worktree made with a prompt takes the branch's name for its
folder too. The send goes through `swarm type`, which waits for the
agent's input box, so no timer in the app decides when to send.

`GET /api/chat/new-preview?cwd=&prompt=` gives the composer's card: the branch, folder, base,
the last origin fetch and the setup script's first line. It writes nothing; it reads branch
names on each pause in typing.

A failed setup starts no chat (ADR 0097). A worktree of place `new` then goes when it holds no
change; one of another place stays, as the dialog kept it.

## Consequences

- The fetch is part of the worktree step; the card shows the last fetch time instead.
- A starting chat lives only in the app, and its start is one task in the server. A page reload
  loses its tab, and the chat appears in the sidebar when it starts. A server restart during a
  start cuts the start off; a worktree it made stays, and the page shows that the start is lost.
- Two composers that send the same prompt at once can both read one name as free. The second
  create then fails with "The branch … already exists", and nothing is made.
- Outside a worktree hub, the rename and the cleanup of ADR 0146 do not run: a new worktree with
  no prompt keeps `swarm/chat-<8 hex>`, and one whose setup failed stays for the owner to remove.
- The "+" menu item "Default profile, no new worktree" of ADR 0146 is gone; the place menu's
  folder item and ⌘T replace it.
