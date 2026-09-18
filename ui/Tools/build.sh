#!/bin/zsh
# Builds the Swarm product and assembles a launchable Swarm.app bundle.
#
#   ./Tools/build.sh            debug build
#   ./Tools/build.sh -r         release build
#   ./Tools/build.sh -r --run   release build, then launch it
#
#   make app / make run         the same two through the Makefile

set -euo pipefail
cd "$(dirname "$0")/.."

CONFIG=debug
RUN=0
BUILD_ARGS=()
while (( $# )); do
  arg="$1"
  shift
  case "$arg" in
    -r|--release) CONFIG=release ;;
    --run) RUN=1 ;;
    --jobs) BUILD_ARGS+=(--jobs "${1:?--jobs needs a number}"); shift ;;
    *) echo "unknown option: $arg" >&2; exit 1 ;;
  esac
done

echo "==> swift build -c $CONFIG"
swift build -c "$CONFIG" "${BUILD_ARGS[@]}" --product Swarm
# The MCP stdio shim an agent CLI launches. A separate invocation because --product names one
# product, and a separate binary because that is what an MCP server registration can point at: the
# CLI spawns it, it forwards to the app over a unix socket, and the app answers. See BridgeShim.
swift build -c "$CONFIG" "${BUILD_ARGS[@]}" --product swarm-bridge
# The privileged daemon that holds the lid, for the same reason: one product per invocation.
swift build -c "$CONFIG" "${BUILD_ARGS[@]}" --product swarm-sleep-helper

BIN_DIR="$(swift build -c "$CONFIG" --show-bin-path)"
APP="$BIN_DIR/Swarm.app"

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

cp "$BIN_DIR/Swarm" "$APP/Contents/MacOS/Swarm"
# Beside the app's own executable, which is where BridgeRegistration.shimPath looks for it. A
# bundle without it is not broken: every chat simply has no bridge tools, which is what every chat
# had before the bridge existed.
cp "$BIN_DIR/swarm-bridge" "$APP/Contents/MacOS/swarm-bridge"
# `SMAppService.daemon(plistName:)` reads this one path and no other, and the plist's BundleProgram
# points back at the executable beside it. Both are signed by the pass at the foot of this file.
cp "$BIN_DIR/swarm-sleep-helper" "$APP/Contents/MacOS/swarm-sleep-helper"
mkdir -p "$APP/Contents/Library/LaunchDaemons"
cp Resources/io.github.priyanshuupadhyay.swarm.sleep.plist "$APP/Contents/Library/LaunchDaemons/"
cp Resources/Info.plist "$APP/Contents/Info.plist"

plist_set() {
  local key="$1" type="$2" value="$3"
  /usr/libexec/PlistBuddy -c "Set :$key $value" "$APP/Contents/Info.plist" >/dev/null 2>&1 \
    || /usr/libexec/PlistBuddy -c "Add :$key $type $value" "$APP/Contents/Info.plist" >/dev/null
}

# What version this build claims to be.
#
# Resources/Info.plist carries a fixed placeholder. A build only claims a version when it is given
# one, and `SwarmBuildChannel` records whether that happened so `BuildIdentity` can distinguish a
# release from a working copy.
#
#   SWARM_UI_VERSION=0.2.0 SWARM_UI_BUILD=7 ./Tools/build.sh -r
#
# SWARM_UI_BUILD has to increase with every release and never repeat. The release pipeline derives
# both values from the tag it is building.
if [[ -n "${SWARM_UI_VERSION:-}" && -n "${SWARM_UI_BUILD:-}" ]]; then
  plist_set CFBundleShortVersionString string "$SWARM_UI_VERSION"
  plist_set CFBundleVersion string "$SWARM_UI_BUILD"
  plist_set SwarmBuildChannel string release
  echo "==> version $SWARM_UI_VERSION ($SWARM_UI_BUILD)"
else
  plist_set SwarmBuildChannel string local
fi

# When this bundle was assembled, which is the only thing that tells two development builds apart.
#
# A release has a version and a build number. A build made here has neither: BuildIdentity prints
# "Development build" for it, plus the commit for the copy Tools/master.sh installs, so two builds
# made an hour apart from the same commit print the same line, and there are usually several of
# them on this machine at once. The About window now adds this date for those cases and ignores it
# for a release, where it would describe the release runner rather than anything the reader has.
#
# Stamped here rather than measured at runtime from the executable's modification date, because an
# mtime moves when a bundle is copied and codesign rewrites the binary below, so that number would
# be an approximation in the shape of a fact. That is the same mistake as reading the placeholder
# version out of Resources/Info.plist, one level down. Written on every build, release included,
# because what the About window does with it is the window's decision and not this script's.
#
# UTC and ISO 8601, so the value is unambiguous wherever it is read and whoever reads it; the
# window renders it in the reader's own zone and locale. See BuildTimestamp.
plist_set SwarmBuildDate string "$(date -u +%Y-%m-%dT%H:%M:%SZ)"

# SwiftPM used to put products at <scratch>/<triple>/<config>. The Xcode build
# system puts them at <scratch>/out/Products/<config>, so two dirnames from
# BIN_DIR lands on `out` rather than the scratch that holds artifacts and
# checkouts. Walk up until the named sibling exists.
spm_scratch_containing() {
  local scratch name="$1"
  scratch="$(dirname "$(dirname "$BIN_DIR")")"
  while [[ ! -d "$scratch/$name" && "$scratch" != "/" ]]; do
    scratch="$(dirname "$scratch")"
  done
  print -r -- "$scratch"
}

zsh Tools/package-licences.sh "$APP" "$(spm_scratch_containing checkouts)/checkouts"

# The accent Swarm hands to AppKit, checked against the one Swarm draws with itself.
#
# Resources/Assets.xcassets/AccentColor.colorset is a colour set and nothing more, and a colour set
# cannot reference a Swift constant. So the hex is stated twice, once in `PaletteInk.accentFill` and
# once in the JSON, and this reads both and refuses a build where they have drifted. Without the
# check the failure is silent and permanent: every AppKit control would go on drawing the colour the
# ramp used to be, and the window would be back to two accents with nothing saying so.
verify_accent_matches_palette() {
  local colourset=Resources/Assets.xcassets/AccentColor.colorset/Contents.json
  local ink=Sources/SwarmCore/Presentation/PaletteInk.swift
  [[ -f "$colourset" && -f "$ink" ]] || return 0

  local declared asset
  # Pair(light: 0x197593, dark: 0x197593). Both members, because a pair whose halves differ cannot
  # be one colour set and this should say so rather than silently taking the light one.
  declared="$(sed -n 's/.*accentFill = Pair(light: 0x\([0-9A-Fa-f]*\), dark: 0x\([0-9A-Fa-f]*\)).*/\1 \2/p' "$ink")"
  if [[ "${declared%% *}" != "${declared##* }" ]]; then
    echo "==> accent: PaletteInk.accentFill is a pair ($declared), which one colour set cannot be" >&2
    return 1
  fi

  asset="$(/usr/bin/python3 -c '
import json, sys
c = json.load(open(sys.argv[1]))["colors"][0]["color"]["components"]
print("".join(c[k][2:].upper() for k in ("red", "green", "blue")))
' "$colourset")"

  if [[ "$asset" != "$(echo "${declared%% *}" | tr "[:lower:]" "[:upper:]")" ]]; then
    echo "==> accent: $colourset says #$asset, PaletteInk.accentFill says #${declared%% *}" >&2
    return 1
  fi
}

verify_accent_matches_palette

# macOS 26 draws an app icon from a layered Icon Composer document rather than from a flat bitmap:
# the glass, the shadow and the specular pass belong to the system and are applied live to the
# layers. Resources/Swarm.icon is that document. actool compiles it into an Assets.car, which the
# system finds through CFBundleIconName in Info.plist. It is now the only icon in the bundle: the
# floor is macOS 26 and there is no system left that would draw a flat one. Tools/icon/make.py's
# docstring carries the measurement that settled that.
#
# Resources/Assets.xcassets goes into the same catalogue and the same invocation, because a second
# actool run compiling to the same directory writes a second Assets.car over the first and the app
# loses whichever went in first. One run, two inputs, one file with both in it. What is in the
# catalogue besides the icon is the AccentColor set NSAccentColorName names, which is what makes
# every AppKit control in the window draw in Swarm's accent rather than the user's.
#
# Command line tools on their own carry no actool, so a machine with only those produces a bundle
# with no icon at all, and no accent either: the app then falls back to the system accent, which is
# what it drew before this existed. That is loud enough to notice and cheaper than failing the
# build.
compile_asset_catalogue() {
  local iconName=Swarm deployment
  local -a inputs
  [[ -d "Resources/$iconName.icon" ]] && inputs+=("$PWD/Resources/$iconName.icon")
  [[ -d "Resources/Assets.xcassets" ]] && inputs+=("$PWD/Resources/Assets.xcassets")
  (( ${#inputs} )) || return 0

  if ! xcrun --find actool >/dev/null 2>&1; then
    echo "==> skipping asset catalogue: actool not found"
    return 0
  fi

  deployment="$(/usr/libexec/PlistBuddy -c 'Print :LSMinimumSystemVersion' Resources/Info.plist)"

  # Absolute, because actool hands a relative input path to ibtoold, which resolves it against a
  # working directory of its own and crashes rather than reporting a missing file.
  xcrun actool "${inputs[@]}" \
    --compile "$APP/Contents/Resources" \
    --app-icon "$iconName" \
    --output-partial-info-plist "$BIN_DIR/$iconName.icon.plist" \
    --platform macosx \
    --target-device mac \
    --minimum-deployment-target "$deployment" \
    --errors --warnings >/dev/null

  # actool writes a flattened $iconName.icns beside the catalogue as well, as a fallback for a
  # system that cannot read the catalogue. There is no such system at this floor, and the flattened
  # file is the layers without the passes that make them read, so it is dropped rather than shipped
  # as a worse copy of the icon nothing will ask for.
  rm -f "$APP/Contents/Resources/$iconName.icns"

  # Nothing above proves the catalogue arrived: actool reports a failure in the plist it prints and
  # is not reliably non-zero about it. The bundle either has the file or the build is wrong.
  if [[ ! -f "$APP/Contents/Resources/Assets.car" ]]; then
    echo "==> asset catalogue: actool produced no Assets.car" >&2
    return 1
  fi
}

compile_asset_catalogue

# What the app looks up in its own bundle by name: the Spatie logos the About pane draws, the
# menu bar mark, and the product marks the About window's makers section shows. The logos and the
# mark are PDFs rather than bitmaps, because AppKit redraws a PDF as vector art at whatever scale
# the display asks for, so one file is right on a Retina display and on a 1x monitor. The .svg
# beside each logo is the source it was generated from and is not needed at runtime; the menu bar
# mark's source is Tools/icon/menubar.py. The Maker*.png files are the exception to the PDF rule:
# they are the exact bitmaps the download email on runswarm.app renders, copied from that
# repository's public/mail/ rather than redrawn, because a product's own mark is not ours to
# approximate. At 192 pixels for a mark drawn about twenty points wide they stay sharp on Retina.
for art in Resources/Spatie*.pdf(N) Resources/SwarmMenuBar.pdf(N) Resources/Maker*.png(N); do
  cp "$art" "$APP/Contents/Resources/"
done

# SwiftTerm and friends ship as dylibs in a debug build; carry them along.
for lib in "$BIN_DIR"/*.dylib(N); do
  cp "$lib" "$APP/Contents/MacOS/"
done

if [[ -d "$BIN_DIR/Swarm_Swarm.bundle" ]]; then
  cp -R "$BIN_DIR/Swarm_Swarm.bundle" "$APP/Contents/Resources/"
fi

# App Intents. Shortcuts and Spotlight do not read the binary: they read a Metadata.appintents
# bundle that Xcode normally produces from constant values the compiler emits while building. A
# Swift package build emits none of that, so intents that compile perfectly are invisible to the
# system. Both halves are reproduced here.
#
# The extraction is its own typecheck pass rather than a flag on `swift build`, because
# -emit-const-values-path names ONE file and is only honoured by a whole-module frontend job: on a
# debug build it is silently dropped, and passing it to `swift build` would hand the same path to
# SwiftTerm and SwarmCore as well. A separate pass over the app target alone costs a few seconds
# and answers about exactly the module that owns the intents.
emit_app_intents_metadata() {
  local toolchain processor sdk deployment triple sources constvalues protocols
  toolchain="$(xcode-select -p 2>/dev/null)/Toolchains/XcodeDefault.xctoolchain"
  processor="$toolchain/usr/bin/appintentsmetadataprocessor"
  protocols="$toolchain/usr/share/swift/SwiftConstantValues/AppIntents.json"

  # Full Xcode only. With just the command line tools there is no processor and no protocol list,
  # and a build that failed over it would be a worse trade than an app whose intents are missing.
  if [[ ! -x "$processor" || ! -f "$protocols" ]]; then
    echo "==> skipping App Intents metadata: $processor not found"
    return 0
  fi

  sdk="$(xcrun --sdk macosx --show-sdk-path)"
  deployment="$(/usr/libexec/PlistBuddy -c 'Print :LSMinimumSystemVersion' Resources/Info.plist)"
  triple="$(uname -m)-apple-macos$deployment"
  sources="$BIN_DIR/Swarm.appintents.sources"
  constvalues="$BIN_DIR/Swarm.swiftconstvalues"

  find Sources/Swarm -name '*.swift' > "$sources"

  # Beside the binary on the old SwiftPM layout. The Xcode build system does not write it
  # there, and failing the whole bundle over missing Shortcuts metadata is worse than an
  # app whose intents are invisible.
  if [[ ! -f "$BIN_DIR/description.json" ]]; then
    echo "==> skipping App Intents metadata: no description.json beside the binary"
    return 0
  fi

  # The frontend wants a bare array of protocol names. The file Xcode ships wraps the same list in
  # an object, which it rejects as malformed.
  local protocolList="$BIN_DIR/Swarm.appintents.protocols.json"
  /usr/bin/python3 -c "import json,sys; json.dump(json.load(open(sys.argv[1]))['constValueProtocols'], open(sys.argv[2],'w'))" \
    "$protocols" "$protocolList"

  # Worktrees take their package identity from their directory, which is not always "swarm".
  # Reuse the actual compiler argument so this pass treats SwarmCore's identifiers as ours too.
  local package_name
  package_name="$(python3 - "$BIN_DIR/description.json" <<'PY'
import json
import sys

with open(sys.argv[1]) as handle:
    commands = json.load(handle)['swiftCommands']
command = next(value for value in commands.values() if value.get('moduleName') == 'Swarm')
arguments = command['otherArguments']
print(arguments[arguments.index('-package-name') + 1])
PY
)"

  swiftc -typecheck -wmo \
    -module-name Swarm \
    -package-name "$package_name" \
    -swift-version 6 \
    -target "$triple" \
    -sdk "$sdk" \
    -I "$BIN_DIR/Modules" \
    -emit-const-values-path "$constvalues" \
    -Xfrontend -const-gather-protocols-file -Xfrontend "$protocolList" \
    "@$sources"

  echo "$constvalues" > "$BIN_DIR/Swarm.appintents.constvalues"

  "$processor" \
    --output "$APP/Contents/Resources" \
    --toolchain-dir "$toolchain" \
    --module-name Swarm \
    --sdk-root "$sdk" \
    --xcode-version "$(xcodebuild -version 2>/dev/null | tail -1 | awk '{print $3}')" \
    --platform-family macOS \
    --deployment-target "$deployment" \
    --target-triple "$triple" \
    --source-file-list "$sources" \
    --swift-const-vals-list "$BIN_DIR/Swarm.appintents.constvalues" \
    --force >/dev/null
}

emit_app_intents_metadata

# After the metadata, because the bundle has to be signed with everything already inside it.
#
# Shortcuts refuses to talk to an ad-hoc signed app: it reaches an intent through an Apple Event
# and the connection is rejected with "Unable to get teamId", so intents that are visible in the
# library fail to run with "Shortcuts couldn't communicate with the app". A real signing identity
# is the only thing that fixes it, and there is no honest default for one, so it is named by the
# environment.
#
#   SWARM_UI_CODESIGN_IDENTITY="Apple Development: You (TEAMID)" ./Tools/build.sh
#
# The pre-rename spelling is still read, so a shell profile or CI job that exports
# BATON_CODESIGN_IDENTITY keeps producing a signed build rather than silently dropping to ad-hoc.
SIGN_IDENTITY="${SWARM_UI_CODESIGN_IDENTITY:-${BATON_CODESIGN_IDENTITY:--}}"
codesign --force --deep --sign "$SIGN_IDENTITY" "$APP" >/dev/null 2>&1 || true
if [[ "$SIGN_IDENTITY" == "-" ]]; then
  echo "==> ad-hoc signed: App Intents will be listed in Shortcuts but will not run."
  echo "    Set SWARM_UI_CODESIGN_IDENTITY to a real identity to make them runnable."
fi

echo "==> $APP"
[[ $RUN -eq 1 ]] && open "$APP"
exit 0
