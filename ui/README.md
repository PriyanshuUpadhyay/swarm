# Swarm app

Swarm.app is the web UI in `web/` inside a thin Swift window (ADR 0075, ADR 0078). The SwiftUI app
that lived here was removed at the cutover; ADR 0078 lists where each of its native parts went.

`SwarmWeb` is the app's main executable. It starts `swarm serve` from `Contents/Helpers/swarm` (or
`SWARM_BIN`) with the web assets in `Contents/Resources/web`, gives it the login shell's PATH and the
bundled `transcript`, and shows the page in one WKWebView. It keeps the menu keys, the Edit menu, the
window frame, the Dock badge, the folder picker, and the PATH swarm alert of ADR 0048. Outside the
bundle, pass `--swarm BIN --assets DIR`; `--smoke` and `--measure OUT.json` run the checks of
`web/perf/run.mjs` in this web view.

`SwarmNotify` is the notice helper at `Contents/Helpers/SwarmNotify.app` (ADR 0077). `swarm` starts
it for a notice, and it exits when idle.

Run `make build` to compile the Swift targets.
Run `make test` to run the Swift tests.
Run `make app` to build `.build/release/Swarm.app`. It builds `swarm` with the `serve` feature,
`web/dist` with pnpm, the Zig `transcript` tool, the shell, and the helper.
Run `make install` to put it in `~/Applications`, keep the old app, and run `swarm init` with this
build. It puts no `swarm` on PATH, because the cask's link is the PATH `swarm` (ADR 0048).
`make app` never changes `~/.swarm`.
Run `make run` to build and launch the app.
Run `make dmg` to package the app as `.build/release/Swarm-<version>.dmg`.
A release build (`SWARM_RELEASE_BUILD=1`, as release.yml builds it) shows one alert at launch when
the login PATH's `swarm` is another build. Every other build, `main` too, keeps its data in
`~/.swarm-<branch>`, because the bundled `swarm` picks its home from the branch it was built on
(ADR 0027, ADR 0048). An explicit `SWARM_HOME` in the app's environment wins.
