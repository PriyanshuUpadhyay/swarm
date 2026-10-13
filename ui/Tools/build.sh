#!/bin/zsh
# Builds the release tarball that the Homebrew formula installs (ADR 0079):
#   swarm-<version>/bin/swarm                    the CLI, with web/dist inside for `swarm ui`
#   swarm-<version>/libexec/transcript           the row tool that `swarm serve` runs
#   swarm-<version>/libexec/SwarmNotify.app      the notice helper (ADR 0077)
#   swarm-<version>/libexec/skills               the source of `swarm skills refresh`
#   swarm-<version>/libexec/NOTICE               the web UI's third-party notices
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
bin_dir="$(swift build --package-path "$root" --disable-sandbox -c release --show-bin-path)"
version="$(cargo metadata --manifest-path "$repo/Cargo.toml" --no-deps --format-version 1 | jq -r '.packages[0].version')"

out="$root/.build/release"
stage="$out/swarm-$version"
rm -rf "$stage"
mkdir -p "$stage/bin" "$stage/libexec"
cp "$repo/target/release/swarm" "$stage/bin/swarm"
cp "$repo/packages/transcript/zig-out/bin/transcript" "$stage/libexec/transcript"
cp "$repo/web/NOTICE" "$stage/libexec/NOTICE"
skills="$stage/libexec/skills"
mkdir -p "$skills"
tar -c -C "$repo/skills" --exclude='.git*' \
  swarm-orchestrator swarm-voice kit/skills kit/references kit/scripts kit/contracts \
  kit/vendor kit/LICENSE kit/README.md | tar -xp -C "$skills"
"$stage/bin/swarm" skills manifest "$skills" "$skills/manifest.json"
notify="$stage/libexec/SwarmNotify.app"
mkdir -p "$notify/Contents/MacOS"
cp "$bin_dir/SwarmNotify" "$notify/Contents/MacOS/SwarmNotify"
cp "$root/Resources/SwarmNotify-Info.plist" "$notify/Contents/Info.plist"
plutil -insert CFBundleShortVersionString -string "$version" "$notify/Contents/Info.plist"
# An ad hoc signature seals the plist to the binary, which the notification center needs.
codesign --force -s - "$notify"

tarball="$out/swarm-$version-macos-arm64.tar.gz"
tar -czf "$tarball" -C "$out" "swarm-$version"
print "==> $tarball"
