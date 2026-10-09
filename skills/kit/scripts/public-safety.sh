#!/bin/sh
# Public-safety gate: gitleaks with .gitleaks.toml plus the owner's private words.
#
#   scripts/public-safety.sh           scan the files of HEAD
#   scripts/public-safety.sh --staged  scan the staged change (the pre-commit hook)
#
# The words come from $PRIVATE_PATTERNS (a CI secret) or
# ~/.config/agent-orchestration/private-patterns, one per line, matched as whole words. They
# never enter the repo, and --redact keeps them out of the output.
set -eu

root=$(git rev-parse --show-toplevel)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

words=${PRIVATE_PATTERNS:-}
list=${XDG_CONFIG_HOME:-$HOME/.config}/agent-orchestration/private-patterns
if [ -z "$words" ] && [ -f "$list" ]; then
    words=$(cat "$list")
fi

# The owner's public name and handle may appear (decided 2026-09-28); other uses of the
# same word may not.
allow="['''(?i)priyanshu[ _-]?upadhyay''', '''^\\s*-\\s*Priyanshu\\s*\$''']"  # gitleaks:allow

{
    printf '[extend]\npath = "%s"\n' "$root/.gitleaks.toml"
    printf '%s\n' "$words" | ALLOW=$allow awk '
        /^[[:space:]]*(#|$)/ { next }
        {
            word = $0
            gsub(/^[[:space:]]+|[[:space:]]+$/, "", word)
            gsub(/[][\\.*^$+?(){}|\/]/, "\\\\&", word)
            n++
            printf "\n[[rules]]\nid = \"private-word-%d\"\n", n
            printf "description = \"A word from the owner'"'"'s private list\"\n"
            printf "regex = '"'''"'(?i)\\b%s\\b'"'''"'\n", word
            printf "[[rules.allowlists]]\nregexTarget = \"line\"\nregexes = %s\n", ENVIRON["ALLOW"]
        }'
} > "$work/config.toml"

if [ "${1:-}" = --staged ]; then
    gitleaks git --pre-commit --staged --config "$work/config.toml" --redact --no-banner -v "$root"
else
    mkdir "$work/tree"
    git -C "$root" archive HEAD | tar -x -C "$work/tree"
    gitleaks dir --config "$work/config.toml" --redact --no-banner -v "$work/tree"
fi
