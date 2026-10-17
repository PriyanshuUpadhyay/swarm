#!/bin/sh
# Release from a checked, clean main branch. The owner runs this on purpose.
# Usage: scripts/release.sh patch|minor|major [--local] [--dry-run]
set -eu

die() { printf '%s\n' "release: $*" >&2; exit 1; }
dry=false
local_install=false
bump=
for arg in "$@"; do
    case "$arg" in
        --dry-run) dry=true ;;
        --local) local_install=true ;;
        patch|minor|major) [ -z "$bump" ] || die 'give one version bump'; bump=$arg ;;
        *) die "unknown argument $arg" ;;
    esac
done
[ -n "$bump" ] || die 'usage: scripts/release.sh patch|minor|major [--local] [--dry-run]'
root=$(git -C "$(dirname "$0")/.." rev-parse --show-toplevel)
version=$(python3 - "$root/Cargo.toml" "$bump" <<'PY'
import re
import sys
import tomllib
with open(sys.argv[1], "rb") as source:
    old = tomllib.load(source)["package"]["version"]
if not re.fullmatch(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", old):
    sys.exit("release: expected a stable semantic version in Cargo.toml")
major, minor, patch = map(int, old.split("."))
if sys.argv[2] == "major":
    major, minor, patch = major + 1, 0, 0
elif sys.argv[2] == "minor":
    minor, patch = minor + 1, 0
else:
    patch += 1
print(f"{major}.{minor}.{patch}")
PY
)
tag=v$version
if "$dry"; then
    printf '%s\n' \
        "New version $version ($tag)." \
        'Require main and no changes outside tmp/.' \
        'Run git fetch origin; require HEAD = origin/main.' \
        'Require the latest ci run for HEAD to have completed with success (gh run list --commit).' \
        "Require tag $tag to be absent locally and on origin." \
        "Set Cargo.toml and the swarm Cargo.lock entry to $version." \
        'Run cargo check --locked.' \
        "Commit release $version; create annotated tag $tag with message $tag." \
        "Run git push --atomic origin main refs/tags/$tag."
    if "$local_install"; then printf '%s\n' "Run $root/scripts/install-local.sh $tag."; fi
    printf '%s\n' \
        'Wait up to 30 minutes for the new Release run; stop unless it succeeds.' \
        "Run gh release edit $tag --draft=false --latest." \
        'Wait up to 30 minutes for the new Tap run; stop unless it succeeds.' \
        'Run brew update, then brew upgrade swarm.' \
        'Restart the HUD if its LaunchAgent plist exists.' \
        'Print swarm --version and check the release commit.'
    exit 0
fi

for tool in gh jq cargo brew python3; do
    command -v "$tool" >/dev/null || die "missing tool $tool"
done
export HOMEBREW_NO_ASK=1
[ "$(git -C "$root" branch --show-current)" = main ] || die 'run this from main'
changes=$(git -C "$root" status --porcelain --untracked-files=all -- . ':(exclude)tmp')
[ -z "$changes" ] || die 'the tree has changes outside tmp/'
git -C "$root" fetch origin
head=$(git -C "$root" rev-parse HEAD)
[ "$head" = "$(git -C "$root" rev-parse origin/main)" ] || die 'HEAD differs from origin/main'
repo=$(gh repo view "$(git -C "$root" remote get-url origin)" --json nameWithOwner --jq .nameWithOwner)
ci=$(gh run list -R "$repo" --workflow ci.yml --commit "$head" --limit 1 --json status,conclusion)
[ "$(printf '%s' "$ci" | jq -r '.[0] | .status + "/" + .conclusion')" = completed/success ] || die 'the latest ci run for HEAD has not succeeded'
if git -C "$root" show-ref --verify --quiet "refs/tags/$tag"; then die "tag $tag already exists"; fi
remote_tag=$(git -C "$root" ls-remote --tags origin "refs/tags/$tag")
[ -z "$remote_tag" ] || die "tag $tag already exists on origin"

python3 - "$root" "$version" <<'PY'
from pathlib import Path
import re
import sys
import tomllib

root = Path(sys.argv[1])
version = sys.argv[2]
manifest = root / "Cargo.toml"
lock = root / "Cargo.lock"
manifest_text = manifest.read_text()
lock_text = lock.read_text()
old = tomllib.loads(manifest_text)["package"]["version"]
packages = [p for p in tomllib.loads(lock_text)["package"] if p["name"] == "swarm"]
if len(packages) != 1 or packages[0]["version"] != old:
    sys.exit("release: Cargo.lock must contain one matching swarm package")
manifest_new, count = re.subn(
    rf'(?ms)(^\[package\]\s*\n.*?^version\s*=\s*"){re.escape(old)}(")',
    lambda m: m[1] + version + m[2], manifest_text, count=1,
)
if count != 1:
    sys.exit("release: cannot locate the package version")
lock_new, count = re.subn(
    rf'(?m)(^name = "swarm"\nversion = "){re.escape(old)}(")',
    lambda m: m[1] + version + m[2], lock_text,
)
if count != 1:
    sys.exit("release: cannot locate the swarm lock version")
manifest.write_text(manifest_new)
lock.write_text(lock_new)
PY
cargo check --manifest-path "$root/Cargo.toml" --locked
# A failed check leaves the version edits for inspection; do not reset owner work.
[ "$(git -C "$root" rev-parse HEAD)" = "$head" ] || die 'HEAD changed during the check'
git -C "$root" add -- Cargo.toml Cargo.lock
git -C "$root" commit -m "release $version" -- Cargo.toml Cargo.lock
git -C "$root" tag -a "$tag" -m "$tag"
head=$(git -C "$root" rev-parse HEAD)
started=$(date -u +%Y-%m-%dT%H:%M:%SZ)
git -C "$root" push --atomic origin main "refs/tags/$tag"
if "$local_install"; then "$root/scripts/install-local.sh" "$tag"; fi

# Poll reads only. Never retry a push or a release publication.
wait_run() {
    workflow=$1
    event=$2
    since=$3
    deadline=$(($(date +%s) + 1800))
    while [ "$(date +%s)" -lt "$deadline" ]; do
        runs=$(gh run list -R "$repo" --workflow "$workflow" --commit "$head" --event "$event" \
            --limit 20 --json databaseId,status,conclusion,createdAt)
        run=$(printf '%s' "$runs" | jq -c --arg since "$since" '[.[] | select(.createdAt >= $since)] | .[0] // empty')
        if [ -n "$run" ]; then
            if [ "$(printf '%s' "$run" | jq -r .status)" = completed ]; then
                conclusion=$(printf '%s' "$run" | jq -r .conclusion)
                run_id=$(printf '%s' "$run" | jq -r .databaseId)
                [ "$conclusion" = success ] || die "$workflow run $run_id ended with $conclusion"
                printf '%s\n' "$workflow run $run_id succeeded"
                return
            fi
        fi
        sleep 10
    done
    die "timed out waiting for $workflow at $head"
}
wait_run release.yml push "$started"
published=$(date -u +%Y-%m-%dT%H:%M:%SZ)
gh release edit "$tag" -R "$repo" --draft=false --latest
wait_run tap.yml release "$published"
brew update
brew upgrade swarm
plist=$HOME/Library/LaunchAgents/io.github.priyanshuupadhyay.swarm.hud.plist
if [ -f "$plist" ]; then launchctl kickstart -k "gui/$(id -u)/io.github.priyanshuupadhyay.swarm.hud"; fi
installed=$(swarm --version)
printf '%s\n' "$installed"
short=$(git -C "$root" rev-parse --short HEAD)
case "$installed" in *"$short"*) ;; *) die "PATH does not run release commit $short" ;; esac
