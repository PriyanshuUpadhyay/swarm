#!/bin/sh
set -eu
printf 'npm %s\n' "$*" >> "$HOME/calls"
test "$1" = install
test "$2" = --prefix
prefix=$3
shift 3
case "$*" in
  *'--ignore-scripts'*'@aliou/pi-processes@0.13.6'*) ;;
  *) exit 1 ;;
esac
proc="$prefix/node_modules/@aliou/pi-processes"
mkdir -p "$proc/skills/pi-processes"
printf '%s\n' '{"name":"@aliou/pi-processes","version":"0.13.6"}' > "$proc/package.json"
for entry in processes processes-logs processes-dock; do
  mkdir -p "$proc/extensions/$entry"
  : > "$proc/extensions/$entry/index.ts"
done
