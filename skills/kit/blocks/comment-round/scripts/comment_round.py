#!/usr/bin/env python3
"""Run one Plannotator round on a set of files, and check that the newest round approved them as they are now.

  comment_round.py run <dir> <file>...     round N: open the files as one review and write <dir>/<N>.json
  comment_round.py check <dir> <file>...   exit 0 when the newest round approved these files and none changed since

A round file is {"files": {path: sha256}, "result": <the plannotator JSON>}. The hashes bind an approval to
the bytes the owner saw, so an edit after approval needs a new round. A round with notes needs
<dir>/<N>-answers.md before the next round starts or the work goes on.
"""

import hashlib
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

MAX_ROUNDS = 3  # the loop stop; after it the owner says in chat which notes are still open
DECISIONS = {"approved", "annotated", "dismissed"}


def digests(folder, files):
    out = {}
    for file in files:
        path = Path(file)
        if "://" in file or not path.is_file():
            # plannotator fetches a URL through an outside reader (Jina) and shows it as Markdown.
            raise SystemExit(f"{file} is not a file; a round reviews files only")
        out[os.path.relpath(path.resolve(), Path(folder).resolve())] = hashlib.sha256(path.read_bytes()).hexdigest()
    return out


def rounds(folder):
    """[(N, round dict)] in order, or SystemExit for a malformed round file."""
    found = []
    for path in Path(folder).glob("*.json"):
        if not path.stem.isdigit():
            continue
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
            ok = isinstance(data["files"], dict) and data["result"]["decision"] in DECISIONS
        except (ValueError, KeyError, TypeError):
            ok = False
        if not ok:
            raise SystemExit(f"{path} is not a round file")
        found.append((int(path.stem), data))
    return sorted(found, key=lambda r: r[0])


def has_notes(result):
    # With --gate, plannotator 0.28.17 puts the notes in `feedback`; other modes use message and annotationCount.
    return bool(result.get("annotationCount")) or any((result.get(k) or "").strip() for k in ("message", "feedback"))


def unanswered(folder, found):
    for n, data in found:
        answers = Path(folder) / f"{n}-answers.md"
        if has_notes(data["result"]) and not (answers.is_file() and answers.read_text(encoding="utf-8").strip()):
            return f"round {n} has notes but no answers in {answers}"
    return None


def run(folder, files):
    if not shutil.which("plannotator"):
        print("unavailable plannotator", file=sys.stderr)
        raise SystemExit(2)
    Path(folder).mkdir(parents=True, exist_ok=True)
    found = rounds(folder)
    if missing := unanswered(folder, found):
        raise SystemExit(missing)
    n = found[-1][0] + 1 if found else 1
    if n > MAX_ROUNDS:
        raise SystemExit(f"loop stop: {MAX_ROUNDS} rounds ran; ask the owner in chat which notes are still open")
    seen = digests(folder, files)
    tmp = Path(folder) / f"{n}.result.tmp"
    tmp.unlink(missing_ok=True)
    out = subprocess.run(["plannotator", "annotate", *files, "--gate", "--json", "--result-file", str(tmp)],
                         stdout=subprocess.PIPE, text=True)
    text = tmp.read_text(encoding="utf-8") if tmp.is_file() else out.stdout
    tmp.unlink(missing_ok=True)
    try:
        result = json.loads(text)
        assert result["decision"] in DECISIONS
    except (ValueError, KeyError, TypeError, AssertionError):
        raise SystemExit(f"plannotator exit {out.returncode} gave no decision JSON: {text[:200]!r}")
    (Path(folder) / f"{n}.json").write_text(json.dumps({"files": seen, "result": result}, indent=2) + "\n", encoding="utf-8")
    print(f"round {n}: {result['decision']}, {'with notes' if has_notes(result) else 'no notes'}")


def check(folder, files):
    found = rounds(folder)
    if not found:
        raise SystemExit(f"no round in {folder}")
    if missing := unanswered(folder, found):
        raise SystemExit(missing)
    n, data = found[-1]
    if data["result"]["decision"] != "approved":
        raise SystemExit(f"round {n} is {data['result']['decision']}, not approved")
    now = digests(folder, files)
    if set(now) != set(data["files"]):
        raise SystemExit(f"round {n} reviewed {', '.join(sorted(data['files']))}, not these files")
    if changed := sorted(f for f in now if now[f] != data["files"][f]):
        raise SystemExit(f"{changed[0]} changed after round {n}; start a new round")
    print(f"round {n} approved these files")


def main(argv):
    if len(argv) < 3 or argv[0] not in ("run", "check"):
        raise SystemExit(__doc__.strip().splitlines()[2].strip() + "\n" + __doc__.strip().splitlines()[3].strip())
    (run if argv[0] == "run" else check)(argv[1], argv[2:])


if __name__ == "__main__":
    main(sys.argv[1:])
