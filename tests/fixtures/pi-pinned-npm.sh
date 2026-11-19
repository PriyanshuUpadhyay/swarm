#!/bin/sh
set -eu
printf 'npm %s\n' "$*" >> "$HOME/calls"
test "$1" = install
test "$2" = --prefix
prefix=$3
shift 3
case "$*" in
  *'--ignore-scripts'*'@tintinweb/pi-subagents@0.26.1'*'@aliou/pi-processes@0.13.6'*) ;;
  *) exit 1 ;;
esac
sub="$prefix/node_modules/@tintinweb/pi-subagents"
proc="$prefix/node_modules/@aliou/pi-processes"
mkdir -p "$sub/src" "$sub/skills" "$proc/skills/pi-processes"
printf '%s\n' '{"name":"@tintinweb/pi-subagents","version":"0.26.1"}' > "$sub/package.json"
printf '%s\n' '{"name":"@aliou/pi-processes","version":"0.13.6"}' > "$proc/package.json"
: > "$sub/src/index.ts"
for entry in processes processes-logs processes-dock; do
  mkdir -p "$proc/extensions/$entry"
  : > "$proc/extensions/$entry/index.ts"
done
