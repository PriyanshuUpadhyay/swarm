#!/bin/zsh
set -euo pipefail

root="${0:A:h:h}"
repo="${root:h}"
export ZIG_GLOBAL_CACHE_DIR="${ZIG_GLOBAL_CACHE_DIR:-$root/.build/zig-global}"
export ZIG_LOCAL_CACHE_DIR="${ZIG_LOCAL_CACHE_DIR:-$root/.build/zig-local}"
install=0
for flag in "$@"; do
  case "$flag" in
    -r) ;;
    --install) install=1 ;;
    *) print -u2 "unknown option: $flag"; exit 1 ;;
  esac
done

zig build --build-file "$repo/packages/transcript/build.zig" -Doptimize=ReleaseSafe
# The app runs this swarm from Contents/Helpers; the cask links it onto PATH (ADR 0048).
cargo build --manifest-path "$repo/Cargo.toml" --release --locked
# The home changes only on --install: building the app must not migrate the real database. Even then
# no swarm goes on PATH, because a cargo build there would shadow the cask's link (ADR 0048).
if (( install )); then
  "$repo/target/release/swarm" init
  for adapter in herdr tmux tmux-solo; do
    "$repo/target/release/swarm" adapter check "$adapter"
  done
fi
swift build --package-path "$root" --disable-sandbox -c release --product Swarm
bin_dir="$(swift build --package-path "$root" --disable-sandbox -c release --show-bin-path)"
app="$root/.build/release/Swarm.app"

rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Helpers" "$app/Contents/Resources"
cp -R "$root/Sources/Swarm/Resources/DiffViewer" "$app/Contents/Resources/DiffViewer"
cp "$bin_dir/Swarm" "$app/Contents/MacOS/Swarm"
cp "$repo/packages/transcript/zig-out/bin/transcript" "$app/Contents/MacOS/transcript"
# Helpers, not MacOS: on a case-insensitive disk MacOS/swarm would overwrite MacOS/Swarm.
cp "$repo/target/release/swarm" "$app/Contents/Helpers/swarm"
cp "$root/Resources/Info.plist" "$app/Contents/Info.plist"
version="$(cargo metadata --manifest-path "$repo/Cargo.toml" --no-deps --format-version 1 | jq -r '.packages[0].version')"
plutil -insert CFBundleShortVersionString -string "$version" "$app/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Add :SwarmBuildDate string $(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  "$app/Contents/Info.plist"
# The app picks its swarm home from this branch; see SwarmHome and ADR 0027. A detached HEAD is
# "" (no branch). plutil takes the value as one argv string, so no quote in a branch name is parsed;
# the read-back refuses a build whose plist says another branch.
branch="$(git -C "$repo" symbolic-ref --short -q HEAD || true)"
plutil -insert SwarmBuildBranch -string "$branch" "$app/Contents/Info.plist"
if [[ "$(plutil -extract SwarmBuildBranch raw -o - "$app/Contents/Info.plist")" != "$branch" ]]; then
  print -u2 "SwarmBuildBranch in Info.plist does not read back as '$branch'"
  exit 1
fi
codesign --force --deep -s - "$app"
print "==> $app"

if (( install )); then
  destination="$HOME/Applications/Swarm.app"
  previous="$HOME/Applications/Swarm.app.previous"
  mkdir -p "$HOME/Applications"
  if [[ -e "$destination" ]]; then
    rm -rf "$previous"
    mv "$destination" "$previous"
  fi
  cp -R "$app" "$destination"
  print "==> $destination"
  print "==> swarm is no longer put on PATH; brew install --cask priyanshuupadhyay/tap/swarm-app links it (ADR 0048)"
fi
