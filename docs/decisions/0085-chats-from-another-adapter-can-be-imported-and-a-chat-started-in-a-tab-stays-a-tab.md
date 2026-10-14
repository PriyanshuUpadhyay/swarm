---
status: accepted
date: 2026-10-14
deciders: [user]
related: ["0084", "0083"]
informed-by:
  - 'owner, 2026-10-14 (paraphrase): let me import the chats that the app does not list; a chat started in a tab shows only as a tab, and when its tab closes it moves to Recently closed.'
---

# 0085. Chats from another adapter can be imported, and a chat started in a tab stays a tab

In the context of the sidebar after ADR 0084, facing lanes on Herdr or plain tmux that the owner
still wants to read, and tabs that each added a sidebar row, we chose an "Import chats…" dialog
that lists each unlisted chat with a readable log and writes its key to the owner's `imported`
choices, so the app lists it, and a `tab_only` mark on a chat started with ⌘T or the tab strip's
"+", kept in `tab_chats`, so the chat shows only as a tab and closing its tab archives it to
Recently closed, and neglected listing every lane again or hiding a tab chat's row only while its
tab is open, to achieve a sidebar of the chats the owner chose and a tab strip that holds its own
chats, accepting that an imported chat on another adapter is still driven from its own host and
that both sets keep the keys of chats that were later archived.
