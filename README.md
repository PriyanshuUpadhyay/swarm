# swarm

Message bus and pane control for a tree of agent CLIs. One agent, the chair, starts other agent
CLIs (Claude Code, Codex, or AGY) in visible terminal panes, sends them work, and collects their
answers. You see every worker and can type into any of them.

![Swarm.app: the chair on the left, a worker's pane on the right](docs/images/app-chat-worker.png)

## How it works

An example: you ask Claude Code in a tmux pane to "have a cheap model add a `version()` function".

1. The chair runs `swarm session new lane`. Swarm records a session and registers the chair's pane.
2. The chair runs `swarm launch version-worker code.small --cwd "$PWD"`. Swarm reads the
   `code.small` profile, takes its first runner that can run (for example AGY on a Gemini Flash
   model), splits a pane, and starts that CLI in it.
3. The chair runs `swarm send version-worker ask` with the task on stdin. Swarm stores the message
   in its SQLite inbox and types a short ring into the worker's pane.
4. The worker does the task and runs `swarm finish` with its answer. Swarm stores the answer and
   rings the chair's pane with `swarm: new message`.
5. The chair runs `swarm inbox`, reads the answer, runs `swarm ack`, and closes the worker with
   `swarm close version-worker`.

```mermaid
flowchart LR
  you([You]) --> chair[Chair pane<br/>Claude, Codex or AGY]
  chair -- "swarm launch" --> worker[Worker pane]
  chair -- "swarm send" --> db[(SQLite inbox)]
  db -- ring --> worker
  worker -- "swarm finish" --> db
  db -- "swarm: new message" --> chair
```

Swarm itself does no AI work. It owns the session, the panes, the messages, and the choice of
model. Each agent learns its part from a skill: `swarm-orchestrator` for the chair and
`swarm-voice` for a worker. A worker that exits without an answer is caught by `swarm sweep`, and
`swarm drain` writes a summary from its log.

Panes live in [tmux](https://github.com/tmux/tmux) (the default) or in
[Herdr](https://github.com/herdrdev/herdr). In a Herdr pane, swarm picks Herdr by itself.
Swarm.app is a macOS chat window over the same sessions.

## Install

One Homebrew cask installs Swarm.app, links the `swarm` CLI inside the app onto PATH, and installs
tmux, so the app and the CLI always come from one release. It needs macOS 26 (Tahoe) or newer:

```sh
brew tap priyanshuupadhyay/tap
brew trust --tap priyanshuupadhyay/tap   # Homebrew 6 or newer
brew install --cask priyanshuupadhyay/tap/swarm-app
swarm init                               # creates ~/.swarm with the db, runs/, and adapters/
```

Update with `brew upgrade priyanshuupadhyay/tap/swarm-app`. If you had the old `swarm` formula,
or a Swarm.app dragged from the DMG, run this once. The formula is now an empty pointer, so its
upgrade frees the PATH link. The cask reinstall links `swarm` to the app, and `--force` lets it
replace a dragged app:

```sh
brew update && brew upgrade priyanshuupadhyay/tap/swarm; brew reinstall --cask --force priyanshuupadhyay/tap/swarm-app
```

The app warns at launch when Terminal's `swarm` is another build than its own, and names the same
command. A `swarm` that brew did not install, such as `~/.cargo/bin/swarm`, you remove yourself.

The CLI alone, on an older macOS or on Linux: `SWARM_RELEASE_BUILD=1 cargo install --path .` from
a clone of a release tag, then `swarm init`. It needs tmux. Without `SWARM_RELEASE_BUILD=1` the
build is a dev build, which keeps its data in a branch folder and not in `~/.swarm`.

### Swarm app without Homebrew

Download `Swarm-<version>.dmg` from the
[latest release](https://github.com/PriyanshuUpadhyay/swarm/releases/latest), open it, and drag
Swarm onto Applications. The app carries its own `swarm` CLI but puts nothing on PATH. It still
needs tmux (`brew install tmux`) and at least one agent CLI (`claude`, `codex`, or `agy`).

The app is not notarized by Apple, so macOS blocks the first launch. Allow it once:

1. Open Swarm from Applications. macOS says it cannot verify the app. Click **Done**.
2. Open **System Settings > Privacy & Security** and scroll to **Security**.
3. Next to "Swarm was blocked", click **Open Anyway**, then confirm with your password.

On macOS 15 and later, a Control-click on the app and **Open** no longer skips this check.
If **Open Anyway** does not show, run `xattr -dr com.apple.quarantine /Applications/Swarm.app`.

## Set up

1. **Install an agent CLI and sign in.** Swarm starts [Claude Code](https://docs.claude.com/en/docs/claude-code),
   [Codex](https://github.com/openai/codex), and AGY. A runner whose CLI is not on `PATH` is skipped.
2. **Link the swarm skills.** The chair and the workers learn the protocol from `skills/`. Clone
   this repo and run the script from the `main` checkout. It links each skill into
   `~/.claude/skills`, `~/.agents/skills` (Codex), and `~/.gemini/config/skills` (AGY):

   ```sh
   git clone https://github.com/PriyanshuUpadhyay/swarm ~/swarm
   sh ~/swarm/scripts/install.sh
   ```

3. **Check your profiles.** A profile maps a role, such as `code.small` or `review.deep`, to an
   ordered list of runners (provider, model, effort). Swarm ships defaults in
   `default-profiles.json`. Edit them on the app's Home, or read them with
   `swarm roles check --json`. [Profiles](#profiles-and-runners) has the details.
4. **Approve the setup plan.** On first launch the app asks "Let swarm set up this Mac?" and
   shows each file it would change, under its group: the Codex and AGY hooks, so worker columns
   show their chat and state, and folder trust, so a seat in a git repo or a swarm scratch folder
   starts with no trust dialog. Clear a group's box to leave it as it is, and pick under folder
   trust whether swarm trusts each folder that passes the safety check or asks in the agent's
   column. Click **Approve and apply**. Until you approve folder trust, a seat
   in a new folder stops at its CLI's own trust question, and the chair's launch prints
   `trust-pending` with the diff. A chat's own folder counts as approved, because you picked it,
   and the chat says so. You can do it later from **Swarm > Set Up Swarm…**, or in a
   terminal with `swarm setup --plan` and the `swarm setup --digest …` it prints.
   **Swarm > Managed Changes…** lists each entry swarm added and removes one after you see the
   diff, as `swarm managed revert` does.

### Optional: accounts with yelo

[yelo](https://github.com/PriyanshuUpadhyay/yelo) keeps more than one Claude or Codex account
(profiles) and reads how much usage each one has left. Swarm uses it when it is on `PATH`:

- `swarm launch --account auto` starts the worker on the account with the most usage left, the
  same answer as `yelo profile pick`. `--account work` takes the account named `work`. Without
  `--account`, yelo's `claude` or `codex` command in the pane picks the account.
- A runner whose best account has less than `min_usage_left_pct` (default 5%) left is skipped, so
  the launch falls through to the next runner.
- `swarm accounts --provider claude --json` and `swarm usage --json` show what yelo reports, and
  the app shows it on the account row of Switch model.

```sh
brew install priyanshuupadhyay/tap/yelo
yelo setup
yelo profile create --cli claude work
claude --profile work auth login
```

Without yelo, each CLI uses its own login. AGY has no account source yet.

### Optional: workflow skills from agent-kit

[agent-kit](https://github.com/PriyanshuUpadhyay/agent-kit) has workflow skills that run their
workers as visible swarm panes:

| Skill | What it does with swarm |
|---|---|
| [`council`](https://github.com/PriyanshuUpadhyay/agent-kit/tree/main/skills/council) | Claude, GPT, and Gemini voices debate in panes and end in one verdict |
| [`web-search`](https://github.com/PriyanshuUpadhyay/agent-kit/tree/main/skills/web-search) | Three seats search different parts of the web; the chair merges |
| [`research`](https://github.com/PriyanshuUpadhyay/agent-kit/tree/main/skills/research) | Answers one question in a short report; can send the web part to a `search.web` seat |
| [`review-check`](https://github.com/PriyanshuUpadhyay/agent-kit/tree/main/skills/review-check) | Seats judge each changed unit of a diff; a script gives the verdict |
| [`orchestrate-claude`](https://github.com/PriyanshuUpadhyay/agent-kit/tree/main/skills/orchestrate-claude), [`-codex`](https://github.com/PriyanshuUpadhyay/agent-kit/tree/main/skills/orchestrate-codex), [`-agy`](https://github.com/PriyanshuUpadhyay/agent-kit/tree/main/skills/orchestrate-agy) | Bind a skill's worker needs to each agent CLI |

The roles these skills ask for (`council.gpt`, `search.web`, `review.deep`, and others) are the
profile names in `default-profiles.json`. The
[agent-kit README](https://github.com/PriyanshuUpadhyay/agent-kit#install) shows how to link the skills.

## Use it

### From a terminal

Start your agent CLI inside tmux, or in a Herdr pane, and ask it to use swarm:

```sh
tmux new -s work
claude
> Use swarm to have a code.small worker add a version() function to src/health.ts.
```

The chair follows `swarm-orchestrator`, and the worker pane opens beside it.

### From the app

1. Click **Import Project…** and choose a folder. Swarm adds the project and starts a chat in it
   with the `chat` profile. A folder that is not a Git repository gets an offer to run `git init`.
   The **+** on a project's header makes a workspace (a branch in its own worktree) there.
2. To use another model, click **Switch model** in the chat.
3. Type your task. When the chair launches a worker, the worker's pane opens as a column to the
   right of the chat, and you can type into it.

![Swarm.app Home: each profile and the runner a launch would take](docs/images/app-profiles.png)

## Swarm app reference

Swarm.app opens on Home, where routed roles show their models. Import Project adds a folder to the
project list and starts a chat in it; a folder that is not a Git repository first gets an offer to run
`git init`, and "Keep as Folder" adds it as a plain project. Create Project makes a folder, runs `git init`
in it, adds it there, and starts a chat in it.
New chat starts the chat profile at once with no sheet (ADR 0035): a "New chat" tab shows at once and becomes the
chat when the chair is up, or shows the launch error with Retry. To use another model, start a chat and use Switch
model. Codex and AGY list CLI models; Claude lists aliases and accepts a full model name in Other model. In a chat,
Switch model asks the live chair
for a compact summary, starts the chosen Claude or Codex model, and keeps both parts in one chat
tab. If the old pane has closed, the new chair receives recent messages and makes its own compact
summary.

The sidebar shows one header per project, with its workspaces under it in last-activity order, and
Pinned workspaces in their own section at the top (ADR 0037). A header collapses with its chevron and
then shows its most urgent status. The **+** at the top makes or imports a project (⇧⌘N). A project's
**+** makes a workspace there, and ⌘N makes one in the current project: a branch from the default branch in a worktree beside the
project, then a chat in it. In a repository with no commit yet, the branch starts empty (an orphan
branch). A plain-folder project offers `git init` first. A workspace row's **+** adds a chat in it
(⌘T). Empty task worktrees stay in the sidebar with 0 chats. The command palette (⌘K) finds
workspace names, projects, branches, and chat titles. Chats in the selected workspace appear as
underlined tabs above the transcript. The plus button starts another chat in that workspace.
Pins, workspace names, and the last selected chat survive restarts. Archive workspace hides its
row without deleting files, archiving chats, or stopping agents; Archived offers Restore workspace.
Herdr-hosted agents with live panes attach through Herdr's direct terminal stream, so the pane
accepts input in Swarm. A closed connection can be reopened with Reconnect.

## Environment

When `SWARM_HOME` is not set, only a release build uses `HOME`. release.yml marks it with
`SWARM_RELEASE_BUILD=1`. Every other build is a dev build and uses `~/.swarm-<branch>`, `main` too
(`~/.swarm-main`), so a dev build never touches the real data (ADR 0027, amended in ADR 0048). A
dev build from a detached HEAD uses `~/.swarm-head+4b9253d8ff1ee183`. A branch name with a character outside `[a-z0-9._-]`, one that starts with `.` or
`-`, or one over 200 bytes gets its letters made lowercase and its other unsafe bytes made `-`, is cut
to 200 bytes, and gets a hash of the whole name added, so `feat/login` uses
`~/.swarm-feat-login+407712bf7898fb7f` and never meets `feat-login`. An uppercase letter also takes
the hash, because the default macOS disk ignores case, so `Feature` and `feature` get two folders. `branch_folder` in `src/paths.rs`
states the exact rule.
`swarm --version` prints the branch after the commit, nothing for a release build and `HEAD` for
a detached one. An explicit `SWARM_HOME` always wins, and an empty one is an error. The data
directory is always `$SWARM_HOME/.swarm`.

Swarm writes there only when the folder is its own (ADR 0036). A missing or empty folder gets the
marker file `.swarm/swarm-home` before any other file. A folder from an older swarm, with swarm's
`swarm.db` and no marker, gets the marker. Any other folder is refused with no write, and the
message names the folder and `SWARM_HOME`.

| Variable | Meaning |
|---|---|
| `SWARM_HOME` | Parent of the `.swarm/` data directory. Defaults to `$HOME` for a release build, or `~/.swarm-<branch>` for a dev build. |
| `SWARM_ADAPTER` | Adapter file name under `.swarm/adapters/`. Defaults to `herdr` in a Herdr pane, else `tmux`. |
| `AGENT_ROUTING_CONFIG` | The old routing file that the first read imports. A path that does not exist is an error. |
| `SWARM_SESSION_ID` | Session the caller belongs to. `spawn` stamps it into each child pane. |
| `SWARM_AGENT_ID` | Identity of the caller. `spawn` stamps it into each child pane. |
| `SWARM_SUMMARIZER` | Shell line `drain` runs with a log on stdin. Required by `drain` only. |

## Profiles and runners

Agent profiles live in `$SWARM_HOME/.swarm/profiles.json` (ADR 0031). A profile is one role, such
as `chat` or `code.complex`, with an ordered list of runners. A runner is a provider, a model, an
effort, and the flags its provider takes. `chat` is always first. With no file, the first read
imports the old routing file (`$AGENT_ROUTING_CONFIG`, else
`$XDG_CONFIG_HOME/agent-routing/roles.json`, else `~/.config/agent-routing/roles.json`) and writes
`profiles.json`. Each route becomes a profile whose runners are the route's runners, then their
substitutes. The old file is never written, and a later edit to it only prints a warning. With
neither file, the `default-profiles.json` built into the binary is used and nothing is written until
the first save. A file that is a broken symlink is an error, never a silent switch to the default.
A save writes through a symlink, so `profiles.json` can live in a dotfiles checkout.

A launch takes the first runner that can run (ADR 0032). It skips a runner whose CLI is not on
PATH, whose accounts are all signed out, or whose best account has less usage left than
`min_usage_left_pct` (default 5). The account read has a 2 s deadline; a read that fails, times
out, or finds no accounts counts as "can run". Each skip is one stderr line, such as
`swarm: code.complex: skipped claude/opus/high: usage 2% left (threshold 5%)`, and a launch where
no runner can run fails with one line per runner.

## Commands

Caller `any` needs no identity. `session` needs a session, and `agent` needs a session and an agent
id. A chair gets both from its pane, which `swarm session new` registers. A child gets
`SWARM_SESSION_ID` and `SWARM_AGENT_ID`, which `launch` and `spawn` stamp into its pane. The
`orchestrator` caller also must match the session's orchestrator agent.

| Command | Caller | Effect and output |
|---|---|---|
| `--version` | any | Print the package version and build commit. |
| `init` | any | Claim `.swarm/` (ADR 0036), then create `runs/`, `adapters/`, and the database. Shipped adapters stay in the binary; matching old disk copies are removed. |
| `setup status --json` | any | Print whether each setup group has nothing pending: `hooks` (swarm's Codex and AGY state hooks), `guard` (as for `hooks status`), `trust` (the owner's answer, `standing` or `ask`, for launch folder trust in `~/.swarm/consent.json`), and `herdr` (always true until a Herdr writer ships). More groups may come (ADR 0043). |
| `setup [--plan [--json] \| --digest <digest>] [--cwd <dir>] [--only <group>,...] [--consent <standing\|ask>] [--resume]` | any | One plan of every write swarm makes outside its home, with one digest: the `hooks` group as `hooks setup` writes it, then the `trust` group, the consent file (set to `--consent`, by default the recorded answer, else `standing`, as a managed edit whose undo means `ask`) and each trust entry a seat launched in `--cwd` (default the current folder) would need, or with `--resume` a resuming seat, whose Claude runs in that folder itself. `--plan` prints a unified diff per file, each conflict, and each skipped part with its reason, and with `--json` gives each file and conflict its `group`. Apply writes each file in plan order and records each trust entry and the consent (ADR 0042); with nothing pending it prints `already set up` and exits 0 (ADR 0043). |
| `hooks status --json` | any | Print whether swarm's own Codex and AGY state hooks are set up, and as `guard` whether the guard registrations match the rule list: all in place while `~/.swarm/guards.json` exists, none left once it is gone (ADR 0040). |
| `hooks setup [--plan [--json] \| --digest <digest>]` | any | Trust swarm's Codex hooks in `~/.codex` and each `~/.codex-<name>`, and add the `swarm` group to AGY's `hooks.json`. With a rule list at `~/.swarm/guards.json`, also register `swarm guard` in each Claude `settings.json`, each Codex `hooks.json` with its trust, and AGY's `swarm-guard` group (ADR 0040). An entry that swarm needs where the owner already has another one, or a file that swarm cannot read or edit, is a conflict: setup names it with its fix, writes no file, and exits 1. `--plan` prints a unified diff per file and each conflict, writes nothing, and exits 0. With `--json` it also prints the `digest` that `--digest` checks, so apply refuses a file that changed after the plan (ADR 0036). Each item it adds is recorded, so `managed list` shows it and `managed revert` removes it (ADR 0042). |
| `managed list [--json]` | any | Print each item swarm wrote outside its home (a TOML key, a JSON key, or a JSON array item) with its live state: `present` (equals what swarm wrote), `changed` (with the value `found` now), `unreadable` (swarm cannot read the file or the path in it, with the reason as `error`; revert refuses it), `gone`, or `off` (swarm removed it). An item with no record that equals swarm's current hook text is listed with `recorded: false` (ADR 0042). |
| `managed revert (<id>... \| --all) [--plan [--json] \| --digest <digest>]` | any but a child | Remove each named item, or each `present` one with `--all`, and only while it equals what swarm wrote; a value swarm replaced is set back. A Codex guard trust key goes with its `hooks.json` group. An item with another value is a conflict with its fix, and no file is written. `--plan` and `--digest` work as for `hooks setup`. A child caller is refused, because a revert can remove the guard that blocks its own calls; its `--plan` still prints (ADR 0042). |
| `guard <claude\|codex\|agy> PreToolUse` | a CLI hook | Read the CLI's hook payload on stdin, run each matching rule of `~/.swarm/guards.json` (`SWARM_GUARDS` overrides the path), and print that CLI's allow or deny. A guard rule that crashes, times out, or does not start, a bad list, or a missing list blocks the call (ADR 0040). |
| `adapter check <name>` | any | Load the shipped adapter plus any disk overrides. Print the verbs that the disk file overrides. |
| `session new <lane\|relay\|open> [--chair <claude\|codex>:<id>]` | any | Create a session for the physical current directory (`pwd -P`) and print its UUID v7 id. Without `--chair`, use a chair id from the current CLI environment when present. |
| `session chair <claude\|codex>:<id>` | orchestrator | Set the chair transcript id. Refuse any other agent. |
| `session continue <new_id> <old_id>` | any | Link a newer session to an older session in the same directory after the new chair receives its context. |
| `session archive <id>...` | any | Archive one or more UUID v7 sessions. |
| `session unarchive <id>...` | any | Restore one or more archived UUID v7 sessions. |
| `sessions --json` | any | List active sessions and resolved chair logs as JSON. |
| `sessions --json --archived` | any | List all active sessions, then the newest 50 archived sessions in archive order, with `archivedAt` in Unix seconds. |
| `host-context --provider <claude\|codex\|agy>` | any | Print the session's host contract in that provider's hook format, or nothing outside a visible host. |
| `hook <claude\|codex\|agy> [event]` | any | Read a provider hook's JSON on stdin and record the agent's state (`working`, `waiting`, `done`, `failed`) for `SWARM_AGENT_ID`. A Claude or Codex `Stop` also stores the model and token total. Claude reads at most 8 MiB per Stop and adds new message usage; Codex copies its cumulative total. A cost is stored only from a complete provider record. Does nothing outside a swarm agent. Always prints `{}` and exits 0. AGY sends no event name, so its hook passes it, as in `swarm hook agy Stop`. |
| `notify <title> [--body <text>]` | orchestrator or owner | Show the owner one notice through the adapter's `notify` verb (osascript on every shipped adapter, ADR 0045), within 5 s. Needs no session. Refuse a worker, and exit 1 when the adapter has no `notify` verb. `hook` and `agents --json` also send one notice when an agent's state changes to `waiting` (ADR 0044). |
| `herdr-split` | any | Split a child pane right of `HERDR_PANE_ID`, stack it under earlier children at equal height, and print its id. The herdr adapter's spawn verb. |
| `roles --json` | any | Print every profile, the file's `revision`, and `imported` when the file came from the old routing file. Reads no usage. |
| `roles check --json` | any | Print, for each profile, the runner a launch would take now (`pick`) and each skipped runner with its `code` and `text`. |
| `roles get <role> [--provider <claude\|codex\|agy>]` | any | Print the runner a launch would take as JSON: its fields plus `role`, `runnerId` (`<role>#<n>`), `fallbackRunnerIds`, `skipped`, and `substitutedFor` when a later runner was taken. With `--provider`, that provider's runners are tried first, and a profile with no runner of that provider is refused. |
| `roles save --revision <revision> <profile-json>` | any | Replace one profile, as `{"name", "runners"}`, and print the new revision. Fails when the file changed after `revision` was read. |
| `providers --json` | any | List each provider with its efforts, default effort, whether it has accounts, and the flags a runner of it takes. |
| `models --provider <claude\|codex\|agy> --json` | any | List models for a provider. Codex and AGY use their CLI catalogs, and a Codex model lists its efforts; Claude shows its model aliases. |
| `accounts --provider <claude\|codex\|agy> --json` | any | List accounts for one provider as JSON. |
| `usage --json` | any | List account use meters as JSON. |
| `drain` | any | Run queued summarize jobs, print `done`, `retry`, or `parked` per job. |
| `agent add <id> <role>` | session | Register an agent. The `orchestrator` role also records the caller pane and session adapter. |
| `agents --json` | session | List agents, pane state, agent state, and adapter attach support as JSON. Each row includes `profile`, `runner`, `model`, `effort`, `account`, `costUsd`, and `tokens` when known. Missing values are omitted. For each live agent it reads the pane's bottom rows (the adapter's `screen` verb) and records `working`, `waiting`, or `done` when the screen shows it and no hook reported in the last 10 s. It re-rings a due message to an idle pane and, while the adapter lists the chair's pane and it reads idle with no question, sends the sweep's reports. None of its rings waits for proof: the next listing or sweep settles it from the hook state and the screen (ADR 0041). |
| `agents --json --all` | any | Print one JSON object that maps a session id to its `agents --json` listing, read with that session's stored adapter. It has a key only for a non-archived session that has agents and a stored adapter and whose read worked; a failed read prints an error on stderr. Adapter work shares an 18 s batch budget, divided across the remaining sessions. A session that exceeds its share is omitted with an error on stderr. The app's sidebar uses it once per refresh. |
| `messages --json [--after <seq>]` | session | List message metadata and available bodies as JSON. `delivery` is what the last ring proved: `hook`, `screen`, `seen` (the agent read its messages after the ring), `unconfirmed`, `unchecked`, or null until the ring's proof is settled; more values may come (ADR 0041). |
| `launch <id> <role> [--provider <claude\|codex\|agy>] [--model <name> for chat] [--account <auto\|name>] [--cwd <dir>] [-- <args>...]` | session | Start the first runner of the role's profile that can run, or, with `chat --provider <provider> --model <name>`, exactly that model once with the chat profile's effort for that provider and no fallback (ADR 0033). `--account` is ignored for a provider with no accounts, and without `--model` a `--provider` the profile has no runner of is refused. Register the agent, split a pane in `--cwd`, and start its provider CLI. A child caller is refused. A Claude child runs from `<cwd>/.herdr/workers`. The pane dir is marked trusted for Claude, Codex, and AGY only with standing consent from `swarm setup`, or for a chair, whose folder the owner picked; each write is recorded and printed as a bare `trusted <provider> <dir>` line on stderr. With no consent, launch writes nothing and prints a bare `trust-pending <provider> <dir>` line, the diff, and the `swarm setup --plan --cwd` command that approves it; the pane shows its CLI's own trust question (ADR 0043). |
| `spawn <id> <role> [--provider <p>] [--account <auto\|name>] [-- <cmd>...]` | session | Register the agent, split a pane, and optionally run `<cmd>; swarm exited`. Print the pane id. |
| `type <id>` | session | Read text from stdin and type it into a live agent pane. A closed pane causes an error before any input is sent. |
| `interrupt <id>` | session | Send the adapter interrupt action to the agent pane. |
| `attach <id>` | session | Attach to the agent pane when the adapter supports it. |
| `close <id>` | session | Close the pane of `<id>` and forget it. |
| `send <recipient> <kind>` | agent | Store stdin as a message, ring the recipient, print the seq. The ring waits, up to 30 s, for the recipient's turn-start hook or a working screen, and presses Enter again while a Claude or Codex input box still holds it. With no proof it warns on stderr and still exits 0 (ADR 0041). |
| `finish` | agent | Send stdin as a `summary` to the orchestrator, print the seq. |
| `exited` | agent | Capture the own pane to `runs/<session>/<id>.log`, report a missing summary. |
| `sweep [--every <secs>]` | agent | Report each child whose pane is gone, print `dead <id>`. Re-ring an unseen message at most once after 60 s, for a child or the caller itself, then wait for an ack. When that ring starts no turn, the chair gets `unconfirmed:<seq>` from the agent and the sweep prints `unconfirmed <id> <seq>`; for the chair's own message it prints only the line. It sends the chair `stall:unacked:<seq>` for a child at done with a read, unacked message, and `stall:silent:<seq>` for a child at done that sent nothing after a proven ring, once each, and prints `stall <id> unacked\|silent <seq>`. It sends these reports only while the chair's screen reads idle with no question, so the ring's Enter cannot answer one; a later pass sends the rest. `agents --json` sends the same messages one pass later, because its rings do not wait (ADR 0041). With `--every`, repeat every N seconds and warn instead of exit on a failed pass. |
| `inbox` | agent | Print `seq sender kind body_path` per unread message. |
| `ack <seq>` | agent | Mark one message read. |

The Herdr adapter's spawn verb runs `swarm herdr-split` through `$SWARM_EXE`, the path of the binary
that runs the verb, so Swarm.app finds it even with a short `PATH`. A disk adapter's spawn verb must pass
`SWARM_HOME` and `SWARM_SESSION_ID` to the pane, because the Codex and AGY state hooks find the
pane's own swarm through them (ADR 0034). Claude, Codex, and AGY run
`swarm host-context --provider <claude|codex|agy>` as a SessionStart hook to learn the host contract.

## Skills and demos

`skills/swarm-voice` is for a child that `swarm launch` or `swarm spawn` started, and
`skills/swarm-orchestrator` is for the parent. Inside the repo, Claude Code finds them through
`.claude/skills` and AGY through `.agents/skills`, both links to `skills/`; Codex reads `AGENTS.md`.
`sh scripts/install.sh` links them into every agent CLI on the machine. `demo/herdr.sh` and
`demo/tmux.sh` each run one live voice on that host: `cargo install --path .` then
`VOICE=claude|codex|agy sh demo/herdr.sh` from a Herdr pane, or `sh demo/tmux.sh` from inside tmux.

## Walkthrough

Run this in a tmux pane, so `spawn` has a pane to split. `my-agent` stands for any command.

```sh
swarm session new lane   # lane: children talk only to the orchestrator; registers this pane
swarm spawn coder coder -- my-agent --task "write the parser"   # prints the pane id, e.g. %3

# in the coder pane, SWARM_SESSION_ID and SWARM_AGENT_ID=coder are already set
echo "parser done, tests green" | swarm finish        # prints the seq, e.g. 0

# back in the orchestrator pane
swarm inbox                                            # 0 coder summary runs/<session-id>/0.txt
swarm ack 0
swarm close coder

# a child that exits without finish gets a fallback summary and a summarize job
SWARM_SUMMARIZER='head -c 200' swarm drain            # done 1
```

## Performance profiling

Turn on **Debug > Performance Logging** in Swarm.app. Then open a slow chat or start a new one.
In Terminal, watch the stage times in milliseconds:

```sh
/usr/bin/log stream --style compact --level info --predicate 'subsystem == "io.github.priyanshuupadhyay.swarm" AND category == "performance"'
```

For a saved trace, use `/usr/bin/log show --last 10m --style compact --info` with the same predicate.
The logs show stage names, times, and item counts. They do not include chat text, paths, or
command arguments. Turn off the menu switch when done. Set `SWARM_PERF=1` to enable the same
timings for a command-line run. In Instruments, use Time Profiler for CPU work and Points of
Interest for the stage intervals. `AppStarted` and `WindowReady` mark startup.
`ChatSelected`, `ChatDetailAppeared`, and `ChatRowsShown` mark the visible chat-open path;
`InitialRefresh`, `WorkspaceTree`, and `TranscriptPoll` show where the time goes before that.

