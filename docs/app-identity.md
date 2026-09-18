# App identity

The UI in `ui/` ships as Swarm (ADR 0009). These values keep each part of its identity consistent.

| What | Value |
|---|---|
| App name (`CFBundleName`, `CFBundleDisplayName`, bundle folder) | `Swarm`, `Swarm.app` |
| Executable in the bundle (`CFBundleExecutable`) | `Swarm` |
| Bundle identifier | `io.github.priyanshuupadhyay.swarm` |
| Development bundle identifier | `io.github.priyanshuupadhyay.swarm.dev` |
| URL scheme and its `CFBundleURLName` | `swarm-ui`, `io.github.priyanshuupadhyay.swarm.deeplink` |
| Sleep helper plist and Mach service | `io.github.priyanshuupadhyay.swarm.sleep` (`.plist`) |
| Owner signing team | `not set` |
| Dispatch queue labels and log subsystem | prefix `io.github.priyanshuupadhyay.swarm` |
| Application Support folder | `Swarm` |
| Terminal tmux socket and session prefix | `swarmui-<hash>` and `swarmui` |
| MCP server name registered with agent CLIs | `swarm-ui-workspace-bridge` |

The URL scheme is `swarm-ui`, not `swarm`, and the tmux names start with `swarmui`, so nothing
collides with the `swarm` CLI or its `swarm` tmux socket.

Code names follow the same family: SwiftPM targets and products (`Swarm`, `SwarmCore`, `swarm-bridge`,
`swarm-sleep-helper`), Swift types, file names, and `SWARM_UI_*` development environment variables.
The build copies the `Swarm` product into the bundle as `Swarm`.

`ui/LICENSE.md` keeps Spatie's MIT notice, and the About window says the app includes code by
Spatie under the MIT licence.
