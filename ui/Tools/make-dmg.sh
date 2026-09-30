#!/bin/zsh
# Package the release Swarm.app into a drag-to-Applications DMG, ready to attach to a release.
# The app is ad-hoc signed and not notarized, so a recipient allows it once (printed at the end).
set -euo pipefail

root="${0:A:h:h}"
app="$root/.build/release/Swarm.app"
out_dir="${OUT_DIR:-$root/.build/release}"
# Pinned: settings.py relies on this version's option names.
dmgbuild_spec="dmgbuild==1.6.7"

[[ -d "$app" ]] || { print -u2 "$app not found. Run 'make app' first."; exit 1; }
command -v uvx >/dev/null || { print -u2 "uvx is required for make dmg (brew install uv)"; exit 1; }

version="$(plutil -extract CFBundleShortVersionString raw -o - "$app/Contents/Info.plist")"
dmg="$out_dir/Swarm-$version.dmg"
mkdir -p "$out_dir"
rm -f "$dmg"

# A stale mount would push dmgbuild's own mount to "/Volumes/Swarm 1".
while [[ -d /Volumes/Swarm ]]; do
  hdiutil detach /Volumes/Swarm -force >/dev/null 2>&1 || break
done

uvx --from "$dmgbuild_spec" dmgbuild -s "$root/Tools/dmg/settings.py" -D "app=$app" Swarm "$dmg"
print "==> $dmg"
print "To install, open the DMG and drag Swarm onto Applications."
print "The first launch is blocked because the app is not notarized. Allow it in System Settings >"
print "Privacy & Security > Open Anyway, or run: xattr -dr com.apple.quarantine /Applications/Swarm.app"
print "Swarm needs tmux (brew install tmux) and at least one agent CLI (claude, codex, or agy)."
