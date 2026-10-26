#!/bin/sh
# Release from a checked, clean main branch, or finish a pushed release.
# Usage: scripts/release.sh patch|minor|major [--no-local] [--dry-run]
# After the tag is pushed, it installs that tag through install-local.sh, unless --no-local is given.
#        scripts/release.sh resume v<version> [--dry-run]
set -eu

die() { printf '%s\n' "release: $*" >&2; exit 1; }
dry=false
local_install=true
mode=
tag=
for arg in "$@"; do
    case "$arg" in
        --dry-run) dry=true ;;
        --no-local) local_install=false ;;
        patch|minor|major|resume) [ -z "$mode" ] || die 'give one release mode'; mode=$arg ;;
        v*) [ "$mode" = resume ] && [ -z "$tag" ] || die "unexpected tag $arg"; tag=$arg ;;
        *) die "unknown argument $arg" ;;
    esac
done
[ -n "$mode" ] || die 'usage: scripts/release.sh patch|minor|major [--no-local] [--dry-run], or resume v<version>'
root=$(git -C "$(dirname "$0")/.." rev-parse --show-toplevel)
if [ "$mode" = resume ]; then
    "$local_install" || die 'resume does not accept --no-local'
    local_install=false  # resume finishes a pushed tag; the Tap step installs it
    python3 - "$tag" <<'PYTAG'
import re
import sys
if not re.fullmatch(r"v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", sys.argv[1]):
    sys.exit("release: resume needs a stable version tag, such as v0.0.20")
PYTAG
    version=${tag#v}
else
    bump=$mode
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
fi
if "$dry"; then
    if [ "$mode" = resume ]; then
        printf '%s\n' "Resume $tag after the push; keep the version, commit, and tag."
    else
        printf '%s\n' \
            "New version $version ($tag)." \
            'Require main and no changes outside tmp/.' \
            'Run git fetch origin; require HEAD = origin/main and no v* tag at HEAD.' \
            'Wait up to 30 minutes for the latest ci push run for HEAD; require success.' \
            'Warn when the last completed background run on main did not succeed.' \
            "Require tag $tag to be absent locally and on origin." \
            "Set Cargo.toml and the swarm Cargo.lock entry to $version." \
            'Run cargo check --locked.' \
            "Commit release $version; create annotated tag $tag with message $tag." \
            "Run git push --atomic origin main refs/tags/$tag."
        if "$local_install"; then printf '%s\n' "Run $root/scripts/install-local.sh $tag; warn and continue if it fails."; fi
    fi
    printf '%s\n' \
        'Wait up to 30 minutes for Release; retry read errors within that window.' \
        "If $tag is a draft, run gh release edit $tag --draft=false --latest." \
        'Wait up to 30 minutes for Tap, using the saved publication time.' \
        'Run brew update, then brew upgrade swarm; reinstall if a .local build remains.' \
        'Restart the HUD if its LaunchAgent plist exists.' \
        'Print swarm --version and check the printed commit as a prefix of the full release hash.' \
        'Run swarm __prune-stale-trust schedule-upgrade; log scheduling errors and continue.'
    exit 0
fi

for tool in gh jq brew python3; do
    command -v "$tool" >/dev/null || die "missing tool $tool"
done
if [ "$mode" != resume ]; then command -v cargo >/dev/null || die 'missing tool cargo'; fi
export HOMEBREW_NO_ASK=1
repo=$(gh repo view "$(git -C "$root" remote get-url origin)" --json nameWithOwner --jq .nameWithOwner)

# Retry only reads, within one deadline. Failed reads back off with jitter.
wait_run() {
    workflow=$1
    event=$2
    since=$3
    deadline=$(($(date +%s) + 1800))
    delay=2
    while [ "$(date +%s)" -lt "$deadline" ]; do
        if ! runs=$(gh run list -R "$repo" --workflow "$workflow" --commit "$head" --event "$event" \
            --limit 20 --json databaseId,status,conclusion,createdAt); then
            now=$(date +%s)
            [ "$now" -lt "$deadline" ] || break
            pause=$((delay + now % 3))
            remaining=$((deadline - now))
            [ "$pause" -le "$remaining" ] || pause=$remaining
            sleep "$pause"
            [ "$delay" -ge 30 ] || delay=$((delay * 2))
            [ "$delay" -le 30 ] || delay=30
            continue
        fi
        delay=2
        run=$(printf '%s' "$runs" | jq -c --arg since "$since" '[.[] | select(.createdAt >= $since)] | .[0] // empty')
        if [ -n "$run" ]; then
            if [ "$(printf '%s' "$run" | jq -r .status)" = completed ]; then
                conclusion=$(printf '%s' "$run" | jq -r .conclusion)
                run_id=$(printf '%s' "$run" | jq -r .databaseId)
                if [ "$conclusion" != success ]; then
                    next_command="gh run rerun $run_id -R $repo"
                    die "$workflow run $run_id ended with $conclusion"
                fi
                printf '%s\n' "$workflow run $run_id succeeded"
                return
            fi
        fi
        sleep 10
    done
    die "timed out waiting for $workflow at $head"
}

if [ "$mode" = resume ]; then
    head=$(git -C "$root" rev-parse --verify --end-of-options "$tag^{commit}") || die "cannot resolve $tag"
    git -C "$root" ls-remote --exit-code --tags origin "refs/tags/$tag" >/dev/null ||
        die "tag $tag is not on origin; push it with git -C \"$root\" push --atomic origin main refs/tags/$tag, then resume"
    started=$(TZ=UTC git -C "$root" show -s --date=format-local:%Y-%m-%dT%H:%M:%SZ --format=%cd "$head")
else
    [ "$(git -C "$root" branch --show-current)" = main ] || die 'run this from main'
    changes=$(git -C "$root" status --porcelain --untracked-files=all -- . ':(exclude)tmp')
    [ -z "$changes" ] || die 'the tree has changes outside tmp/'
    git -C "$root" fetch origin
    head=$(git -C "$root" rev-parse HEAD)
    head_tags=$(git -C "$root" tag --points-at HEAD 'v*')
    if [ -n "$head_tags" ]; then
        for pushed_tag in $head_tags; do
            printf '%s\n' "release: HEAD already has $pushed_tag; resume with $root/scripts/release.sh resume $pushed_tag" >&2
        done
        exit 1
    fi
    [ "$head" = "$(git -C "$root" rev-parse origin/main)" ] || die 'HEAD differs from origin/main'
    wait_run ci.yml push ''
    # The background checks do not gate a release; a failure there is fixed before the next one.
    background=$(gh run list -R "$repo" --workflow background.yml --branch main --status completed --limit 1 \
        --json conclusion,url --jq '.[] | select(.conclusion != "success") | .url') || background=
    [ -z "$background" ] || printf '%s\n' "release: warning: the last background run on main failed: $background" >&2
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
    cargo check --manifest-path "$root/Cargo.toml" --locked || die "cargo check failed; inspect the edits, then restore with git -C \"$root\" checkout -- Cargo.toml Cargo.lock"
    # Leave failed checks for inspection; never reset owner work.
    [ "$(git -C "$root" rev-parse HEAD)" = "$head" ] || die 'HEAD changed during the check'
    git -C "$root" add -- Cargo.toml Cargo.lock
    git -C "$root" commit -m "release $version" -- Cargo.toml Cargo.lock
    git -C "$root" tag -a "$tag" -m "$tag"
    head=$(git -C "$root" rev-parse HEAD)
    started=$(date -u +%Y-%m-%dT%H:%M:%SZ)
    git -C "$root" push --atomic origin main "refs/tags/$tag" || die "push failed; check origin before retrying. If nothing was pushed, use git -C \"$root\" tag -d $tag, then git -C \"$root\" reset --keep origin/main"
fi

# Every failure after the push names a command that can finish this tag.
resume_command="$root/scripts/release.sh resume $tag"
next_command=$resume_command
release_exit() {
    status=$?
    trap - EXIT HUP INT TERM
    if [ "$status" -ne 0 ]; then
        printf '%s\n' "release: $tag is pushed. Next command: $next_command" >&2
        if [ "$next_command" != "$resume_command" ]; then
            printf '%s\n' "release: then run $resume_command" >&2
        fi
    fi
    exit "$status"
}
trap release_exit EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM
if "$local_install"; then
    "$root/scripts/install-local.sh" "$tag" || printf '%s\n' 'release: local install failed; the release continues' >&2
fi

wait_run release.yml push "$started"
release=$(gh release view "$tag" -R "$repo" --json isDraft,publishedAt)
if [ "$(printf '%s' "$release" | jq -r .isDraft)" = true ]; then
    gh release edit "$tag" -R "$repo" --draft=false --latest
    release=$(gh release view "$tag" -R "$repo" --json isDraft,publishedAt)
fi
published=$(printf '%s' "$release" | jq -r '.publishedAt // empty')
[ -n "$published" ] || die "release $tag has no publication time"
wait_run tap.yml release "$published"
brew update
brew upgrade swarm
# A local build sorts above the release of the same tag, so upgrade keeps it.
installed_versions=$(brew list --formula --versions swarm)
case "$installed_versions" in
    *"$version.local."*) brew reinstall --formula priyanshuupadhyay/tap/swarm ;;
esac
plist=$HOME/Library/LaunchAgents/io.github.priyanshuupadhyay.swarm.hud.plist
# SIGTERM, so KeepAlive starts the new HUD; `kickstart -k` waits out the 60 s ThrottleInterval.
# A HUD that does not run is started.
hud_target=gui/$(id -u)/io.github.priyanshuupadhyay.swarm.hud
if [ -f "$plist" ]; then launchctl kill SIGTERM "$hud_target" 2>/dev/null || launchctl kickstart "$hud_target"; fi
installed=$(swarm --version)
printf '%s\n' "$installed"
printed=$(printf '%s\n' "$installed" | awk 'NR == 1 {print $3}')
case "$printed" in ''|*[!0-9a-f]*) die 'swarm --version did not print a commit hash' ;; esac
case "$head" in "$printed"*) ;; *) die "PATH does not run release commit $head" ;; esac

# Cleanup runs in a detached child and never fails the install.
swarm __prune-stale-trust schedule-upgrade || printf '%s\n' 'swarm: could not schedule trust cleanup' >&2
