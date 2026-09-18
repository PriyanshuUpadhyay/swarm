# Agent bus contract for the UI

Swarm starts swarm agents, chats with them, and shows each one's live pane (ADR 0003). Swarm is the
chair of every agent it starts (ADR 0007), and swarm builds the command line for a role (ADR 0008).
Swarm reads only the JSON and commands below, through the Swift types in
`ui/Sources/SwarmCore/Agent/SwarmBus.swift`. A change to a shape changes this file, the Rust output,
and the Swift types in one commit. The rules of `docs/profiles-contract.md` apply here too: one JSON
object on stdout and exit 0, or one stderr line and a non-zero exit; `snake_case` keys; `null`,
never a missing key.

## How Swarm calls swarm

Workspace agent panes use `SWARM_ADAPTER=tmux-solo`. An app chat uses `SWARM_ADAPTER=tmux`, and a
reader uses the adapter stored on the discovered session. Every call after `session new` also sets
`SWARM_SESSION_ID` and `SWARM_AGENT_ID=orchestrator`. A call that starts panes (`launch`) runs with
its working directory, and `PWD`, set to the workspace's worktree.

Each app chat has one swarm session. Swarm creates it in the chat's workspace with `swarm init` and
`swarm session new lane --chair <claude|codex>:<id>`. The lead CLI starts in that workspace's tmux
pane with the session variables and runs `swarm agent add orchestrator orchestrator` there, so the
adapter records its pane. Codex reports its thread id after it starts, so Swarm then runs `swarm
session chair codex:<id>`. Both chair commands accept ids that match `^[A-Za-z0-9-]{1,64}$`; an
invalid id records no chair and is not an error. `session chair` uses `SWARM_SESSION_ID` and prints
nothing.

Without `--chair`, `session new` records `claude:$CLAUDE_CODE_SESSION_ID` when that variable is
set, else `codex:$CODEX_THREAD_ID` when that variable is set, else no chair. While a session has an
agent with a pane, Swarm runs `swarm sweep` every 30 seconds, which re-rings an unread ask and
reports an agent that died.

The agent CLI answers only if the `swarm-voice` skill is installed for it
(`scripts/install-skills.sh`). This contract does not check that.

Example. The user starts `coder-1` with role `code.complex` and types "Fix the parser". Swarm runs
`swarm launch coder-1 code.complex --account auto`, then `swarm send coder-1 ask` with that text
on stdin. The agent replies with `swarm finish`, and the reply is the next `summary` that
`swarm messages --json` returns.

## Adapter `tmux-solo`

`adapters/tmux-solo.conf`, installed by `swarm init` like the other shipped adapters. Each agent
runs in its own detached tmux session on the private socket `swarm` (`tmux -L swarm`), so a UI can
attach to one agent's pane alone.

| Verb | Behaviour |
|---|---|
| `self` | Prints nothing. The chair has no pane, so nothing rings it. |
| `spawn` | Creates session `swarm_<session_id>_<agent_id>` in `$PWD` with the four `SWARM_*` variables, hides the tmux status line, sets the prefix key to `None`, and prints the pane id. The user's `~/.tmux.conf` is not loaded. |
| `ring` | Types the text into the pane, waits 0.5 seconds, and presses Enter, as `tmux.conf` does. |
| `list` | Prints every pane id on the socket, and prints nothing (exit 0) when no tmux server runs there. |
| `close` | Kills the session that holds the pane. |
| `capture` | Prints the pane's whole scrollback. |
| `attach` | Attaches the terminal to the session that holds the pane. |

`attach` is a new verb that every adapter may declare and none must. `tmux.conf` and `herdr.conf`
do not declare it.

`interrupt` is an optional verb every shipped adapter declares. It presses Escape in `$SWARM_PANE`:
`herdr pane send-keys "$SWARM_PANE" esc` for `herdr`, `tmux send-keys -t "$SWARM_PANE" Escape` for
`tmux`, and the same with `-L swarm` for `tmux-solo`.

## `swarm type <agent_id>`

Caller `session`. Reads the text on stdin and types it into the agent's pane with the adapter's
`ring` verb (`SWARM_PANE` is the agent's pane, `SWARM_TEXT` is the text), so the text is submitted
with Enter the way a ring is. It prints nothing and exits 0. It fails before it runs anything with
`swarm: no pane recorded` or `swarm: empty text` (stdin holds only whitespace). It writes no message
row: typed text is input to the pane, not a bus message.

## `swarm interrupt <agent_id>`

Caller `session`. Runs the adapter's `interrupt` verb for the agent's pane, prints nothing, and
exits 0. It fails before it runs anything with `swarm: adapter <name> has no interrupt` or
`swarm: no pane recorded`.

Example. Swarm shows session 10, whose chair is a Claude Code chat in Herdr pane `wBC:p2`. The owner
types "use tmux" in the session's input box. Swarm runs `swarm type orchestrator` with
`SWARM_ADAPTER=herdr` and `SWARM_SESSION_ID=10`, and the text lands in that chat as if typed there.
Esc in the box runs `swarm interrupt orchestrator`.

## `swarm attach <agent_id>`

Caller `session`. Runs the adapter's `attach` verb with `SWARM_PANE` set to the agent's pane and
with stdin, stdout and stderr inherited, and exits with the verb's exit status. It fails before it
runs anything with `swarm: adapter <name> has no attach` or `swarm: no pane recorded`.

## `swarm agents --json`

Caller `session`. Every agent of the session, sorted by `id`.

```json
{
  "agents": [
    { "id": "coder-1", "role": "code.complex", "pane": "%3", "alive": true },
    { "id": "orchestrator", "role": "orchestrator", "pane": null, "alive": null }
  ]
}
```

`pane` is the recorded pane id, or `null` when the agent has none: the chair, or an agent that was
closed or reported dead. `alive` is `null` when `pane` is `null`, else whether the adapter's `list`
shows the pane. When `list` fails, every `alive` is `null`, swarm prints one stderr line, and the
command still exits 0.

## `swarm messages --json [--after <seq>]`

Caller `session`. The session's messages with `seq` greater than `--after` (default 0), in `seq`
order, at most 500. A reader pages by passing the last `seq` it has.

```json
{
  "messages": [
    {
      "seq": 7,
      "sender": "orchestrator",
      "recipient": "coder-1",
      "kind": "ask",
      "body": "Fix the parser",
      "created_at": 1789576942,
      "read": true
    }
  ]
}
```

`body` is the text of the message's body file, or `null` when that file cannot be read. `read` is
whether the recipient ran `swarm ack` for it.

## `swarm sessions --json`

Caller none: it needs no `SWARM_SESSION_ID` or `SWARM_AGENT_ID`. Every unarchived session that has
a recorded `cwd`, newest first (`created_at` descending, then `id` descending). A session made
before migration 0007 has no `cwd` and is left out.

Migration 0009 adds nullable `chair_provider TEXT` and `chair_id TEXT` columns to `session`.
Migration 0010 adds nullable `archived_at INTEGER` to `session`.

```json
{
  "sessions": [
    {
      "id": 10,
      "talk_mode": "lane",
      "adapter": "herdr",
      "cwd": "/Users/me/work/swarm/wt/main",
      "created_at": 1789600000,
      "chair_provider": "claude",
      "chair_id": "cc272e02-a473-4131-991a-d2c42f340438",
      "chair_log": "/Users/me/.claude/projects/-Users-me-work-swarm-wt-main/cc272e02-a473-4131-991a-d2c42f340438.jsonl",
      "agents": 11,
      "messages": 96,
      "last_message_at": 1789610000
    }
  ]
}
```

`swarm session new` records these, so any chair gets them, a CLI chat or Swarm:

- `adapter` is the adapter name `session new` ran with (`SWARM_ADAPTER`, default `tmux`), from
  migration 0008, or `null` for an older session. A reader that calls swarm about this session sets
  `SWARM_ADAPTER` to it, never to its own adapter.
- `cwd` is the absolute working directory of `swarm session new`.
- `created_at` is Unix seconds at `session new`.
- `chair_provider` and `chair_id` identify the chair CLI, or are both `null`.
- `chair_log` is the chair's transcript, or `null`. When its stored path is absent or missing and a
  chair is recorded, swarm searches for it at read time. Claude uses
  `<CLAUDE_CONFIG_DIR or ~/.claude>/projects/*/<id>.jsonl`. Codex uses
  `<CODEX_HOME or ~/.codex>/sessions/YYYY/MM/DD/rollout-*-<id>.jsonl` and checks only the UTC day
  of `created_at` and one day on each side. A found path is stored for later reads. Swarm never
  writes to the transcript.
- `agents` counts the session's agents, the chair included. `messages` counts its messages.
  `last_message_at` is the newest message's `created_at`, or `null` when it has none.

A reader gets one session's agents and messages with the calls above, with `SWARM_SESSION_ID` set
to that session's `id`.

## `swarm session archive <id>...`

Caller none. Sets `archived_at` to the current Unix time for every id and prints nothing. The
update is atomic. If any id is unknown, nothing changes and the command fails with
`swarm: no session <id>`. Archived sessions stay in the bus database but `sessions --json` leaves
them out.

Example. A Claude Code chat in `~/work/swarm/wt/main` runs `swarm session new lane` and gets 10.
Swarm has the project `~/work/swarm/wt/feature`, which shares that git repository, so it lists
session 10 under that project. Opening it draws the chat from `chair_log` and each agent's
`summary` messages from `swarm messages --json`.

## `swarm launch <agent_id> <role> [--account <auto|name>]`

Caller `session`. Starts the agent CLI for a role in a new pane.

1. `agent_id` must match `^[a-z0-9][a-z0-9-]{0,39}$`, because the tmux session name carries it.
   Otherwise it fails with `swarm: bad agent id <agent_id>` before anything else.
2. swarm resolves the role with `$SWARM_ROUTING_CMD get <role>`, which prints `provider`, `model`,
   `effort`, and optional `sandbox`, `approval` and `permission`. A failed run, or JSON with an
   `error` key, fails with `swarm: cannot resolve role <role>: <reason>`.
3. swarm builds the argv for the provider. The executable is the bare CLI name, which the pane's
   login shell finds on its `PATH`.
   - `claude`: `claude --model <model> --effort <effort>`, then `--permission-mode <permission>`
     when `permission` is set. `model` and `effort` are required.
   - `codex`: `codex --model <model> -c model_reasoning_effort="<effort>"`, then
     `--sandbox <sandbox>` and `--ask-for-approval <approval>` when set. When `sandbox` is
     `workspace-write`, it adds `-c sandbox_workspace_write.writable_roots=["<SWARM_HOME>/.swarm"]`
     so the agent can reach the bus. `model` and `effort` are required.
   - `agy`: `agy`, then `--model <model>` unless `model` is `default`, then `--effort <effort>`,
     then `--dangerously-skip-permissions` when `permission` is `skip`, or `--mode <permission>`
     when `permission` has another value. `effort` is required.
   - A missing required value fails with `swarm: role <role> has no <field>`. Any other provider
     fails with `swarm: role <role> uses unsupported provider <provider>`.
4. It then does exactly what `swarm spawn <agent_id> <role> --provider <provider> [--account <a>]
   -- <argv>` does: the pane id on stdout, and `account <name>` on stderr when `--account` was
   given. Without `--account`, the CLI runs on its default home.

## Swift side

`SwarmBus` is the protocol Swarm calls, and `AppModel.swarmBus` is the one shared instance. Views
call it only through a model or store beside them, never from a `body` or a button action.
`attachCommand(for:in:)` builds the argv and environment for `swarm attach` and runs nothing, so
the live pane can start it in a terminal view.
