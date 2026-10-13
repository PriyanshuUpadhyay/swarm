---
status: proposed
date: 2026-10-14
deciders: [user]
supersedes: "0014"
related: ["0027", "0048", "0062", "0075", "0076", "0077"]
informed-by:
  - 'ADR 0076: "`ui/` (except the shell) is deleted when Scope v1 ships"'
  - "tmp/web-port/2026-10-13-web-port.md (Scope v1 rows 1-12 done in S1-S6)"
  - "Owner answers 2026-10-14 to the S7 block (tmp/web-port/s7-result.md): 1A port the PATH swarm alert to the shell, 2A and 3A Reveal, Open in Terminal, Finder drops and image paste are Later, 4A drop three Advanced items"
---

# 0078. The app is the web UI in the SwarmWeb shell, and the SwiftUI app is deleted

## Context and Problem Statement

ADR 0014 rebuilt the UI as a SwiftUI app in `ui/` (targets `Swarm`, `SwarmCore`, `TranscriptTool`,
about 34k lines of source and 21k of tests). ADR 0075 moved the UI to a web app on `swarm serve` in
a thin Swift shell, and ADR 0076 set the scope (Scope v1) and said `ui/`, except the shell, goes
when Scope v1 ships. S1 to S6 built Scope v1. Swarm.app must now ship the shell, and each
Swift-only behaviour needs an owner or an explicit drop.

## Considered Options

- Keep both apps until SwiftUI parity.
- Ship the shell as Swarm.app and delete the SwiftUI targets now.

## Decision Outcome

Chosen: ship the shell as Swarm.app and delete the SwiftUI targets, because ADR 0076 replaced
parity with Scope v1, and two apps double every fix.

- Swarm.app keeps its bundle id `io.github.priyanshuupadhyay.swarm` and name. Its main executable
  `Contents/MacOS/Swarm` is the `SwarmWeb` shell. The bundle holds `Contents/Helpers/swarm` (built
  with the `serve` feature), `Contents/MacOS/transcript`, `Contents/Resources/web` (`web/dist`),
  `Contents/Resources/NOTICE` (`web/NOTICE`), the bundled skills (ADR 0062) and
  `Contents/Helpers/SwarmNotify.app` (ADR 0077). `Tools/build.sh` signs it ad hoc as before, and
  release.yml builds it with `make -C ui dmg`, so the cask of ADR 0048 ships it with no change.
- Deleted: `ui/Sources/Swarm`, `ui/Sources/SwarmCore`, `ui/Sources/TranscriptTool`, their tests,
  `ui/Tools/lint.sh` (it checked only those targets), `ui/LICENSE.md` (the Bloom notice, for code
  that lived only there), the two Rust tests that compiled the SwiftUI `choices.json` decoder, and
  the generator that read a deleted Swift file.

Where each Swift-only behaviour goes:

| Behaviour | Owner |
|---|---|
| Notices, their click to a pane | SwarmNotify helper (0077) |
| Start at login | dropped (0077) |
| Dock badge, folder picker for Import | shell (0077) |
| Login shell PATH for `swarm` and the agents it starts (a Finder launch has only `/usr/bin:/bin:/usr/sbin:/sbin`) | shell: `$SHELL -i -l -c /usr/bin/env -0` once, 5 s limit, then the launch PATH and a fixed guess list |
| Transcript tool path | shell sets `SWARM_TRANSCRIPT_TOOL` to the bundled `transcript` |
| PATH swarm alert of 0048, with Copy Command and one answer per path-and-version pair | shell; the fix text follows the tap without the formula: `brew uninstall swarm; brew reinstall --cask --force priyanshuupadhyay/tap/swarm-app` |
| Home per build branch (0027) | Rust: the bundled `swarm` reads its own stamp |
| Window frame restore, Edit menu (⌘C, ⌘V in the page) | shell |
| `app.lock` run lock | none: LaunchServices keeps one app instance, and no process takes the lock now |
| Done notice from a tree change | ended (0077); a chair sends `swarm notify` |
| Clipboard, Create Project, settings pages | web app and `swarm serve` |
| Usage HUD panel, tab groups | dropped (0076) |
| Skills editor, skills refresh at start | dropped (0076 amendment) |
| Runs graph, Guards, Managed changes, Setup sheet, Files tree, chat windows, raw data mode | Later (0076) |
| Reveal folder in Finder, Open in Terminal, Reveal file, Reveal data home | Later |
| Finder drops (folder to import, files and images to the composer), image paste in the composer | Later |
| Advanced settings: helper version, performance logging, reset declined prompts | dropped |

## Consequences

- Good: one UI, and every UI check runs with Vite, Vitest and Playwright (0075).
- Good: the repo loses about 34k lines of Swift source and 21k lines of Swift tests.
- Bad: the bundled `swarm`, which the cask links onto PATH, is built with `serve`, so hook calls
  run a larger binary (10 MB; `swarm --version` took about 3 ms per call on 2026-10-14).
- Bad: the Later and dropped rows above are gone from the app until a new record brings them back.
- Bad: `app_holds_lock` in `src/main.rs` is now always false; it is dead code until removed.
- Bad: no Swift test now proves that the SwiftUI decoder reads a Rust-written `choices.json`; that
  decoder is gone, so only Rust and the web app read the file.

## Links

- Supersedes: 0014. Related: 0027, 0048, 0062, 0075, 0076, 0077.
