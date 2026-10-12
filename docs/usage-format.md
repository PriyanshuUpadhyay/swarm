# Usage and account format

`swarm usage --json` and `swarm usage --refresh --provider <claude|codex> --json` print a usage
object. `swarm accounts --provider <p> --json` prints an account list. This file is the contract for
the two, and for the OpenUsage export of `swarm usage --format openusage`.

## Version

Each object has a `schema` key at the top: `swarm.usage.v1` for usage, `swarm.accounts.v1` for
accounts. A change in `v1` only adds fields or codes. A reader ignores a field or a code that it
does not know. A removed or renamed field, or a changed unit, needs `v2`.

## Usage object

`{"schema": "swarm.usage.v1", "meters": [meter, ...]}`. A meter is one usage window of one account.

| Field | Type | Meaning |
|---|---|---|
| `provider` | string | `claude`, `codex`, or `agy` |
| `account` | string or null | The account name. Null for a provider row with no account, such as `agy` |
| `label` | string | The name to show: the account name, else the provider |
| `window` | string or null | The window id: `5h`, `7d`, `fb` (Fable week) for Claude; `primary` or `secondary`, with `<limit id>:` before it for a named Codex limit |
| `window_minutes` | integer or null | The window length in minutes |
| `used_pct` | integer or null | Percent used, 0 to 100. Null when there is no reading; never a zero in its place |
| `reset_time_seconds` | integer or null | When the window resets, in seconds since 1970 (UTC) |
| `state` | string | See the states below |
| `source` | string or null | Where the reading comes from: `claude_api` or `codex_app_server` |
| `reason` | string or null | Text for a person. Do not parse it |
| `reason_code` | string or null | The machine code of `reason`. Set exactly when `reason` is set |
| `as_of_seconds` | integer or null | When the reading was taken, in seconds since 1970 (UTC) |

A meter `state` is one of five values:

| State | Meaning |
|---|---|
| `fresh` | A reading from the last 300 seconds |
| `stale` | An older reading, or a reading that a failed refresh kept |
| `missing` | No reading yet, or the account is signed out |
| `failed` | Swarm could not read usage, and it has no reading to keep |
| `no_source` | Swarm has no usage source for this account or provider |

## Account list

`{"schema": "swarm.accounts.v1", "provider", "source", "state", "revision", "modified", "auto",
"accounts": [account, ...]}`. `state` is `ready`, `unavailable`, or `no_source`. `auto` is the name
of the account that a launch with no `--account` takes, or null. `revision` is an opaque text for
the next change.

| Field | Type | Meaning |
|---|---|---|
| `name` | string | The account name |
| `email` | string or null | The sign-in email, when the CLI gives one |
| `home` | string | The account's folder |
| `env` | object of strings | The environment that a pane of this account gets |
| `auth_state` | string | `signed_in`, `signed_out`, or `unavailable` (swarm could not read the sign-in state) |
| `usage_state` | string | The worst meter `state` of the account |
| `usage_source` | string or null | As `source` of a meter |
| `remaining_pct` | integer or null | Percent left in the fullest window, 0 to 100, for `fresh` or `stale` |
| `summary` | string or null | Text for a person, such as `40% left` |

## Reason codes

A reader matches `reason_code` and never the `reason` text, because the text can change (RFC 9457,
section 3.1.4). The list can grow. For a code that a reader does not know, it shows `reason` as
plain text. `no_source` and `read_failed` have more than one text, and each text names the cause.
That holds for refresh output only, because a cached read gives a stored meter that is not `fresh` or
`stale` the base text of its code, such as "Usage read failed".

| Code | Text | Meaning |
|---|---|---|
| `token_expired` | Token expired | The sign-in token expired. The CLI renews it when a chat starts, so Auto may pick the account |
| `relogin_required` | Sign in again | The provider refused a token that has not expired |
| `no_credentials` | Account is signed out | The account has no sign-in |
| `refresh_failed` | Usage refresh failed; old reading | A refresh failed, and the meter keeps the old reading (`stale`) |
| `no_cached_usage` | No cached usage | No refresh has run yet; a refresh can fix it |
| `no_source` | No usage source | Swarm has no usage source, such as an API-key account or AGY |
| `read_failed` | Usage read failed | Swarm could not read usage and has no reading (`failed`) |
| `no_quota_windows` | No Codex quota windows | Codex answered with no quota window; a refresh does not fix it |

`refresh_failed` keeps an old reading, so `used_pct` stays. `read_failed` has no reading, so
`used_pct` is null.

## OpenUsage export

`swarm usage --format openusage` does the same cached read as `swarm usage --json`, with no provider
call, and prints an `openusage.limits.v1` object: `schema`, `generatedAt`, `providers`, `errors`.

- The provider key is `<provider>` for the account named `default`, else `<provider>@<account>`.
  OpenUsage reads the provider family as the text before the first `@`.
- `displayName` is the provider name, with the account name in parentheses when it is not `default`.
- `fetchedAt` is the newest `as_of_seconds` of the account, and `expiresAt` is 300 seconds later.
- `stale` is true when a meter of the account is `stale`. OpenUsage itself means "`expiresAt` has
  passed"; a `stale` meter in swarm is older than 300 seconds, so the two agree for a cached read.
- Claude `5h` is `session`, `7d` is `weekly`, `fb` is `fable`. The main Codex limit's 300-minute
  window is `session` and its 10080-minute window is `weekly`. Another window is left out.
- A meter with `used_pct` is a resource: `kind` `consumption`, `unit` `percent`, `used`, `limit`
  100, `remaining`, `utilization` (0 to 1), `resetsAt`, and `windowSeconds` when known.
- A `failed` meter adds `{"providerId", "message"}` to `errors`, once per account. A `missing` or
  `no_source` meter adds nothing. An account with no resource has no entry under `providers`.
- Times are ISO 8601 in UTC with milliseconds, such as `2026-07-13T01:39:30.000Z`.

| Swarm field | OpenUsage name |
|---|---|
| `provider` and `account` | provider key `<provider>` or `<provider>@<account>` |
| `window` | resource key (`session`, `weekly`, `fable`) |
| `used_pct` | `used`; also `remaining` = 100 - used, `utilization` = used / 100 |
| `reset_time_seconds` | `resetsAt` |
| `window_minutes` | `windowSeconds` (× 60) |
| `as_of_seconds` | `fetchedAt`; `expiresAt` = `fetchedAt` + 300 s |
| `state` `stale` | `stale` |
| `reason` of a `failed` meter | `errors[].message` |

## Left out

Both formats leave out tokens and the plan name. The export also leaves out the home path, the
email, `env`, the state of each meter, and the reason codes.
