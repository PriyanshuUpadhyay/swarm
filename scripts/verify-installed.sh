# shellcheck shell=sh
# Sourced by release.sh and install-local.sh; it uses the caller's `die`.
# verify_installed <commit>: reload the HUD, check that PATH runs <commit>, and schedule trust cleanup.
verify_installed() {
    # Reload the HUD login item, so the new build starts at once (ADR 0127).
    swarm hud restart
    installed=$(swarm --version)
    printf '%s\n' "$installed"
    printed=$(printf '%s\n' "$installed" | awk 'NR == 1 {print $3}')
    case "$printed" in ''|*[!0-9a-f]*) die 'swarm --version did not print a commit hash' ;; esac
    case "$1" in "$printed"*) ;; *) die "PATH does not run commit $1" ;; esac

    # Cleanup runs in a detached child and never fails the install.
    swarm __prune-stale-trust schedule-upgrade || printf '%s\n' 'swarm: could not schedule trust cleanup' >&2
}
