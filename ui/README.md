# Helper apps and release package

swarm ships as one Homebrew formula (ADR 0079). The web UI is inside the `swarm` binary, and
`swarm ui` opens it in `Swarm.app`. This folder holds the rest of the release.

`SwarmNotify` is the notice helper (ADR 0077). It ships at `libexec/SwarmNotify.app` beside
`bin/swarm`. `swarm` starts it for a notice, and it exits when idle.

`SwarmApp` is the window that `swarm ui` opens. It ships at `libexec/Swarm.app`, shows the page
in one WKWebView in a normal window, and in full screen (⌃⌘F) paints the camera row above the window
in the page's background colour. `swarm ui` opens it with `open -a`, so a running app only comes to the
front. `SWARM_APP` points `swarm ui` at another build of the app. Its icon is the HUD's Deno guest;
`make package` draws it with `SwarmHUD --app-icon <dir.iconset>` and `iconutil`. It has no Dock badge.

`SwarmHUD` is the notch usage HUD (ADR 0087). It ships at `libexec/SwarmHUD.app`, reads
`swarm usage --json` each minute and `swarm working --json` every 3 s, and keeps 7 days of samples
for the pace in `~/Library/Application Support/SwarmHUD/history.json`. `swarm hud install` starts it
at login. Closed, it shows a pixel mascot that works, waits or sleeps with the swarm agents, and a
ring for the best account's fullest limit; the open card shows the same mascot larger, with its
credit. A Codex account row shows its usage limit resets under its name. Swarm's own mascot, its
bee, 17 open-licence guests and any pets installed for Codex (`$CODEX_HOME/pets`, by default
`~/.codex/pets`, read once at launch) take turns every 10 minutes. `python3 -I Tools/mascots.py`
rebuilds the guests' sheets from pinned commits, and `MASCOT-NOTICE` holds
their licences, which `make package` appends to `libexec/NOTICE`. `SwarmHUD --snapshot <dir> --fixture
Tests/SwarmHUDTests/fixtures/nine-accounts.json --closed Tests/SwarmHUDTests/fixtures/closed.json
--history Tests/SwarmHUDTests/fixtures/history.json --working Tests/SwarmHUDTests/fixtures/working.json
--now 1792050120` renders each theme, each mascot family and state, the ring's edge cases, a
sheet of own mascot frames and a sheet of every guest in each state to PNG files with no window.

Run `make build` to compile the helpers.
Run `make package` to build `.build/release/swarm-<version>-macos-arm64.tar.gz`. It builds the Zig
`transcript` tool, `web/dist` with pnpm, `swarm` with the `serve` feature (which embeds
`web/dist`), and the helpers, and packs them with the bundled skills (ADR 0062). release.yml runs it
with `SWARM_RELEASE_BUILD=1`, so only that build uses `~/.swarm`; every other build keeps its data
in `~/.swarm-<branch>` (ADR 0027, ADR 0048). An explicit `SWARM_HOME` wins.
