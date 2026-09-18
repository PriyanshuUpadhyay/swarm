#!/bin/zsh

set -euo pipefail

python3 - "$@" <<'PY'
import datetime
import glob
import json
import os
import statistics
import sys

if len(sys.argv) > 1:
    paths = sys.argv[1:]
else:
    root = os.path.expanduser("~/Library/Application Support/Swarm/diagnostics")
    today = datetime.date.today()
    names = [f"perf-{today - datetime.timedelta(days=offset):%Y-%m-%d}.jsonl" for offset in range(7)]
    paths = [os.path.join(root, name) for name in names]

events = []
for path in paths:
    if not os.path.isfile(path):
        continue
    with open(path, encoding="utf-8") as stream:
        for number, line in enumerate(stream, 1):
            try:
                event = json.loads(line)
            except json.JSONDecodeError as error:
                print(f"{path}:{number}: invalid JSON: {error}", file=sys.stderr)
                continue
            if isinstance(event, dict):
                events.append(event)

print(f"{'kind':<18} {'count':>7} {'median ms':>12} {'worst ms':>12}")
print(f"{'-' * 18} {'-' * 7} {'-' * 12} {'-' * 12}")
for kind in sorted({str(event.get('kind', 'unknown')) for event in events}):
    group = [event for event in events if str(event.get("kind", "unknown")) == kind]
    durations = [float(event["ms"]) for event in group if isinstance(event.get("ms"), (int, float))]
    median = f"{statistics.median(durations):.1f}" if durations else "-"
    worst = f"{max(durations):.1f}" if durations else "-"
    print(f"{kind:<18} {len(group):>7} {median:>12} {worst:>12}")

print("\nten worst events")
worst = sorted(
    (event for event in events if isinstance(event.get("ms"), (int, float))),
    key=lambda event: float(event["ms"]),
    reverse=True,
)[:10]
for event in worst:
    detail = json.dumps(event.get("detail", {}), sort_keys=True, separators=(",", ":"))
    print(f"{float(event['ms']):9.1f} ms  {str(event.get('kind', 'unknown')):<18} {event.get('at', '')} {detail}")
PY
