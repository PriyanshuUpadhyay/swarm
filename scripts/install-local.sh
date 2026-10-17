#!/bin/sh
# Install an exact ref through the release tap, without waiting for a release.
# Usage: scripts/install-local.sh <tag-or-commit> [--dry-run]
set -eu

die() { printf '%s\n' "install-local: $*" >&2; exit 1; }
dry=false
ref=
for arg in "$@"; do
    case "$arg" in
        --dry-run) dry=true ;;
        -*) die "unknown option $arg" ;;
        *) [ -z "$ref" ] || die 'give one tag or commit'; ref=$arg ;;
    esac
done
[ -n "$ref" ] || die 'usage: scripts/install-local.sh <tag-or-commit> [--dry-run]'
root=$(git -C "$(dirname "$0")/.." rev-parse --show-toplevel)
commit=$(git -C "$root" rev-parse --verify --end-of-options "$ref^{commit}")
short=$(git -C "$root" rev-parse --short "$commit")
version=$(git -C "$root" show "$commit:Cargo.toml" | python3 -c 'import sys,tomllib; print(tomllib.loads(sys.stdin.read())["package"]["version"])')
local_version=$version-pre.local.$short
tap=priyanshuupadhyay/tap
formula=$tap/swarm
plist=$HOME/Library/LaunchAgents/io.github.priyanshuupadhyay.swarm.hud.plist

if "$dry"; then
    printf '%s\n' \
        "Resolve $ref to $commit (package $version; Homebrew $local_version)." \
        'Check macOS arm64, Zig 0.16.0, Node 22, pnpm 11.15.1, and the build tools.' \
        "Create a clean temporary detached worktree at $commit." \
        'Run SWARM_RELEASE_BUILD=1 make -C <worktree>/ui package.' \
        "Run brew tap $tap; refuse a changed tap formula or an active local install." \
        "Save the tap formula; replace it with a file:// tarball formula at version $local_version." \
        "Run brew reinstall --formula $formula if installed, else brew install --formula $formula." \
        'Restore the tap formula, also on failure; the install receipt keeps the real tap.' \
        "If $plist exists, run launchctl kickstart -k gui/$(id -u)/io.github.priyanshuupadhyay.swarm.hud." \
        "Print swarm --version and check that it contains commit $short." \
        'Remove the temporary worktree and build files, also on failure.'
    exit 0
fi

[ "$(uname -s)/$(uname -m)" = Darwin/arm64 ] || die 'the package needs an arm64 Mac'
for tool in brew make zig node pnpm cargo swift jq python3 shasum; do
    command -v "$tool" >/dev/null || die "missing tool $tool"
done
[ "$(zig version)" = 0.16.0 ] || die 'Release needs Zig 0.16.0'
case "$(node --version)" in v22.*) ;; *) die 'Release needs Node 22' ;; esac
[ "$(pnpm --version)" = 11.15.1 ] || die 'Release needs pnpm 11.15.1'
export HOMEBREW_NO_AUTO_UPDATE=1 HOMEBREW_NO_ANALYTICS=1 HOMEBREW_NO_ASK=1
brew tap "$tap"
tap_root=$(brew --repository "$tap")
formula_path=$tap_root/Formula/swarm.rb
[ -f "$formula_path" ] || die "missing $formula_path"
tap_changes=$(git -C "$tap_root" status --porcelain -- Formula/swarm.rb)
[ -z "$tap_changes" ] || die 'the tap formula has local changes'
lock=$tap_root/.swarm-install-local.lock
mkdir "$lock" 2>/dev/null || die "local install lock exists at $lock"
work=
saved=false
cleanup() {
    status=$?
    trap - EXIT HUP INT TERM
    if "$saved"; then
        if cp "$work/swarm.rb" "$formula_path"; then
            saved=false
        else
            printf '%s\n' "install-local: restore failed; original formula is at $work/swarm.rb" >&2
            status=1
        fi
    fi
    if [ -n "$work" ]; then
        if [ -e "$work/tree/.git" ]; then
            # This path belongs to this run; builds leave untracked files in it.
            git -C "$root" worktree remove --force "$work/tree" || status=1
        fi
        if [ ! -e "$work/tree/.git" ] && ! "$saved"; then rm -rf "$work"; fi
    fi
    rmdir "$lock" || status=1
    exit "$status"
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM
work=$(mktemp -d "${TMPDIR:-/tmp}/swarm-install.XXXXXX")
git -C "$root" worktree add --detach "$work/tree" "$commit"
SWARM_RELEASE_BUILD=1 make -C "$work/tree/ui" package
tarball=$work/tree/ui/.build/release/swarm-$version-macos-arm64.tar.gz
[ -f "$tarball" ] || die "package is missing at $tarball"
sha=$(shasum -a 256 "$tarball" | awk '{print $1}')
cp "$formula_path" "$work/swarm.rb"
saved=true
# Keep the real tap in the receipt so an ordinary brew upgrade can replace this build.
# Homebrew sorts a `pre` version below the matching stable release. Reinstall also
# replaces an installed copy with the same package version or a newer local build.
TARBALL=$tarball SHA=$sha LOCAL_VERSION=$local_version python3 - "$formula_path" <<'PY'
import json
import os
from pathlib import Path
import sys

url = Path(os.environ["TARBALL"]).as_uri()
Path(sys.argv[1]).write_text(f'''class Swarm < Formula
  desc "Message bus and pane control for a tree of agent CLIs"
  homepage "https://github.com/PriyanshuUpadhyay/swarm"
  url {json.dumps(url)}
  version {json.dumps(os.environ["LOCAL_VERSION"])}
  sha256 {json.dumps(os.environ["SHA"])}
  license "MIT"
  depends_on arch: :arm64
  depends_on macos: :tahoe
  depends_on "tmux"
  def install
    bin.install "bin/swarm"
    libexec.install Dir["libexec/*"]
  end
end
''')
PY
if [ -n "$(brew list --formula --versions swarm)" ]; then
    brew reinstall --formula "$formula"
else
    brew install --formula "$formula"
fi
cp "$work/swarm.rb" "$formula_path"
saved=false
if [ -f "$plist" ]; then
    launchctl kickstart -k "gui/$(id -u)/io.github.priyanshuupadhyay.swarm.hud"
fi
installed=$(swarm --version)
printf '%s\n' "$installed"
case "$installed" in *"$short"*) ;; *) die "PATH does not run commit $short" ;; esac
