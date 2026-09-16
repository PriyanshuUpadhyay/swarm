# Agent bus contract for the UI

Bloom starts swarm agents, chats with them, and shows each one's live pane (ADR 0003). Bloom is the
chair of every agent it starts (ADR 0007), and swarm builds the command line for a role (ADR 0008).
Bloom reads only the JSON and commands below, through the Swift types in
`ui/Sources/BloomCore/Agent/SwarmBus.swift`. A change to a shape changes this file, the Rust output,
and the Swift types in one commit. The rules of `docs/profiles-contract.md` apply here too: one JSON
object on stdout and exit 0, or one stderr line and a non-zero exit; `snake_case` keys; `null`,
never a missing key.

## How Bloom calls swarm

Every call Bloom makes sets `SWARM_ADAPTER=tmux-solo`. Every call after `session new` also sets
`SWARM_SESSION_ID` to the workspace's session and `SWARM_AGENT_ID=orchestrator`. A call that starts
panes (`launch`) runs with its working directory, and `PWD`, set to the workspace's worktree.

One Bloom workspace has at most one swarm session. Bloom creates it the first time the workspace
starts an agent, with `swarm init`, `swarm session new lane`, and `swarm agent add orchestrator
orchestrator`, and keeps the id. While a session has an agent with a pane, Bloom runs `swarm sweep`
every 30 seconds, which re-rings an unread ask and reports an agent that died.

The agent CLI answers only if the `swarm-voice` skill is installed for it
(`scripts/install-skills.sh`). This contract does not check that.

Example. The user starts `coder-1` with role `code.complex` and types "Fix the parser". Bloom runs
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

`SwarmBus` is the protocol Bloom calls, and `AppModel.swarmBus` is the one shared instance. Views
call it only through a model or store beside them, never from a `body` or a button action.
`attachCommand(for:in:)` builds the argv and environment for `swarm attach` and runs nothing, so
the live pane can start it in a terminal view.
