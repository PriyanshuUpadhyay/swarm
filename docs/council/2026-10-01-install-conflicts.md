# Council: swarm install must not conflict with a user's existing setup

Date: 2026-10-01. Invoked by the owner through `flow` (`tmp/flow/2026-10-01-main/01-frame.md`):
"need to make sure that we dont conflict with user's bare setup of advence setup when they
install ... hook setup should show the diff and edits it would do. if there are conflicts it
should say so ... if apple provides storage for each app which it should then we should rather use
that to resolve conflicts. get the council to opine".

## Artifact

`main` at `3d6d6a7f`. `src/paths.rs`, `src/main.rs:16-39` (init), `src/main.rs:537-572` (hooks),
`src/bus.rs:250-380` and `:640-710` (Codex trust, AGY group), `src/store.rs:15-50`, ADRs 0027,
0029, 0031, 0034, `ui/Sources/Swarm/HooksSetupSheet.swift`.

## Round 0

Run with round 1. Gemini and GPT had no questions. Claude asked two, and the chair answered both
from evidence. (1) The app writes nothing in the swarm home; it writes only temp folders
(`AgentScratchDirectory.swift`, `ComposerState.swift`). (2) The owner's
`swarm hooks status --json` is `{"codex":true,"agy":true}`, so the owner already has the ADR 0034
shared hash.

## Models used

`council.claude` (claude opus, xhigh), `council.gpt` (codex `gpt-6.1-sol`, xhigh),
`council.gemini` (agy `gemini-3.8-flash-high`). All three ran both rounds.

## Verdict: GO-WITH-CHANGES (unanimous bucket; marker form 2-1)

Converged at round 2. Round 1 took about 6 minutes, round 2 about 4 minutes.

Consensus reasoning: Apple's per-app storage does not fix either conflict. Application Support is
a naming convention, not an exclusive store, so swarm still needs an ownership check there. A brew
CLI has no bundle, and branch builds share the app's bundle id. A sandbox container exists only for
a sandboxed app. A move also changes the ADR 0034 hook text and the owner's `profiles.json` link.
The hook files belong to Codex and AGY at fixed paths, so no per-app store can separate them. The
real defects are that `init` writes before any ownership check, and that hook setup silently
overwrites any entry at swarm's key or group name.

Required changes:

1. Keep `~/.swarm` and the ADR 0027 branch homes. Do not move to Application Support or a
   container.
2. Add an owner proof that is checked before any write in the home. A new home is claimed first.
3. Adopt an unmarked old home only when a read-only open of `swarm.db` shows `user_version` 1..=4
   and swarm's migration-0001 tables. `user_version` alone is not proof.
4. Refuse any other existing folder, change nothing in it, and print the folder and the fix
   (`SWARM_HOME=...`).
5. Hooks: a missing entry is an add. An entry that equals the current text exactly is unchanged.
   An entry that equals one exact known old swarm form is an update. Any other entry is a conflict.
   The key or the group name `swarm` is never proof.
6. If any file has a conflict, setup writes no file and exits non-zero. Each conflict names the
   file, the key or group, what was found, what swarm wants, and a one-line fix.
7. A read-only plan mode prints a unified diff per file and the conflict list, with JSON for the
   app. The sheet shows the diff and conflicts and disables approval on a conflict. Apply rechecks
   the approved bytes and refuses a file that changed after consent.
8. A linked home is allowed when the link target passes the owner check. ADR 0031's linked
   `profiles.json` stays.
9. Close `src/store.rs:27-28`, where a foreign `swarm.db` with `user_version` 0 gets swarm's
   tables.

Dissent / residual risk:

- Owner proof form. Claude and GPT want a marker file (for example `.swarm/swarm-home`) made with
  `create_new` before any other write, because `init` makes folders before the db and a stat is
  cheap. Gemini wants SQLite `PRAGMA application_id` in `swarm.db`.
- Old hook forms. Claude says the list starts empty, because each pre-0034 form holds a build path
  and cannot be complete. GPT and Gemini want a finite list of exact old templates. GPT warns that a
  suffix match such as `... hook agy Stop` is not proof.
- GPT also wants a marker or db link that points outside the checked root to be refused.
- A false refusal or false conflict can lock out a real owner, so each message must give the fix.

Per-model trail:

- GEMINI: R1 marker file plus `user_version` adoption; R2 moved to `application_id` plus tables,
  fixed old-form list, links allowed after the check.
- GPT: R1 marker plus ownership receipt, refuse links; R2 dropped the receipt and the blanket link
  refusal, kept exact old templates and the escape-link refusal.
- CLAUDE: R1 `application_id`, a list of old exe hashes; R2 moved to a marker file and an empty
  old-form list, no link rule.
