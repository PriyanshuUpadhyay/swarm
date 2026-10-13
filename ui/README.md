# Notice helper and release package

swarm ships as one Homebrew formula (ADR 0079). The web UI is inside the `swarm` binary, and
`swarm ui` opens it in the browser. This folder holds the rest of the release.

`SwarmNotify` is the notice helper (ADR 0077). It ships at `libexec/SwarmNotify.app` beside
`bin/swarm`. `swarm` starts it for a notice, and it exits when idle.

Run `make build` to compile the helper.
Run `make package` to build `.build/release/swarm-<version>-macos-arm64.tar.gz`. It builds the Zig
`transcript` tool, `web/dist` with pnpm, `swarm` with the `serve` feature (which embeds
`web/dist`), and the helper, and packs them with the bundled skills (ADR 0062). release.yml runs it
with `SWARM_RELEASE_BUILD=1`, so only that build uses `~/.swarm`; every other build keeps its data
in `~/.swarm-<branch>` (ADR 0027, ADR 0048). An explicit `SWARM_HOME` wins.
