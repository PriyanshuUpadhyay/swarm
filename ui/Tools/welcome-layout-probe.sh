#!/bin/zsh
# Runs the welcome transition regression in an invisible, isolated bundle after swift build.
set -euo pipefail
cd "$(dirname "$0")/.."

bin_dir="$(swift build --show-bin-path)"
probe_root="$(mktemp -d "${TMPDIR:-/tmp}/swarm-welcome-probe.XXXXXX")"
probe_app="$probe_root/Swarm Welcome Probe.app"
mkdir -p "$probe_app/Contents/MacOS"
cp "$bin_dir/Swarm" "$probe_app/Contents/MacOS/Swarm"
cp Resources/Info.plist "$probe_app/Contents/Info.plist"
ditto Resources "$probe_app/Contents/Resources"
/usr/libexec/PlistBuddy -c 'Set :CFBundleIdentifier io.github.priyanshuupadhyay.swarm.welcome-probe' "$probe_app/Contents/Info.plist"
/usr/libexec/PlistBuddy -c 'Set :CFBundleName Swarm Welcome Probe' "$probe_app/Contents/Info.plist"
codesign --force --deep --sign - "$probe_app" >/dev/null 2>&1

# Refuse a release or stale binary, which would ignore the flag and start the application.
python3 - "$probe_app/Contents/MacOS/Swarm" "$probe_root" <<'PY'
import json
import pathlib
import subprocess
import sys

binary, root = sys.argv[1:]
if b'--welcome-layout-probe' not in pathlib.Path(binary).read_bytes():
    raise SystemExit('Build the debug app with swift build before running this probe.')
for scenario in ('all-clear', 'no-agent', 'signed-out-github'):
    subprocess.run(
        ['open', '-g', '-n', '-W', '-a', str(pathlib.Path(binary).parents[2]),
         '--stdout', f'{root}/{scenario}.json', '--stderr', f'{root}/{scenario}.log',
         '--args', '--welcome-layout-probe', '--setup-rehearsal', scenario],
        timeout=45, check=True,
    )
    report = pathlib.Path(root, f'{scenario}.json').read_text()
    print(f'{scenario}: {report}')
    if not json.loads(report)['passed']:
        raise SystemExit(f'{scenario} failed; evidence: {root}')
print(f'Probe evidence: {root}')
PY
