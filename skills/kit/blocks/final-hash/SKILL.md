---
name: final-hash
description: "Record the hash of each handed-off file after its last edit, so the step closes only on the bytes that were checked. Use when a build step hands off a page, a film, or another file."
license: MIT
metadata:
  kind: step
  owns: "that the handed-off files are the bytes the critic and the checks saw"
  serves: "build"
  needs: "the deliverable files in the piece folder"
  gives: "{{site}}/handoff.sha256"
  conflicts: ""
  users: "build"
  source: "presets/build/PRESET.md (Preset, 07-export row); docs council 2026-11-02 kit-right-size (U2: the final page was not checked on its final hash)"
---

## Rules
- After the last edit, and before the end critic and the handoff, write `{{site}}/handoff.sha256` from inside `{{site}}` with `shasum -a 256 <file>...`, one line for each handed-off file, with paths relative to `{{site}}`.
- Send the end critic the frames of those bytes, and quote the hash line of the page in the step's result.
- Any edit after that changes a hash, so the step does not close. Write the file again only after the critic has seen the new bytes.

## Example
Initial state: `07-export` hands off `index.html` and `loop.mp4`.
Action: in the piece folder, run `shasum -a 256 index.html loop.mp4 > handoff.sha256`, then send the end critic frames rendered from that `index.html`.
Result: a later one-line CSS fix makes `done` refuse until the critic sees the new frames and the file is written again.

## Check (script)
- `cd {{site}} && shasum -a 256 -c handoff.sha256 >/dev/null` exits 0: each handed-off file still has the hash recorded after its last edit.

## Check (judge)
- Do the end critic's frames come from the bytes in `handoff.sha256`? Quote the hash line of the page and the critic's verdict line.
