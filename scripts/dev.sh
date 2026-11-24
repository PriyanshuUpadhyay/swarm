#!/bin/sh
# Build and check this worktree the way CI does, apart from the owner's live swarm.
# Usage: scripts/dev.sh doctor | build | check [--e2e] [step...]
#   doctor  report what a build needs; change nothing
#   build   web install, web build, cargo build --features serve --bins
#   check   build, then the ci.yml checks in order; --e2e adds background.yml's e2e
#   check <step>...  run only the named steps; the step names are in `step` below
# Each step writes tmp/dev/<step>.log and prints one line. The first failure prints the log's
# end and exits with the step's code. Steps run one at a time, so no step waits on a cargo lock.
set -eu

die() { printf '%s\n' "dev: $*" >&2; exit 1; }
root=$(git -C "$(dirname "$0")/.." rev-parse --show-toplevel)

# A pane exports the owner's live SWARM_HOME, session, agent, adapter and PTY host. Remove them
# so a test or a dev binary cannot reach that session; a dev build then uses ~/.swarm-<branch>
# (ADR 0027). The list comes from env, so a new SWARM_ or HERDR_ variable is removed too.
host_vars=$(env | sed -n 's/^\(SWARM_[A-Za-z0-9_]*\)=.*/\1/p; s/^\(HERDR_[A-Za-z0-9_]*\)=.*/\1/p; s/^\(TMUX\)=.*/\1/p; s/^\(TMUX_PANE\)=.*/\1/p')
for name in $host_vars; do unset "$name"; done

# CI and install-local.sh pin these; Node 24 or another pnpm can build a different web bundle.
pnpm_pin=12.16.1
node_major=22
pinned() {
    [ "$(pnpm --version 2>/dev/null)" = "$pnpm_pin" ] &&
        case "$(node --version 2>/dev/null)" in "v$node_major".*) true ;; *) false ;; esac
}
use_pins() {
    pinned && return 0
    if [ -z "${DEV_SH_PINNED:-}" ] && command -v mise >/dev/null; then
        DEV_SH_PINNED=1 exec mise exec "pnpm@$pnpm_pin" "node@$node_major" -- "$0" "$@"
    fi
    die "need node $node_major and pnpm $pnpm_pin; run: mise install node@$node_major pnpm@$pnpm_pin"
}

logs=$root/tmp/dev
run() {
    name=$1; shift
    mkdir -p "$logs"
    log=$logs/$name.log
    start=$(date +%s)
    if (cd "$root" && "$@") >"$log" 2>&1; then
        printf 'ok    %-13s %5ss  %s\n' "$name" "$(($(date +%s) - start))" "$log"
        return 0
    else
        code=$?
    fi
    printf 'FAIL  %-13s %5ss  %s (exit %s)\n' "$name" "$(($(date +%s) - start))" "$log" "$code"
    tail -n 40 "$log"
    case $(tail -n 40 "$log") in
        *ERR_PNPM_NO_OFFLINE*) printf '%s\n' 'dev: pnpm needs a package that is not in the store; the install needs the network.' ;;
    esac
    exit "$code"
}

step() {
    case $1 in
        web-install) run "$1" pnpm --dir web install --frozen-lockfile --prefer-offline ;;
        web-build) run "$1" pnpm --dir web build ;;
        cargo-build) run "$1" cargo build --features serve --bins ;;
        fmt) run "$1" cargo fmt --check ;;
        clippy) run "$1" cargo clippy --all-targets -- -D warnings ;;
        clippy-serve) run "$1" cargo clippy --all-targets --features serve -- -D warnings ;;
        test) run "$1" cargo test ;;
        test-serve) run "$1" cargo test --features serve -- --include-ignored --skip serve::live_delta::performance_tests:: ;;
        kit) run "$1" sh scripts/kit-checks.sh ;;
        web-typecheck) run "$1" pnpm --dir web typecheck ;;
        web-test) run "$1" pnpm --dir web test ;;
        e2e) run "$1" pnpm --dir web e2e:required ;;
        *) die "unknown step $1; steps: web-install web-build cargo-build fmt clippy clippy-serve test test-serve kit web-typecheck web-test e2e" ;;
    esac
}
build_steps='web-install web-build cargo-build'
check_steps='fmt clippy clippy-serve test test-serve kit web-typecheck web-test'

doctor() {
    ok() { printf 'ok    %s\n' "$*"; }
    fix() { printf 'fix   %s\n' "$*"; }
    branch=$(git -C "$root" symbolic-ref --short -q HEAD || echo HEAD)
    head=$(git -C "$root" rev-parse --short HEAD)
    ok "worktree $root, branch $branch at $head"
    [ "$branch" != main ] || fix 'this is main; the guard blocks edits here, so work in a task worktree'
    if [ -n "$host_vars" ]; then
        ok "removes host variables for every step: $(printf '%s' "$host_vars" | tr '\n' ' ')"
    else
        ok 'no host variables to remove'
    fi

    channel=$(sed -n 's/^channel = "\(.*\)"/\1/p' "$root/rust-toolchain.toml")
    if ! command -v cargo >/dev/null; then
        fix 'cargo is missing; install rustup'
    else
        case $(rustup toolchain list 2>/dev/null) in
            *"$channel"-*) ok "rust $channel" ;;
            *) fix "rust $channel is not installed; the first build installs it, which needs the network" ;;
        esac
    fi
    if pinned; then
        ok "node $(node --version), pnpm $pnpm_pin"
    elif command -v mise >/dev/null && mise exec "pnpm@$pnpm_pin" "node@$node_major" -- node --version >/dev/null 2>&1; then
        ok "node $node_major and pnpm $pnpm_pin through mise (local: node $(node --version 2>/dev/null || echo none), pnpm $(pnpm --version 2>/dev/null || echo none))"
    else
        fix "need node $node_major and pnpm $pnpm_pin; run: mise install node@$node_major pnpm@$pnpm_pin"
    fi
    for tool in tmux ctags; do
        if command -v "$tool" >/dev/null; then ok "$tool"; else fix "$tool is missing; the tests or kit checks need it"; fi
    done
    # macOS /usr/bin/python3 is 3.9; a PATH that puts /usr/bin first fails the kit tests (tomllib).
    if python3 -c 'import sys; sys.exit(sys.version_info < (3, 11))' 2>/dev/null; then
        ok "python3 $(python3 -c 'import sys; print(sys.version.split()[0])')"
    else
        fix "python3 at $(command -v python3 || echo none) is older than 3.11, so the kit tests fail; put a newer python3 first on PATH"
    fi

    if [ -f "$root/web/node_modules/.modules.yaml" ]; then
        ok 'web/node_modules'
    else
        fix 'web/node_modules is missing; run: scripts/dev.sh build'
    fi
    if [ -f "$root/web/dist/index.html" ]; then
        ok 'web/dist'
    else
        fix 'web/dist is missing, so a serve build embeds no UI and e2e fails; run: scripts/dev.sh build'
    fi
    bin=$root/target/debug/swarm
    if [ ! -x "$bin" ]; then
        fix 'target/debug/swarm is missing; run: scripts/dev.sh build'
    else
        version=$("$bin" --version 2>/dev/null || true)
        case " $version " in
            *" $head "*) ok "target/debug/swarm is built from HEAD ($version)" ;;
            *) fix "target/debug/swarm is from another commit ($version); run: scripts/dev.sh build" ;;
        esac
    fi
    playwright=$root/web/node_modules/.bin/playwright
    if [ -x "$playwright" ]; then
        # The first location that the dry run prints is WebKit's, for this platform and version.
        webkit=$("$playwright" install --dry-run webkit 2>/dev/null | sed -n 's/^ *Install location: *//p' | head -n 1)
        if [ -n "$webkit" ] && [ -d "$webkit" ]; then
            ok "playwright $(basename "$webkit")"
        else
            fix "playwright webkit is missing at ${webkit:-an unknown path}, so e2e fails; run: pnpm --dir web exec playwright install webkit"
        fi
    fi

    if [ -e "$root/target/debug/.cargo-lock" ] && holders=$(lsof -t "$root/target/debug/.cargo-lock" 2>/dev/null) && [ -n "$holders" ]; then
        fix "another cargo uses this target dir (pid $(printf '%s' "$holders" | tr '\n' ' ')); a build waits on its lock"
    else
        ok 'no other cargo uses this target dir'
    fi
    free=$(df -Pk "$root" | awk 'NR == 2 { print int($4 / 1048576) }')
    if [ "$free" -lt 10 ]; then
        fix "only $free GiB free; a cold serve build and test need several GiB"
    else
        ok "$free GiB free"
    fi
}

command=${1:-}
[ "$#" -eq 0 ] || shift
case $command in
    doctor)
        [ "$#" -eq 0 ] || die 'usage: scripts/dev.sh doctor'
        doctor
        ;;
    build)
        [ "$#" -eq 0 ] || die 'usage: scripts/dev.sh build'
        use_pins build
        for name in $build_steps; do step "$name"; done
        ;;
    check)
        steps=
        e2e=
        for arg in "$@"; do
            case $arg in
                --e2e) e2e=e2e ;;
                -*) die "unknown option $arg" ;;
                *) steps="$steps $arg" ;;
            esac
        done
        [ -n "$steps" ] || steps="$build_steps $check_steps"
        steps="$steps $e2e"
        use_pins check "$@"
        for name in $steps; do step "$name"; done
        ;;
    *) die 'usage: scripts/dev.sh doctor | build | check [--e2e] [step...]' ;;
esac
