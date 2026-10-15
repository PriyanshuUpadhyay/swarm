#!/bin/zsh
# Builds the release tarball that the Homebrew formula installs (ADR 0079):
#   swarm-<version>/bin/swarm                    the CLI, with web/dist inside for `swarm ui`
#   swarm-<version>/libexec/transcript           the row tool that `swarm serve` runs
#   swarm-<version>/libexec/SwarmNotify.app      the notice helper (ADR 0077)
#   swarm-<version>/libexec/SwarmHUD.app         the notch usage HUD (ADR 0087)
#   swarm-<version>/libexec/skills               the plugin swarm (kit/) and the swarm skills
#   swarm-<version>/libexec/NOTICE               third-party notices: the web UI's, then the HUD mascots'
set -euo pipefail

root="${0:A:h:h}"
repo="${root:h}"
export ZIG_GLOBAL_CACHE_DIR="${ZIG_GLOBAL_CACHE_DIR:-$root/.build/zig-global}"
export ZIG_LOCAL_CACHE_DIR="${ZIG_LOCAL_CACHE_DIR:-$root/.build/zig-local}"
if [[ "$(uname -m)" != arm64 ]]; then
  print -u2 "the tarball is named macos-arm64, so build it on an arm64 Mac"
  exit 1
fi

zig build --build-file "$repo/packages/transcript/build.zig" -Doptimize=ReleaseSafe
# build.rs embeds web/dist, so the web build comes first.
pnpm --dir "$repo/web" install --frozen-lockfile
pnpm --dir "$repo/web" build
cargo build --manifest-path "$repo/Cargo.toml" --release --locked --features serve
swift build --package-path "$root" --disable-sandbox -c release --product SwarmNotify
swift build --package-path "$root" --disable-sandbox -c release --product SwarmHUD
bin_dir="$(swift build --package-path "$root" --disable-sandbox -c release --show-bin-path)"
version="$(cargo metadata --manifest-path "$repo/Cargo.toml" --no-deps --format-version 1 | jq -r '.packages[0].version')"

out="$root/.build/release"
stage="$out/swarm-$version"
rm -rf "$stage"
mkdir -p "$stage/bin" "$stage/libexec"
cp "$repo/target/release/swarm" "$stage/bin/swarm"
cp "$repo/packages/transcript/zig-out/bin/transcript" "$stage/libexec/transcript"
cat "$repo/web/NOTICE" "$root/MASCOT-NOTICE" > "$stage/libexec/NOTICE"
skills="$stage/libexec/skills"
mkdir -p "$skills"
# The whole kit, because it is the plugin swarm: its skills read blocks/ two folders up, and
# .claude-plugin/ holds the manifest that the CLIs install from.
tar -c -C "$repo/skills" --exclude='.git*' --exclude='__pycache__' --exclude='node_modules' \
  swarm-orchestrator swarm-voice kit | tar -xp -C "$skills"
python3 -I "$skills/kit/scripts/test_block_refs.py"
"$stage/bin/swarm" skills manifest "$skills" "$skills/manifest.json"
notify="$stage/libexec/SwarmNotify.app"
mkdir -p "$notify/Contents/MacOS"
cp "$bin_dir/SwarmNotify" "$notify/Contents/MacOS/SwarmNotify"
cp "$root/Resources/SwarmNotify-Info.plist" "$notify/Contents/Info.plist"
plutil -insert CFBundleShortVersionString -string "$version" "$notify/Contents/Info.plist"
# An ad hoc signature seals the plist to the binary, which the notification center needs.
codesign --force -s - "$notify"
hud="$stage/libexec/SwarmHUD.app"
mkdir -p "$hud/Contents/MacOS"
cp "$bin_dir/SwarmHUD" "$hud/Contents/MacOS/SwarmHUD"
cp "$root/Resources/SwarmHUD-Info.plist" "$hud/Contents/Info.plist"
plutil -insert CFBundleShortVersionString -string "$version" "$hud/Contents/Info.plist"
codesign --force -s - "$hud"

tarball="$out/swarm-$version-macos-arm64.tar.gz"
tar -czf "$tarball" -C "$out" "swarm-$version"
print "==> $tarball"
