---
status: proposed
date: 2026-10-14
deciders: [user]
supersedes: "0078"
amends: ["0048", "0077"]
related: ["0027", "0062", "0075"]
informed-by:
  - 'owner, 2026-10-14: "just use the raw swarm as the binary that we would ship, and it will have all the functionality via the CLI"'
  - "Owner decisions 2026-10-14 in tmp/release/brief-r1-formula.md: no Swarm.app and no swarm-app cask; the web UI is embedded and `swarm ui` opens it in the browser; SwarmNotify.app ships beside the binary"
---

# 0079. swarm ships as one formula, with the web UI inside the binary

## Context and Problem Statement

ADR 0078 shipped the web UI in a thin Swift window, Swarm.app, and ADR 0048 shipped that app and the
`swarm` CLI as one cask. The window adds little over a browser tab: it starts `swarm serve`, shows
one web view, and keeps a Dock badge, a folder picker and the PATH alert of 0048. The owner wants
one binary that does everything from the CLI, and the notice click of ADR 0077 must still work.

## Considered Options

- Keep Swarm.app and the cask (0078).
- One Homebrew formula: `swarm` with `web/dist` inside, and `swarm ui` opens the page in the
  browser. The notice helper, the transcript tool and the skills ship in the formula's `libexec`.
- One formula with the CLI only, and the web UI as a separate download.

## Decision Outcome

Chosen: one formula with the web UI inside the binary, because one install then gives every
surface, and no app bundle needs a window, a signature for Gatekeeper, or a DMG.

- `build.rs` writes a list of `include_bytes!` entries for every file in `web/dist` when the
  `serve` feature is on. `swarm serve` serves them; `--assets DIR` still replaces them for
  development. A release build fails when `web/dist` is missing; a dev build embeds nothing.
- `swarm ui [--port N] [--assets DIR]` starts the server on 127.0.0.1, prints the URL, writes it
  to `<root>/ui-url` (mode 0600, because the token is the API key), runs `open <url>` on macOS, and
  runs until Ctrl-C. When `<root>/ui-url` exists and that server answers `GET /api/badge` with its
  token, `swarm ui` opens that URL and exits, so one home has one `swarm ui` server.
- `ui/Tools/build.sh` (`make -C ui package`) builds `swarm-<version>-macos-arm64.tar.gz` with
  `bin/swarm`, `libexec/transcript`, `libexec/SwarmNotify.app` (ad hoc signed),
  `libexec/skills` with its manifest (ADR 0062), and `libexec/NOTICE`. release.yml uploads it to
  the draft release with `SWARM_RELEASE_BUILD=1`, so the build still uses `~/.swarm` (0027, 0048
  amendment). The DMG is gone.
- tap.yml writes `Formula/swarm.rb`: the tarball URL and sha256, `depends_on "tmux"`, arm64 and
  macOS 26 (the helper's target), `bin.install`, `libexec.install`, and a test that runs
  `swarm --version`. The same tap commit deletes `Casks/swarm-app.rb`.
- `swarm` finds what ships beside it with `paths::bundled`: it resolves its own path through links
  (Homebrew's `bin/swarm` links into the Cellar), then looks in `../libexec/` and in its own
  folder. That finds the notice helper (amends 0077: `SWARM_NOTIFY_APP`, else the bundled
  `SwarmNotify.app`; no `/Applications/Swarm.app` fallback) and the transcript tool
  (`SWARM_TRANSCRIPT_TOOL`, else the bundled one, else the build checkout's). `swarm skills
  refresh` reads `../libexec/skills` from the resolved binary.
- Deleted: the SwarmWeb shell and its tests, `ui/Resources/Info.plist`, the DMG tools, and
  `web/perf/shell-table.mjs` (it read results that only the shell wrote).
- The PATH alert of 0048 is dropped. It compared the PATH `swarm` with the app's bundled copy, and
  no bundled copy exists now: the formula's `bin/swarm` is the PATH `swarm`. `swarm --version`
  still names the commit and branch.

## Consequences

- Good: `brew install priyanshuupadhyay/tap/swarm` gives the CLI, the web UI, notices and skills,
  and `brew upgrade` moves them together.
- Good: a formula download carries no quarantine flag, so no Gatekeeper step is needed.
- Bad: the formula needs an arm64 Mac on macOS 26, because the helper targets macOS 26 and the
  tarball is built for arm64 only. Others use `cargo install --path . --features serve` after
  `pnpm --dir web build`.
- Bad: the browser has no folder picker, so Import Project takes a typed path, and no Dock badge
  shows the waiting count.
- Bad: a cask user must move by hand, because the tap drops the cask:
  `brew uninstall --cask swarm-app && brew install priyanshuupadhyay/tap/swarm`.
- Bad: the `swarm` that hook calls run carries the web UI too (about 0.9 MB more).

## Links

- Supersedes: 0078. Amends: 0048, 0077. Related: 0027, 0062, 0075.
