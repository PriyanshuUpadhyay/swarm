# App identity

The UI in `ui/` ships as Swarm (ADR 0009). Every seat that renames or removes Bloom's identity uses
these values, so the pieces agree.

| What | Value |
|---|---|
| App name (`CFBundleName`, `CFBundleDisplayName`, bundle folder) | `Swarm`, `Swarm.app` |
| Executable in the bundle (`CFBundleExecutable`) | `Swarm` |
| Bundle identifier | `io.github.priyanshuupadhyay.swarm` |
| Development bundle identifier (where Bloom had `be.spatie.bloom.dev`) | `io.github.priyanshuupadhyay.swarm.dev` |
| URL scheme and its `CFBundleURLName` | `swarm-ui`, `io.github.priyanshuupadhyay.swarm.deeplink` |
| Sleep helper plist and Mach service | `io.github.priyanshuupadhyay.swarm.sleep` (`.plist`) |
| Owner signing team | `not set` |
| Dispatch queue labels and log subsystem | prefix `io.github.priyanshuupadhyay.swarm` |
| Application Support folder | `Swarm` |
| Terminal tmux socket and session prefix | `swarmui-<hash>` and `swarmui` |
| MCP server name registered with agent CLIs | `swarm-ui-workspace-bridge` |

The URL scheme is `swarm-ui`, not `swarm`, and the tmux names start with `swarmui`, so nothing
collides with the `swarm` CLI or its `swarm` tmux socket.

Code names stay: SwiftPM targets and products (`Bloom`, `BloomCore`, `bloom-bridge`,
`bloom-sleep-helper`), Swift types, file names, and `BLOOM_*` development environment variables.
The build copies the `Bloom` product into the bundle as `Swarm`.

`ui/LICENSE.md` keeps Spatie's MIT notice, and the About window says the app is based on Bloom by
Spatie under the MIT licence.
