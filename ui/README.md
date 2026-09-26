# Swarm UI

Chat messages render headings, lists, quotes, code blocks, and simple tables with native views.
Complex table syntax retains its source layout. Message, code, and table copy actions keep the
source text. Links open only HTTP(S) destinations; message images do not load remote content.

Tool cards join each call with its recorded output and diffs by identity within the same turn.
Interleaved calls keep separate cards. Ambiguous or missing call identities leave results visible
as separate rows. Cards show Waiting for result, Finished, Failed, Interrupted, or No result;
Finished describes a tool result and does not claim that tests passed. Failed cards open by
default. Commands and source input remain available, and file paths can be revealed in Finder.
Tool output initially shows up to 120 lines and 12,000 characters with an explicit notice; Show
full output and Copy output retain all saved text. Search opens matching cards and full content.

Saved Claude Edit results with structured patches show a Changes row in the chat. Expand it
to see the patch with the existing diff viewer. When its call is present, the diff and original
tool result are inside that tool's card. Both unified and split layouts are available.
Previews show up to 200 patch lines and 400 characters per line, with an explicit notice when
content is omitted. Full patch data and paths remain in the translated event; only display labels
use short filenames. Show full patch, Copy patch, and search matches use all saved patch lines.
Failed edits have no successful-change preview. Malformed or ambiguous
patches retain their tool result and appear as unknown data for inspection. Saved Codex `exec`
edit calls and AGY edit results retain their inputs and result text. AGY's saved textual patches
do not yet produce a Changes row; some saved results are truncated. The checked Codex logs contain
wrapped edit requests rather than applied-patch events. Neither format has a structured diff
preview yet. Saved fixtures cover both existing text paths.

Known Claude, Codex, and AGY chair log paths can open in the chat UI. Automatic log discovery
still supports Claude and Codex; AGY needs a log path supplied by the session.

Swarm is a macOS chat app for the swarm bus. The sidebar lists workspaces across projects in
Pinned and My workspaces. Selecting a workspace opens its remembered chat, with its other chats
in the top tab strip. Empty workspaces offer New chat.

Workspace rows use the saved name or `project / workspace-folder`, with chat counts below.
Saved names show project and branch details; default names show a branch when it differs from
the folder. Duplicate names show a short distinct path, and hovering reveals the full path.
Names stay the same when chats or branches change. The green bar means an agent process is alive.

Workspace names, pins, archive state, and selection survive restarts. Archive workspace hides its
row; it keeps the files, chats, and running agents, and Archived offers Restore workspace. Agents
stay in the selected chat detail. ADR 0014 sets its scope to projects, worktrees, sessions, a chair
transcript, and a live pane.

File suggestions load when you type `@`, rather than when a chat opens. They search project
folders but do not scan the home folder or its ancestors. This prevents background suggestions
from entering personal libraries. Selecting the Workspaces icon returns to the current chat,
including when a file or diff preview was open.

The composer shows the model reported by the current chat. Choose it to search models and
switch providers. The picker keeps the current model selected and remembers model choices for
each provider. Reasoning effort is fixed at Medium for new agents and is shown in the picker.
Account selection is available below the model list.

Switching starts a new agent with a summary or recent messages, not the full conversation.
The picker reports each step. Cancel stops the switch before the new agent starts; the current
agent may still finish its summary. After launch, cancellation is disabled until the operation
finishes. The chats are linked only after the new agent receives the context.

One sidebar switches between Workspaces, Files, Changes, PR, and Usage. The toolbar menu moves
it left or right; ⌘B hides or shows it, and ⌥⌘I opens Changes. Drag its inner edge to resize it.
Its side, width, and selected view survive restarts. The composer usage line selects Usage.
Reads run when a view opens or Refresh is pressed; they do not fetch or change Git.

Files lists folders on demand, includes hidden files, and omits `.git`. File previews are read-only,
limited to 256 KiB, and do not follow symbolic links. Each folder lists at most 2,000 entries and
reports when it reaches that limit. Files and diffs open in the main area; the Chat tab returns to
the same chat and draft. There is one replaceable file preview tab.

Diffs use bundled diff2html 3.4.55 in an offline WebKit view, with unified and split layouts,
line numbers, syntax colors, and linked scrolling. Source and licenses are in
`Sources/Swarm/Resources/DiffViewer`. The app needs no CDN or network connection to render a diff.

Changes separates staged, unstaged, untracked, and conflicted files. Branch compares a chosen
local ref's common base with the displayed HEAD snapshot. PR uses `gh` for github.com and checks
the full source repository and branch, including forks. Its diff is GitHub's version; the view
marks a local HEAD that differs. SSH aliases and other GitHub hosts are not supported yet.

Usage shows the latest provider report for the current agent session, not linked earlier agents.
Codex context includes the last call's input and output; cached input is already in input.
Claude context includes input plus cache read and write. Claude logs do not supply a reliable
capacity, so their percentage stays unavailable. Cost is a provider estimate in USD; missing or
partial reports are marked. Account limits stay separate. Compaction clears context until a new
report. Opening a chat reads the last 100 source log lines from its newest session.
Older pages load when you scroll up, or select **Load earlier messages**. Earlier linked sessions
load only after the newer session reaches its start. Loaded history and live updates stay in memory
until you leave the chat. Search covers loaded messages. If no usage report is loaded, Usage says so. Draft text is not included in these counts.

The `Swarm` target owns views and focus.
`SwarmCore` owns bus calls, rules, and process work.
`TranscriptTool` reads the Zig transcript stream.

Run `make build` to compile the Swift targets.
Run `make test` to build the transcript tool and run tests.
Run `make lint` to check the source boundaries.
Run `make app` to build `.build/release/Swarm.app`.
Run `make install` to put it in `~/Applications` and keep the old app.
Run `make run` to build and launch the app.
For a development build, set `SWARM_HOME=~/.swarm-<branch>` to keep its data apart.
