# CLI edge cases that can break the composer: Claude Code, Codex, AGY

Date: 2026-10-02. Scope: documented or source-defined behavior of Claude Code 2.1.287, Codex CLI
0.159.0, and Antigravity CLI (AGY) 1.2.14 that touches the composer-gaps contract
(`tmp/flow/2026-10-02-composer-gaps/03-contracts.md`, sections §1-§5 below). Research only. No repo
code changed.

## Result

The contract holds for the main path. A plain sentence typed by `send-keys -l`, then Enter 0.5 s
later, is submitted by all three CLIs. Codex's paste-burst window is 120 ms, far under 0.5 s.
Claude's queue records match §5. But **eight documented cases break it**, and most of them have a
small fix in the app, not in the contract.

Example: the user's draft is `look at @src/main.rs`. The app types it and presses Enter. Claude and
Codex both have a file popup open at the cursor, so Enter completes the path and sends nothing. AGY
also takes Enter as "accept the menu item". The app shows "Sent" (Codex, AGY) or waits for a queue
record (Claude) that never comes.

Rule: before Enter, the app must leave the CLI box in a state where Enter can only mean "submit".
So no open popup, no trailing `\`, no invisible characters, INSERT mode, and no dialog on screen.

## Act on (ranked)

1. **Never type into an open dialog.** Claude's permission prompt takes Enter as `confirm:yes` and
   digits as choices, so a ring can approve a tool call (C5.6). Do not ring or press keys while the
   agent is in "needs you". Use Claude's `PermissionRequest` hook, which fires at once, not
   `Notification` `permission_prompt`, which waits about 6 s (C4.1).
2. **Make Enter mean submit.** Strip the invisible characters Claude refuses (C5.1). Append a space
   when the draft ends in `\` (C5.4b, A5.1) or in an `@`/`$`/`:` token (C5.3, X2.6, A2.1). Map CR to
   LF (X5.2). For Claude 2.1.247+, `C-x Enter` (`chat:queueSubmit`) submits even with a suggestion
   highlighted.
3. **Codex `/new` opens a worktree picker in any git repo.** Offer `/clear` for Codex (X2.1).
4. **Vim mode breaks typing and interrupt in all three CLIs** (C1.5, C5.5, X5.4, A5.2). Read the
   setting (`editorMode` for Claude and AGY, `tui.vim_mode_default` for Codex) and warn. For Codex,
   launch with `-c tui.vim_mode_default=false`.
5. **Interrupt is not one Escape everywhere.** For AGY, use `C-c` (it cannot be remapped, and a
   first `Esc` only closes an open menu, A1.3). For Codex, a first Esc also closes a popup (X5.6).
   Codex Esc with pending steers resubmits them joined by `\n`, so the "Sent" rows must match the
   joined text or clear at turn end (X1.5).
6. **Clear every "Sent" row at turn end.** Codex and AGY store normalised or rewritten text, and
   hooks, blocked commands, `!`/`@agent` prefixes, and size limits can stop a row from ever
   appearing (X1.7-X1.9, A2.4, A2.5, C4.3).
7. **Plugin names and paths.** The command prefix is plugin.json `name`, not the id or folder
   (C7.3, A7.1). Claude `skills` adds to `./skills`, while `commands` replaces `./commands` and may
   be an object map. An id that no `enabledPlugins` names is on by default per the docs (C7.1, C7.4), but see the check under C7.4.
8. **Pull-back must re-check before it says "Already sent".** A rebound or unbound Up gives no
   `popAll` and no `remove`, so check `pending` again at the deadline (C5.7). Also strip a
   `<pasted_content id=…>` wrapper from popped text (C1.9).

## Could not verify

- How tmux `send-keys -l` passes `\n` to the pane. `man tmux` (3.7c) only says `-l` sends "literal
  UTF-8 characters". All three CLIs take `Ctrl+J` as a newline, so LF is safe if it arrives as
  0x0A. One live test settles it.
- Whether Claude's `/new` and `/reset` log `<command-name>/clear` or their alias name (C2.4).
- Which Claude key action the queue take-back uses, so whether rebinding `history:previous` also
  disables it (C5.7).
- Whether a `send-keys -l` burst counts as a paste in Claude (and so yields `<pasted_content>` in
  the queue record). The wrapper was seen only in the owner's logs (C1.9).
- Whether a trailing space closes the Codex `$`/`@` popup and Claude's `@` suggestion (X2.6, C5.3).
- Whether the Claude vim box returns to INSERT after a submit (C5.5).
- Codex official docs at developers.openai.com/codex were not read; the Codex rows rest on source
  only. Whether untrusted Codex project layers still add `.codex/skills`.
- AGY 1.2.14 itself (no release notes past 1.2.13); which AGY commands are blocked or queued
  mid-turn; whether `/clear` and `/fork` update `last_conversations.json`; the `transcript.jsonl`
  line schema; what `Ctrl+U` does in AGY.
- Whether typed text sent to Claude can start with `<` and so be dropped by the §5 tag filter
  (C6.2).

## How to read the tables

Each row gives the case, the exact source, the contract section it touches, an impact tag
(**breaks**, degrades, none), and the smallest fix when it breaks. "Observed" means the owner's
local session logs, which are evidence but not a doc. A claim with no primary source is marked
(unverified).

## Claude Code 2.1.287

Doc URLs below are under `https://code.claude.com/docs/en/`. CHANGELOG lines are from
`https://github.com/anthropics/claude-code/blob/main/CHANGELOG.md` (fetched 2026-10-02, top entry
2.1.287). "Observed" means this owner's local session logs, not a doc.

### C1. Queueing and steering

| # | Edge case | Source | Contract | Impact | Smallest fix |
|---|---|---|---|---|---|
| C1.1 | A queued **message** reaches Claude when the running tool calls finish, inside the same turn. A queued **command** or `!` command waits for turn end and runs one at a time. | `interactive-mode#when-claude-code-sends-what-you-queued` | §5 rows | none (matches §1) | – |
| C1.2 | Some commands skip the queue and run at once: `/status`, `/tasks`, `/usage`, `/model`, `/effort`, `/fast`, `/rename`, `/btw`; in fullscreen also dialogs such as `/theme`, `/help` (v2.1.234+). A whole-draft `/model` sent mid-turn opens a picker on top of the running turn. | `commands` (intro, para 3); `interactive-mode#queue-messages-while-claude-works`; CHANGELOG 2.1.234 | §2 whole-draft send, §5 | degrades | Hold the "Queued" row only for text that is not a known immediate command. |
| C1.3 | Up from the first input row takes back **all** queued messages and commands, one per line, **ahead of** any typed text. A queued `!` command comes back only when the box is empty and nothing else is queued. | `interactive-mode#take-back-what-you-queued` | §5 pull back | none for messages; degrades for `!` (it stays queued and runs) | Treat a pending `!…` entry as not pull-backable. |
| C1.4 | Plain Up with nothing queued recalls history; the same prompt twice counts once. | `interactive-mode#command-history` | §5 step 3 | none (already handled by C-u) | – |
| C1.5 | `Esc` interrupts the turn and **sends the queue right away**. In vim mode, the first `Esc` in INSERT only goes to NORMAL; a second `Esc` interrupts (CHANGELOG 2.1.119, "Vim mode: Esc in INSERT no longer pulls a queued message back… press Esc again to interrupt"). `Esc` + `Esc` on an empty box opens the **rewind menu**; with text it clears the draft. While a footer item is selected, `Esc` only deselects. | `interactive-mode` key table rows `Esc`, `Esc + Esc`; `keybindings#vim-mode-interaction` | interrupt verb; §5 rows | **breaks** in vim mode (one Escape does not stop the turn); degrades otherwise (Queued rows flip to sent at once) | Read `editorMode` from `<configDir>/settings.json`; when `"vim"`, send Escape twice for interrupt. Never send Escape twice on a non-vim idle box. |
| C1.6 | `Ctrl+Enter` / `Ctrl+X Ctrl+S` (`chat:sendNow`, v2.1.275+) sends the queue now; since v2.1.281 it backgrounds running shell work instead of interrupting. `Ctrl+X Enter` (`chat:queueSubmit`, v2.1.247+) queues without interrupting **and submits even while an autocomplete suggestion is highlighted**. | `keybindings#chat-actions`; `interactive-mode#when-claude-code-sends-what-you-queued` | ring (`send-keys Enter`) | none today; see C5.3 for why `queueSubmit` matters | – |
| C1.7 | A queued `/compact <long text>` is sometimes sent to the model as a plain prompt (`promptSource:"queued"`), so no compaction happens (7 of 36 on 2.1.257). Open issue, labels `bug`, `has repro`. | github.com/anthropics/claude-code/issues/92478 | §2 whole-draft send mid-turn | degrades | Do not offer `/compact` (or other session commands) as a mid-turn send; or show the row as plain text when the next user record has `promptSource:"queued"`. |
| C1.8 | A message absorbed mid-turn is logged as `queue-operation` `enqueue` → `remove` with `reason:"absorbed_mid_turn"` → an `attachment` of `type:"queued_command"` with `origin.kind:"human"`, parented to the tool result. Not a `user` record. | issue #97575 (labels `bug`, `has repro`, `area:core`); observed reasons: `absorbed_mid_turn` 1624, `delivered_to_agent` 20 | §5 "Sent" matching, Z parser | none (Z already parses `queued_command`, `root.zig:330`) | Keep it; the Queued row must end on `remove`, not wait for a `user` record. |
| C1.9 | A paste-collapsed message is queued and logged as `<pasted_content id="…">…` around the text, also in `popAll`. | observed (12 `enqueue`/`popAll` records); `terminal-config#how-claude-treats-pasted-text` says pasted text is "marked" | §5 replay equality, pull-back draft text | degrades (draft gets the wrapper; equality with app text fails) | Strip one outer `<pasted_content id="…">…</pasted_content>` before compare and before putting text in the draft. |
| C1.10 | `dequeue` records carry no `content`. | observed (`{"operation":"dequeue",…}` with no content) | §5 `pending` | none (head drop is right) | – |

### C2. Slash command dispatch

| # | Edge case | Source | Contract | Impact | Smallest fix |
|---|---|---|---|---|---|
| C2.1 | A command is recognized only at message start. Exception (v2.1.199+): up to six skills chained at the start, `/a /b text`. | `commands` intro para 2 | §3 mid-draft menu | none for the menu; mid-sentence `/skill` text is sent as plain text, Claude may still load the skill itself | Label mid-sentence picks as "mention", not "run". |
| C2.2 | Enter runs the **highlighted** suggestion only when the typed letters match a name or alias from a word start (v2.1.236+). After a typo nothing is highlighted and Enter submits the raw text → `Unknown command`. `/new` highlights `/clear` through its alias. | `commands#how-the-command-menu-matches-what-you-type` | §1 "Enter on a partial `/cle`", §3 `pickAndSend` | none if the app always sends the full picked name; degrades if it sends the partial | Send the picked full name, never the partial. |
| C2.3 | Hidden commands (e.g. `/heapdump`) appear only when the full name is typed. Unavailable commands are left out of the menu. | same section | §2 built-in list | none | – |
| C2.4 | Aliases: `/clear` = `/reset` = `/new`; `/resume` = `/continue`; `/rewind` = `/checkpoint` = `/undo`; `/config` = `/settings`; `/usage` = `/cost` = `/stats`; `/exit` = `/quit`; `/code-review` = `/review`. A user skill named like a bundled command replaces it but not its alias. | `commands` table rows; `skills#resolve-skills-that-share-a-name` | §2 built-ins, §4 `isClear` | degrades: §4 matches only `<command-name>/clear`. Whether `/new` logs `/clear` or `/new` is (unverified) | Rely on `SessionStart:clear` first; also accept `/new` and `/reset` in the command fallback. |
| C2.5 | The docs list 119 commands; many open pickers that need the terminal (`/model`, `/resume`, `/config`, `/skills`, `/plugin`, `/rewind`, `/branch` prints two ids). | `commands` table | §2 built-in list | degrades (a picker opens in the pane the user does not see) | Mark picker commands in the built-in list and show "opens in the terminal". |
| C2.6 | MCP prompts appear as commands (`/mcp__server__prompt`). | `commands#mcp-prompts` | §2 discovery | degrades (missing rows) | – (out of scope) |

### C3. Session rotation

| # | Edge case | Source | Contract | Impact | Smallest fix |
|---|---|---|---|---|---|
| C3.1 | SessionStart `source` is `startup`, `resume` (`--resume`, `--continue`, `/resume`), `clear`, `compact`, or `fork` (`--fork-session`, `/fork`, `/branch`, background move). Before v2.1.214 forks reported `resume`. | `hooks#sessionstart` matcher table; CHANGELOG 2.1.214 | §4 `isClear`, bus hook | degrades: `/branch` and `/resume` also move `agent.log`; §4 then replaces rows (today's behavior), which is right for resume, but loses the old rows for `/branch` | Pass the hook `source` from `S/bus.rs` with the log path; freeze for `clear` only, show "Branched" for `fork`. |
| C3.2 | `/clear` writes a new `<session-id>.jsonl`; the old one stays and can come back through `/resume` or the rewind menu's `/resume <id> (previous session)` row in the same process. | `sessions` (Clear and compact list); `checkpointing#rewind-past-a-cleared-conversation` | §4 | degrades: returning to the old session moves the log back with `source:"resume"`; frozen rows then repeat | On `resume` to a path already in `frozenRows`' history, drop the frozen rows. |
| C3.3 | `/compact` keeps the same file: `system` `compact_boundary` then `SessionStart:compact` hook records. | observed (`d680b4bf….jsonl` lines 746, 760-763) | §4 "same path never freezes" | none | – |
| C3.4 | `/branch` copies the transcript into a new session id and switches the running process to write it. | `sessions#branch-a-session` | §4 | degrades (see C3.1) | as C3.1 |
| C3.5 | SessionStart hooks run in the background after `/clear`; a prompt sent before they finish waits; `/clear` during a running hook discards its output; `Esc` takes the waiting prompt back. | `hooks#sessionstart` (paras after the matcher table) | §4 child `agent.log` move; §5 "Sent" | degrades (a send right after `/clear` shows no reply for the hook time) | – (show "Sent" as for Codex until the user record appears) |
| C3.6 | The transcript file "is written asynchronously and may lag"; a hook can fire before the records exist. | `hooks#common-input-fields` (`transcript_path` row) | §4 `isClear` on first read; §5 1.5 s pull-back deadline | degrades (first read of the new log may hold no `SessionStart:clear` yet → no divider) | Re-run `isClear` when the new log grows, until it holds 30 records; or use C3.1's `source`. |
| C3.7 | Transcript format "is internal to Claude Code and changes between versions". `CLAUDE_CODE_SKIP_PROMPT_HISTORY` stops transcript writes. `CLAUDE_CONFIG_DIR` and `CLAUDE_CODE_PROJECT_DIR_NAME` move or rename the folder; long cwd names are cut at 200 chars plus a hash. | `sessions#export-and-locate-session-data` | §2 `configDirectory(fromLog:)`, §4, §5 | degrades (no log = no Queued rows, no divider) | `configDirectory` must take the parent of `projects/`, not rebuild the slug. |

### C4. Hooks that time log reads

| # | Edge case | Source | Contract | Impact | Smallest fix |
|---|---|---|---|---|---|
| C4.1 | `PermissionRequest` fires the moment a permission prompt opens; `Notification` `permission_prompt` only after about 6 s; `idle_prompt` about 60 s after the reply. | `hooks#permissionrequest`; `hooks#notification` matcher table | ring while a dialog is open (C5.6) | degrades | Use `PermissionRequest` (not `Notification`) to mark "needs you". |
| C4.2 | `Stop` input has `last_assistant_message`; the docs tell hooks to use it, not the transcript, because the transcript may lag. | `hooks#stop` input | turn-end detection, "Sent" rows | degrades (a read right after `Stop` can miss the last records) | Re-read once on the next file change. |
| C4.3 | `UserPromptSubmit` default timeout 30 s; a blocked prompt still writes the block message with `Original prompt:` to the transcript. | `hooks#userpromptsubmit`; `#what-a-blocked-prompt-leaves-behind` | §5 "Sent" matching | degrades (no `user` record with equal text; row stays "Sent") | Clear "Sent" rows at turn end. |
| C4.4 | Hook results show in the log as `attachment` `hook_success` with `hookName` `SessionStart:<source>`, one per hook. They exist only when a hook is configured. | observed (`1a51c0ed….jsonl` lines 4-5, 7) | §4 `isClear` | none for the chair (the bus adds a hook); degrades for a bare Claude with no hooks (fallback to `<command-name>`) | – |

### C5. Input box and key presses

| # | Edge case | Source | Contract | Impact | Smallest fix |
|---|---|---|---|---|---|
| C5.1 | **Invisible characters**: Claude Code removes tag characters, bidi controls and zero-width spaces on Enter. "If Claude Code removed anything, that Enter sends nothing"; the cleaned prompt goes back in the box with `Removed N invisible characters · review and press Enter to send`. | `interactive-mode#invisible-characters-in-prompts`; CHANGELOG 2.1.277 | ring; §5 "Sent"/Queued | **breaks** (the app thinks it sent; the text sits in the CLI box; the next Up/C-u logic then works on a full box) | Strip the same classes (U+E0000–E007F, U+200B–U+200F, U+202A–U+202E, U+2066–U+2069, U+FEFF; keep ZWJ/ZWNJ in words and emoji VS) in the app before `send-keys`. |
| C5.2 | Paste of more than 800 characters or more than three lines collapses to `[Pasted text #N +L lines]`; the full text is sent. | `terminal-config#paste-large-content` | ring of long text; §5 C-u count | degrades (if the burst is seen as a paste, see C1.9; C-u across a placeholder removes it whole) | – (C-u count stays safe, extra presses do nothing) |
| C5.3 | Enter (`chat:submit`) does not submit while an autocomplete suggestion is highlighted; it accepts the suggestion. `chat:queueSubmit` (`Ctrl+X Enter`) submits regardless. Autocomplete opens for `/` at start, `@` paths, `:` emoji (v2.1.217+). | `keybindings#chat-actions` (`chat:queueSubmit` row); `interactive-mode#quick-commands` | ring (`send-keys Enter`) | **breaks** for a draft that ends inside an `@path` or `:emoji` token: Enter completes the token and nothing is sent | Send `C-x` `Enter` instead of `Enter` for Claude (2.1.247+), or append a space before Enter. |
| C5.4 | Multi-line: `\`+Enter, `Ctrl+J` (any terminal), Shift+Enter (needs tmux `extended-keys`), bracketed paste. A newline byte from `send-keys -l` arrives as `Ctrl+J`, which is `chat:newline` (tmux behavior (unverified)). Without bracketed paste, multi-line text was once submitted line by line after the terminal's paste mode reset (fixed 2.1.282). | `interactive-mode#multiline-input`; `terminal-config#configure-tmux`; CHANGELOG 2.1.282 | ring of multi-line text; §8 "queued message that holds newlines" | degrades if a user rebinds `ctrl+j` | Read `keybindings.json` (C5.7). |
| C5.4b | `\` + Enter is the "Quick escape" newline in all terminals, so a text that ends in `\` gets a newline, not a submit. | `interactive-mode#multiline-input` (row "Quick escape") | ring | **breaks** (text stays in the box; the next ring joins it) | Append a space when the text ends in `\` (same fix as A5.1). |
| C5.5 | Vim mode (`editorMode: "vim"`): after an `Esc` the box is in NORMAL mode, so typed text runs as vim commands; `vimInsertModeRemaps` (for example `jj`→Esc, within 1 s) fires inside a fast `send-keys` burst. Up/`k` in NORMAL at the top go to history. | `interactive-mode#vim-editor-mode`, `#remap-insert-mode-key-sequences`; `keybindings#vim-mode-interaction`; CHANGELOG 2.1.284 ("text typed very fast… in tmux") | ring after interrupt; §5 pull back | **breaks** for vim users | Read `editorMode` and `vimInsertModeRemaps` from `<configDir>/settings.json` and show a warning under the composer. Do not send `Escape` to force a mode, because `Escape` mid-turn interrupts. Whether the box goes back to INSERT after a submit is (unverified). |
| C5.6 | A permission prompt, `AskUserQuestion`, or a picker owns the keys: Enter is `confirm:yes`, digits pick options, `Esc` declines, Up/Down move. | `keybindings#confirmation-actions`; `interactive-mode` key table `Esc` row | ring; §5 Up and C-u | **breaks** (a ring can approve a tool call) | Do not ring or press keys while the agent state is "needs you" (from `PermissionRequest`). |
| C5.7 | `~/.claude/keybindings.json` can rebind or unbind `enter` (`chat:submit`), `up` (`history:previous`), `escape` (`chat:cancel`), `ctrl+j`; it hot-reloads. Reserved (not rebindable): `Ctrl+C`, `Ctrl+D`, `Ctrl+M`, `Ctrl+[`, `Ctrl+I`, `Ctrl+H`. `Ctrl+U` is not in the action list, so it is not documented as rebindable. Which action Up's take-back uses is not documented (unverified). | `keybindings` (Configuration file, History actions, Chat actions, Reserved shortcuts) | ring, interrupt, §5 pull back | **breaks** when rebound (Enter makes a newline; Up does nothing, and §5 step 3 then wrongly says "Already sent") | Read `<configDir>/keybindings.json`; if `enter`, `up`, or `escape` in `Chat`/`History` is remapped, show a one-line warning and skip pull-back. After the 1.5 s deadline, check `pending` again before saying "Already sent". |
| C5.8 | Prompt suggestions: gray next-prompt text in an empty box; Tab or Right places it. Typing dismisses it. | `interactive-mode#prompt-suggestions` | §5 Up on an empty box | none (Up is not a suggestion key) | – |

### C6. Log formats

| # | Edge case | Source | Contract | Impact | Smallest fix |
|---|---|---|---|---|---|
| C6.1 | No doc defines `queue-operation`, `queued_command`, or `hook_success`; the docs say the format is internal and changes between versions. Observed operations: `enqueue`, `dequeue`, `remove`, `popAll`; reasons `absorbed_mid_turn`, `delivered_to_agent`. `remove` also carries `commandUuid`, `deliveryId`. | `sessions#export-and-locate-session-data`; observed | §5 (A4: unknown ops ignored) | degrades on any release | Keep A4. Add a parser test per observed shape (exists at `root.zig:1137-1149`). |
| C6.2 | Task notifications and agent messages are also queued (`<task-notification`, `<agent-message`); since 2.1.287 replies from `claude agents` arrive as queued messages. | CHANGELOG 2.1.287; observed counts 2866 and 140 | §5 filter | none (filter exists); new prefixes may appear | Filter on any leading `<tag` that is not typed text (unverified that typed text never starts with `<`). |

### C7. Skills, plugins, commands discovery

| # | Edge case | Source | Contract | Impact | Smallest fix |
|---|---|---|---|---|---|
| C7.1 | plugin.json `skills` **adds to** the default `skills/` scan; `commands` **replaces** the default `commands/` scan. `commands` may also be an **object map** of name to `{source}` or `{content}`. Paths must start with `./`; `skills` also takes `"."`. A path outside the plugin root or missing does not load. | `plugins-reference#how-each-key-combines-with-its-default-location`, `#path-only-fields`, `#commands`, `#containment-and-existence` | §2 `ComposerPluginReader` | degrades (§2 says "default is `./skills`", read as replace) | Always scan `./skills`, then add listed paths; for `commands`, accept the object map keys as names. |
| C7.2 | A plugin with `SKILL.md` at its root and no `skills/` loads as one skill named by frontmatter `name`, else the plugin dir name. | `plugins-reference#standard-layout` (Skills row); `skills#how-a-skill-gets-its-command-name` | §2 | degrades | Check `<installPath>/SKILL.md`. |
| C7.3 | The command prefix is the **manifest** `name`, not the `enabledPlugins` id's name part (the marketplace entry name); they can differ. | `plugins/loading#entry-name-and-manifest-name`; `plugins-reference#metadata-precedence` | §2 `ComposerPlugin.name` | degrades (wrong `plugin:` prefix → `Unknown command`) | Take the prefix from `plugin.json` `name`, fall back to the id. |
| C7.4 | An id absent from every `enabledPlugins` is **on** when `defaultEnabled` is unset (default `true`); a dependency of an enabled plugin is on regardless. | `plugins-reference#defaultenabled` | §2 "absent or `false` is disabled" | degrades (missing rows) | Absent → use entry/manifest `defaultEnabled`, default true. |

Check on 2026-10-02: on the owner's Mac (Claude Code 2.1.287), `claude plugin list --json` reports `cloudflare@cloudflare`, which no `enabledPlugins` names, as `enabled=false`. So the app keeps "absent means disabled" until a case shows otherwise.

| C7.5 | `enabledPlugins` has six sources, low to high: `--add-dir`, user, project, local, `--settings` flag, managed (managed `true`/`false` wins over all). | `plugins/loading#find-where-a-plugin-is-enabled` | §2 merge order | degrades (managed ignored) | Add managed settings as the last file. |
| C7.6 | Plugins outside `installed_plugins.json`: `@inline` (`--plugin-dir`, `CLAUDE_CODE_PLUGIN_DIRS`), `@skills-dir` (a skill folder with `.claude-plugin/plugin.json`), `@synced` (claude.ai), relative-path marketplace plugins (no install record). A marketplace entry can also append `skills`/`commands` to `plugin.json` (`strict`). | `plugins/loading#find-where-a-plugin-came-from`, `#enabled-in-project-settings-but-not-installed`; `plugins-reference#how-entry-fields-combine-with-pluginjson` | §2 | degrades | – (note as a known gap) |
| C7.7 | Skill name comes from frontmatter `name` (else dir name). Nested `.claude/skills` with a clashing name is `/apps/web:deploy`. `.claude/commands/a/b.md` is `/a:b`. Claude.ai synced skills live in `~/.claude/skills/synced/` and are `/anthropic-skills:name` (bare name if free). `synced` and `anthropic-skills` are reserved folder names. | `skills#how-a-skill-gets-its-command-name`, `#where-synced-skills-load`, `#where-skills-live` | §2 Claude folders | degrades | Read frontmatter `name`; map `skills/synced/*` to `anthropic-skills:`; join command subfolders with `:`. |
| C7.8 | Name clash: enterprise > personal > project; a skill beats a same-named `.claude/commands` file; plugin skills never clash (namespaced). | `skills#resolve-skills-that-share-a-name` | §2 `matches` | none (rank only) | Dedupe by name with that order. |
| C7.9 | `user-invocable: false` hides a skill from the `/` menu and `/name` does not run it. `skillOverrides` in settings (`"off"`, `"user-invocable-only"`, …) hides or shows skills; plugin skills ignore it. `disable-model-invocation` does not hide from the menu. | `skills#control-who-invokes-a-skill`, `#override-skill-visibility-from-settings` | §2 | degrades (the app lists skills that do not run) | Skip `user-invocable: false` and `skillOverrides: "off"`. |
| C7.10 | Project skills load from `.claude/skills` in cwd and each parent up to the repo root; in a linked worktree only up to the worktree root, and (v2.1.277+) from the main checkout when the worktree has no `.claude/skills`. Skills below cwd load only after Claude touches files there. | `skills#discovery-from-parent-and-nested-directories` | §2 Claude folders | degrades (Swarm runs in `wt/` worktrees) | Walk cwd → repo or worktree root; add the main checkout's `.claude/skills` when the worktree has none. |
| C7.11 | `~/.agents/skills` is not a documented Claude Code skill folder. | `skills#where-skills-live` (no such row) | §2 Claude folders | degrades (rows that return `Unknown command`) unless the user links it | Drop it for Claude, or keep only when symlinked into `~/.claude/skills` (unverified intent). |

## Codex CLI 0.159.0

Paths are in `github.com/openai/codex` at tag `rust-v0.159.0`, relative to `codex-rs/`. All rows are
from source reading. The official docs at developers.openai.com/codex were not fetched (see Could
not verify). Rows marked "checked" were re-read by the chair; the rest come from one source worker.

### X1. Queueing and steering

| # | Edge case | Source | Contract | Impact | Smallest fix |
|---|---|---|---|---|---|
| X1.1 | Enter while a turn runs **steers** (`handle_submission(self.queue_submissions)`); Tab queues. Defaults Enter=submit, Tab=queue. | `tui/src/bottom_pane/chat_composer.rs:3523-3531`; `tui/src/keymap.rs:1680-1681` | §1 row 1, §5 Codex "Sent" | none | – |
| X1.2 | Enter becomes a **local queue** (sent later as a new turn) when the session is not configured, a plan streams, a rate-limit recovery runs, a submitted turn has not started yet, or only `!` shell commands run. | `tui/src/chatwidget/input_flow.rs:61-83`; `input_submission.rs:141-189` | §5 caption "joins after the next tool call" | degrades (it joins as a new turn) | Caption "Sent" only, as for AGY. |
| X1.3 | Review and compact turns refuse steers (`ActiveTurnNotSteerable`); the TUI holds them and sends after the turn. | `core/src/session/turn_input.rs:658-672`; `tui/src/chatwidget/turn_runtime.rs:304-309` | §5 caption | degrades | as X1.2 |
| X1.4 | A pending steer is drained at the top of the next sampling step, so it joins after the current step, tool call or final answer; it is not lost at turn end. | `core/src/session/turn.rs:428-443`, `:552-564` | §5 caption | none (wording only) | – |
| X1.5 | **Esc with pending steers interrupts and resubmits them all as one user turn, joined with `\n`**, and prints "Model interrupted to submit steer instructions." | `tui/src/chatwidget/interaction.rs:203-216`; `input_restore.rs:315-386`; `user_messages.rs:413-434` | interrupt verb; §5 "Sent" equality | **breaks** (with two or more pending rows the logged text is `A\nB` and matches no row) | On interrupt, match a row against the `\n`-join of the pending rows, or clear "Sent" rows at `turn_complete`/`turn_aborted`. |
| X1.6 | Esc with only Tab-queued messages puts them back in the composer, unsent. Shift+Left / Alt+Up pop the latest queued message, else the latest pending steer, into the composer. | `input_restore.rs:387-391`; `keymap.rs:1675`; `chatwidget/reconnect.rs:113-118` | §5 Codex `pullBack` nil | degrades only if a user presses them in the pane | – |
| X1.7 | Submitted text is normalised: CRLF→LF, control and escape sequences removed, paste placeholders expanded, trimmed (`trim_submission` default true). | `tui/src/paste_input.rs:119-120`; `chat_composer.rs:578`, `:3017-3031` | §5 "Sent" equality | degrades | Compare trimmed text, LF line ends, control characters removed. |
| X1.8 | Text over 1 MiB (`MAX_USER_INPUT_TEXT_CHARS = 1<<20`) or an unknown `/x` stays in the composer with an error. A draft that is only a path to an existing image becomes `[Image #N]`. | `protocol/src/user_input.rs:10`; `chat_composer.rs:3033-3084`, `:1223-1243` | §5 "Sent" | degrades ("Sent" never clears) | Clear "Sent" rows at turn end. |
| X1.9 | A `UserPromptSubmit` hook can block a steer; blocked input is never recorded. | `core/src/session/turn.rs:855-893` | §5 "Sent" | degrades | as X1.8 |

### X2. Slash command dispatch

| # | Edge case | Source | Contract | Impact | Smallest fix |
|---|---|---|---|---|---|
| X2.1 | **`/new` opens a picker, "Where should the new conversation run?"**, whenever `Feature::Worktrees` is on (Stable, default on), `local_worktree_operations` is true (default), and cwd is in a git repo. `/fork` does the same. `/clear` starts a fresh session with no picker. Checked. | `tui/src/chatwidget/slash_dispatch.rs:204-206`; `tui/src/chatwidget/worktree_picker.rs:16-45`; `features/src/lib.rs:1309-1313`; `tui/src/app/event_dispatch.rs:385-399` | §2 whole-draft `/new` on one Return; §1 row 6 | **breaks** (the Return opens the picker; nothing rotates) | Offer `/clear` for Codex, not `/new`. |
| X2.2 | Blocked while a task runs (draft cleared, "'/<cmd>' is disabled while a task is in progress."): new, archive, delete, fork, worktree, init, compact, recap, export, keymap, tui, vim, elevate-sandbox, experimental, memories, import, plan, cd, clear, logout, theme, pets, memory debug commands. `/review` is checked at dispatch. | `tui/src/slash_command.rs:213-215`, `:239-301`; `chat_composer.rs:3392-3407` | §2 whole-draft send mid-turn | degrades (the draft is lost) | Grey out these commands while `isRunning`. |
| X2.3 | Aliases: `/cwd`→`/pwd`, `/pet`→`/pets`, `/clean`→`/stop`, `/subagents`, `/approve`. `/new` and `/clear` are separate commands; there is no `/reset`. `/rollout`, `/test-approval` are hidden in release builds. | `slash_command.rs:20-84`, `:304-312` | §2 Codex list; §1 row 6 | none | – |
| X2.4 | Enter on a partial `/cle` runs the highlighted popup item; a bare `/name` with no popup also dispatches. | `chat_composer/slash_input.rs:360-403`; `chat_composer.rs:3228-3237` | §1 row 4 | none | Send the full name. |
| X2.5 | `/resume` opens a picker; `/model`, `/keymap`, `/memories`, `/title`, `/statusline`, `/theme` open settings views. | `slash_dispatch.rs:271-273`; `input_flow.rs:26-35` | §2 | degrades | Mark as "opens in the terminal". |
| X2.6 | **Enter or Tab with a `$skill` or `@file` popup open inserts the selected item and does not submit.** Checked for the Enter arm. | `chat_composer.rs:2170-2195` (files), `:2252-2260` (skills) | ring; §3 `$` menu | **breaks** for a message whose last word is `$name` or `@path` | Append a space before Return (trim removes it, `chat_composer.rs:3027-3031`). Whether a space closes the popup is (unverified). |
| X2.7 | A `$skill` mention is a separate `UserInput::Skill` item, and the skill body is injected as a separate user-role message wrapped in `<skill>…</skill>`. | `input_submission.rs` ~312-376; `ext/skills/src/fragments.rs:90` | transcript rows | degrades (a `<skill>` row shows as user text) | Hide role=user text that starts with `<skill>`. |
| X2.8 | A slash command in the Tab queue is parsed when it leaves the queue. | `chat_composer.rs:3169-3205`; `input_flow.rs:177-196` | – | none | – |

### X3. Session rotation

| # | Edge case | Source | Contract | Impact | Smallest fix |
|---|---|---|---|---|---|
| X3.1 | `/clear` starts a new thread id (`ThreadStartSource::Clear`); `/new` does too after the picker; `/fork` sets `forked_from_id` in `SessionMeta`. | `tui/src/app/event_dispatch.rs:385-399`; `protocol/src/protocol.rs:3123-3135` | §4 (Codex chair out of scope) | none today | – |
| X3.2 | **A new rollout file is created only on the first write** (deferred creation). | `rollout/src/recorder.rs:970-973`, `:1844-1846` | log discovery after `/clear` | degrades (no file until the next send) | Poll for the newest rollout after the first send. |
| X3.3 | `/compact` and auto-compact stay in the same file and append `compacted` with `replacement_history` (copies of user messages). Auto-compact can run mid-turn. | `history/src/lib.rs:275-287`; `core/src/session/turn.rs:710-775` | §5 "Sent" equality | degrades (a copied user text must not count as a new row) | Do not read `replacement_history` as rows. |
| X3.4 | Rollouts older than 7 days are compressed to `rollout-*.jsonl.zst`; a reverted thread is `rollout-<ts>-<threadId>_<rolloutId>.jsonl`. | `rollout/src/compression.rs:26`, `:335-341`; `rollout/src/rollout_file_name.rs:11-14`, `:39-47` | §2 `configDirectory(fromLog:)` | degrades (a resumed old session's path changes) | Accept `.jsonl.zst` and the `_` form in path parsing. |

### X4. Hooks and write timing

| # | Edge case | Source | Contract | Impact | Smallest fix |
|---|---|---|---|---|---|
| X4.1 | Hook events: PreToolUse, PermissionRequest, PostToolUse, PreCompact, PostCompact, SessionStart, SessionEnd, UserPromptSubmit, SubagentStart, SubagentStop, Stop, Interrupt. Sources: `hooks.json` in a config folder and `[hooks]` in TOML; hooks need trust. Legacy `notify` sends `agent-turn-complete`. | `app-server-protocol/src/protocol/v2/hook.rs:19-21`; `hooks/src/engine/discovery.rs:343`, `:387`; `hooks/src/legacy_notify.rs:13-15` | future Codex `/new` support | none today | – |
| X4.2 | The rollout writer is an async task fed by a channel; it flushes after each batch, no fsync. A row can land after the TUI shows the event. | `rollout/src/recorder.rs:1790-1797`, `:1888-1899`, `:1938-1943` | §5 "Sent" | degrades | Re-read on file change. |

### X5. Input box

| # | Edge case | Source | Contract | Impact | Smallest fix |
|---|---|---|---|---|---|
| X5.1 | Paste-burst heuristic: `PASTE_BURST_MIN_CHARS = 3`, `PASTE_BURST_CHAR_INTERVAL = 8ms`, `PASTE_BURST_ACTIVE_IDLE_TIMEOUT = 8ms` (60 ms on Windows), `PASTE_ENTER_SUPPRESS_WINDOW = 120ms`. Enter is a newline only during a burst or inside the 120 ms window. Checked. | `tui/src/bottom_pane/paste_burst.rs:159-170`; `chat_composer/reconnect.rs:17-48`; `paste_burst.rs:345-348` | ring (0.5 s sleep, then Enter) | none (0.5 s is far outside 120 ms) | Keep the sleep above 120 ms. |
| X5.2 | A raw CR inside the text is Enter. Inside an ASCII burst it becomes a newline, but not when the first line starts with `/`, and not for non-ASCII text (which starts buffering only after 16+ chars or a space). LF (`Ctrl+J`) is a default newline key. | `chat_composer.rs:3633-3643`; `paste_burst.rs:401-421`; `chat_composer.rs:2044-2110`; `keymap.rs:1690-1696` | ring of multi-line text | **breaks** when the text holds `\r` (early submit) | Normalise CRLF and CR to LF in the app before `send-keys -l`. |
| X5.3 | Over 1000 chars the draft shows `[Pasted Content N chars]`; the full text is sent. Checked. | `chat_composer.rs:447`, `:201-210`, `:3017-3025` | – | none | – |
| X5.4 | `tui.disable_paste_burst` turns burst detection off; `tui.vim_mode_default = true` starts the composer in Vim NORMAL (typed text runs as Vim commands); with Vim on, Esc in INSERT leaves INSERT and does not interrupt. | `config/src/types.rs:838-847`; `tui/src/bottom_pane/mod.rs:1713` | ring, interrupt | **breaks** for Vim users; degrades with burst off (multi-line text then needs LF, which is X5.2's fix) | Launch Codex with `-c tui.vim_mode_default=false`. |
| X5.5 | `tui.keymap.<context>` can rebind or unbind submit, queue, `interrupt_turn`, the newline keys, `kill_line_start` (Ctrl+U), and `move_up`. | `tui/src/keymap.rs:61-78`, defaults `:1643-1750` | ring, interrupt | **breaks** when rebound | Pin the keys the app uses with `-c tui.keymap…` at launch (exact key syntax (unverified)). |
| X5.6 | Esc interrupts only when a task runs, no popup or modal is open, the shortcut overlay is closed, and the draft is not `/agents`; otherwise the first Esc closes the popup. On an empty idle composer Esc arms backtrack. | `bottom_pane/mod.rs:1700-1715`; `chat_composer.rs:3512-3519` | interrupt verb | degrades | Send Esc only while running; a second Esc if the first closed a popup. |

### X6. Rollout format

| # | Edge case | Source | Contract | Impact | Smallest fix |
|---|---|---|---|---|---|
| X6.1 | **New threads use paginated history by default.** In that mode `event_msg` `user_message` and `agent_message` are no longer written; user text goes into `event_msg` `item_completed` with `item.type:"UserMessage"`. `response_item` `message` rows are still written. Resumed old threads keep Legacy (`session_meta.payload.history_mode`). Checked; a local 0.159.0 rollout has `history_mode:"paginated"`, no `user_message`, one `item_completed` `UserMessage`. | `thread-store/src/local/mod.rs:474-476`; `history/src/lib.rs:481-488`; `rollout/src/policy.rs:44-46`, `:96-136`; `protocol/src/items.rs:46-87` | §5 "Sent" matching; Z `codex.zig` | none today (`codex.zig:112-173` reads `response_item` role user and skips `item_completed` at `:86`), but any matcher keyed on `user_message` breaks | Keep matching on `response_item` role user; if `user_message` is used anywhere, add `item_completed` `UserMessage`. |
| X6.2 | Line types: `session_meta`, `response_item`, `inter_agent_communication`(`_metadata`), `compacted`, `turn_context`, `token_usage_record`, `world_state`, `retained_context`, `security_risk_score`, `event_msg`, `realtime_item`; a new optional `ordinal`. Error, Warning, ExecCommandEnd and others are never persisted. Turn bounds: `turn_started`, `turn_complete`, `turn_aborted`. | `history/src/rollout_payload.rs:31-70`; `history/src/lib.rs:201-216`, `:350-357`; `rollout/src/policy.rs:115-160` | Z parser | degrades on new types | Ignore unknown types (A4). |
| X6.3 | `background_paginated_rollout_migration` (off by default, under development) rewrites Legacy rollouts in place. | `features/src/lib.rs:1179-1183`; `thread-store/src/local/rollout_migration.rs:1-9` | transcript reader | none today | – |

### X7. Skills discovery

| # | Edge case | Source | Contract | Impact | Smallest fix |
|---|---|---|---|---|---|
| X7.1 | Roots: every `.codex/skills` from cwd up (one per Project config layer), `$CODEX_HOME/skills` (deprecated), `~/.agents/skills`, `$CODEX_HOME/skills/.system`, **`/etc/codex/skills`** (Admin), **plugin skill roots named `plugin:skill`**, extra roots, and `.agents/skills` from the project root (marker default `.git`) down to cwd. | `ext/skills/src/host_roots.rs:48-170`; `config/src/loader/mod.rs:131-133`; `skills/src/lib.rs:63-67`; `loader/namespace.rs:11-24` | §2 Codex folders | degrades (Admin and plugin skills missing) | Add `/etc/codex/skills` and every ancestor `.codex/skills`. |
| X7.2 | A name clash keeps both skills (dedupe by SKILL.md path); order Repo, User, System, Admin, then name. | `loader/host_merge.rs:232-268` | §2 `matches` | degrades (duplicate rows) | Keep the first by that order. |
| X7.3 | Limits: name ≤ 64 chars, description ≤ 1024, scan depth 6, ≤ 2000 dirs and 20000 entries per root. `name` and `description` are required. | `loader/mod.rs:22-32`; `loader/discovery.rs:17`; `skills/src/parser.rs:13-84` | §2 | degrades (the app may list a skill Codex rejects) | Skip a SKILL.md with no `name` or `description`. |
| X7.4 | `[[skills.config]]` with `path` or `name` and `enabled = false` turns a skill off; `skills.bundled` too. | `config/src/skills_config.rs:20-46` | §2 | degrades | Read `config.toml` `skills.config`. |

## Antigravity CLI (AGY) 1.2.14

No release notes exist for 1.2.14. The local changelog `~/.gemini/antigravity-cli/cache/CHANGELOG.md`
(here **CL**) stops at `## 1.2.13`, and https://antigravity.google/docs/changelog lists CLI notes up to
1.2.12. So every row below holds "as of 1.2.13 or older". **W** is `https://antigravity.google/docs`.
**BI** is `~/.gemini/antigravity-cli/builtin/skills`.

### A1. Queueing and steering

| # | Edge case | Source | Contract | Impact | Smallest fix |
|---|---|---|---|---|---|
| A1.1 | Enter mid-turn **queues**; it does not steer. `/model <name> <prompt>` queued mid-turn now waits for its own turn (fixed 1.2.6). | CL:101 (1.2.6) | §1 row 1, §5 "Sent" | none | – |
| A1.2 | No CLI key to edit, delete or send-now a queued message is documented. The Send now / Edit / Delete queue buttons are in the desktop app, not the CLI. | W/changelog (hub entries "Queued messages…") | §5 `pullBack` nil | none (matches contract) | – |
| A1.3 | `Esc` has three jobs: with a suggestions menu open, the first `Esc` only closes it and a second interrupts (1.2.10); `Esc Esc` on an idle prompt clears the draft; `Esc` closes overlays. `Ctrl+C` always interrupts and cannot be remapped. | CL:38 (1.2.10); W/cli/using (Quick tips); W/cli/reference#default-keybindings; CL:684, CL:691 (1.0.11) | interrupt verb | **breaks** when a `/` or `@` menu is open (one Escape does not interrupt) | For AGY, interrupt with `C-c` once, not `Escape`. |

### A2. Slash command dispatch

| # | Edge case | Source | Contract | Impact | Smallest fix |
|---|---|---|---|---|---|
| A2.1 | Enter (`prompt.submit`) "Submits your prompt or active menu selection". A trailing `@path` or `/x` token can leave a menu open, and Enter then picks the item. A partial alias only fills the prompt; it must match exactly to run (1.1.11). | W/cli/reference#prompt-focus-keys; CL:454 (1.1.11); CL:377 (1.1.14) | ring; §3 `pickAndSend` | degrades (draft ending in `@x` is not sent) | Send the exact full command name; append a space when the draft ends inside an `@` token. |
| A2.2 | Aliases: `/clear`=`/new`, `/fork`=`/branch`, `/resume`=`/switch`=`/conversation`, `/rewind`=`/undo`, `/config`=`/settings`, `/usage`=`/quota`, `/exit`=`/quit`, `/plugin`=`/plugins`. The docs table is stale: `/fast` was removed and `/planning` became `/plan` (1.1.0); `/codesearch`, `/effort`, `/goal`, `/learn`, `/schedule`, `/browser` exist only in other pages. | W/cli/reference#core-slash-commands; CL:612 (1.1.0); W/slash-commands#command-catalog | §2 AGY built-in list | degrades | Build the AGY list from W/slash-commands, not from the reference table. |
| A2.3 | Many commands open an overlay that takes typed keys (`/resume`, bare `/model`, `/config`, `/agents`, `/mcp`, `/plugin`, `/skills`, `/hooks`, `/permissions`, `/keybindings`, `/rewind`, `/diff`, …). | W/cli/reference; W/cli/commands/* | §2 whole-draft send | degrades | Mark as "opens in the terminal". |
| A2.4 | Prefixes change meaning: `!` at start runs a shell command; `?` opens help; `@<subagent> <msg>` goes to a subagent conversation (1.2.9). These never become a main user row. | W/cli/using ("Terminal commands"); CL:206; CL:46 (1.2.9) | §5 "Sent" | degrades ("Sent" never clears) | Do not hold a "Sent" row for text starting `!`, `?`, or `@name `. |
| A2.5 | Stored user text can differ from typed text: skill commands lose the slash "at the serialization boundary" (1.0.13); stacked `/plan /grill-me …` is parsed (1.1.4); boundary whitespace is normalised (1.1.20). | CL:656, CL:552, CL:299 | §5 "Sent" equality | degrades | Trim, and for `/skill args` match on the argument part. Clear all "Sent" rows at turn end. |
| A2.6 | No source says which commands are blocked mid-turn, or that commands run only at message start. | – | §3 | unknown | – (see Could not verify) |

### A3. Session rotation

| # | Edge case | Source | Contract | Impact | Smallest fix |
|---|---|---|---|---|---|
| A3.1 | `/clear` (`/new`) starts a new conversation session. `/fork` (`/branch`) "allocates a new unique session ID" and switches to it. `/resume` loads another id. Switching agents in `/agents` forks. `/rewind` can revert or fork. | W/cli/using (Quick tips); W/cli/conversations ("Branching with /fork"); W/cli/commands/resume; W/cli/commands/agents; CL:103 (1.2.6), CL:71 (1.2.8) | §4 (AGY not covered), §5 "Sent" | **breaks** (the app keeps reading the old conversation; "Sent" never clears) | After any of these, re-resolve the id from `~/.gemini/antigravity-cli/cache/last_conversations.json` (workspace path → latest id, W/cli/commands/resume#under-the-hood-the-session-cache). Whether `/clear` and `/fork` update it is (unverified). |
| A3.2 | No `/compact` command; compaction is automatic and rewrites `transcript.jsonl` (1.1.13). | CL:402, CL:564, CL:64-65 | transcript reader | degrades (rows can vanish) | Match "Sent" only against rows newer than the send time. |

### A4. Hooks

| # | Edge case | Source | Contract | Impact | Smallest fix |
|---|---|---|---|---|---|
| A4.1 | No SessionStart, UserPromptSubmit or Notification hook. Events: `PreToolUse`, `PostToolUse`, `PreInvocation`, `PostInvocation`, `Stop`. Handlers are synchronous, 30 s default timeout. Every payload carries `conversationId` and `transcriptPath`. | BI/agy-customizations/docs/hooks.md:69-157; W/hooks | §4 child `agent.log` move | degrades (no clear signal) | A `PreInvocation` hook would report the live `conversationId`, but it edits user config; prefer A3.1's cache. |

### A5. Input box

| # | Edge case | Source | Contract | Impact | Smallest fix |
|---|---|---|---|---|---|
| A5.1 | **Trailing `\` + Enter inserts a newline**: "The CLI automatically removes the backslash and inserts a newline." | W/cli/prompting ("Shorthand newline insertions") | ring | **breaks** (a text ending in `\` is never sent; the next ring joins it) | Append a space when the text ends in `\`. (Claude has the same `\`+Enter rule, `interactive-mode#multiline-input`; Codex: see X5.) |
| A5.2 | Vim mode (`editorMode: "vim"`) starts in **NORMAL** unless `vimInsertFirst: true`. In INSERT, Enter inserts a newline and submit is `ctrl+enter`/`ctrl+s` (`vim.insert.submit`); `Esc` only goes to NORMAL. | W/cli/vim-editor-mode#switch-between-modes, #submit-your-prompt, #customize-the-submit-and-newline-keys; CL:440-441 (1.1.11) | ring, interrupt | **breaks** | Read `~/.gemini/antigravity-cli/settings.json`; if `editorMode` is `vim`, warn and do not drive the box. |
| A5.3 | `~/.gemini/antigravity-cli/keybindings.json` can remap or disable (`[]`) `prompt.submit`, `prompt.newline`, `cli.escape`, `navigation.up`, `navigation.tab`; a remapped Enter → newline is honoured (1.1.4). Only `cli.exit` and `cli.enter` cannot be disabled. `Ctrl+U` is not a documented prompt key. | W/cli/settings ("Keybindings configuration"); W/cli/using (Note under "Default keybindings"); W/cli/reference#default-keybindings; CL:557 (1.1.4) | ring, interrupt | **breaks** when remapped | Read the file at launch; warn when `enter` or `esc` lost their actions; interrupt with `C-c`. |
| A5.4 | Newline keys: Shift+Enter, `Ctrl+J`, `Alt+Enter`. An LF inside `send-keys -l` arrives as `Ctrl+J` (unverified for tmux). Long or multi-line input folds to `[Pasted text #X +Y lines]`; the full text is still sent. Non-bracketed multi-line paste is detected by a heuristic (1.1.14). | W/cli/prompting; CL:830, CL:731, CL:489, CL:453, CL:383 | ring of multi-line text | degrades (heuristic paste detection is undocumented in detail) | Live-test a 2-line ring once. |

### A6. Storage

| # | Edge case | Source | Contract | Impact | Smallest fix |
|---|---|---|---|---|---|
| A6.1 | Conversations are SQLite `.db` (+`-wal`), one file per conversation (1.2.2); schema and location are not documented. | CL:786 (1.0.4), CL:780, CL:148 (1.2.2) | AGY transcript | degrades on any release | – |
| A6.2 | A documented JSONL exists: `~/.gemini/antigravity-cli/brain/<conversationId>/.system_generated/logs/transcript.jsonl` (hook `transcriptPath`). ERROR steps were missing until 1.2.4; compaction rewrites it; the line schema is not documented. | W/hooks (common fields, `transcriptPath`); hooks.md:148-157; CL:128, CL:402 | AGY transcript | none today; an option | Consider it instead of the db. The headless `--output-format stream-json` is the only documented stable format. |

### A7. Skills and plugins

| # | Edge case | Source | Contract | Impact | Smallest fix |
|---|---|---|---|---|---|
| A7.1 | A marketplace plugin dir is a signed 64-bit hash (`-1870732469773246571`), "frequently not the same as `name` from plugin.json". The slash form is `/<plugin.json name>:<skill name>`. | BI/plugin/SKILL.md:70-76; CL:85 (1.2.7) | §2 AGY `plugin:skill` | **breaks** (wrong prefix → command not found) | Prefix from `plugin.json` `name`; fall back to the dir name (BI/agy-customizations/docs/plugins.md:39-40). |
| A7.2 | The slash name is frontmatter `name`, not the folder (`antigravity_guide` → `antigravity-guide`); `name` defaults to the folder. | BI/antigravity_guide/SKILL.md:2; W/skills#frontmatter-fields | §2 | degrades | Read frontmatter `name`. |
| A7.3 | `disable-slash-command: true` hides a skill from the menu **and** from `/name`. `user-invocable`, `disable-model-invocation`, `argument-hint` are not AGY keys. | CL:414 (1.1.12); BI/agy-customizations/docs/skills.md:47-54 | §2 | none (contract skips it) | – |
| A7.4 | Precedence high → low: workspace walk (cwd → folder holding `.git`) > workspace `skills.json`/`plugins.json` > `~/.gemini/config/` > built-in > global JSON configs; explicit paths win a clash (1.1.21); a plugin `name` clash keeps the first and drops the other silently. Worktrees and submodules resolve to the right root (1.1.12). | BI/agy-customizations/SKILL.md:46-74; CL:288; BI/plugin/SKILL.md:168-171, 229-234; CL:420 | §2 `matches`, folder walk | degrades | Dedupe with that order. |
| A7.5 | Other slash sources the app misses: legacy workflows `.agents/workflows/*.md` (and `_agents`, `.agent`, `_agent`), `~/.gemini/config/workflows/`, `global_workflows/` (retire **2026-11-01**); built-in skills under BI; `skills.json`/`plugins.json` `entries`; `/add-dir` directories. Disabled plugins (`config.json` `plugins.<dir>.enabled:false`, or `"disabled": true`) load nothing. | W/migration/workflows-to-skills; BI/migrate-workflows/SKILL.md:10-47; BI/agy-customizations/docs/json_configs.md:42-47; BI/agy-customizations/docs/plugins.md:72-96; CL:734, CL:25 | §2 AGY folders | degrades | Skip disabled plugins; add workflows until 2026-11-01. |
