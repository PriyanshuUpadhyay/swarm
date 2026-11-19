import { expect, it } from "vitest";
import { parsePatchFiles } from "@pierre/diffs";
import type { PatchLineWire } from "../core/api";
import { patchText } from "./pierre-patch";

it("rebuilds a multi-file patch from display lines, one Pierre file per target", () => {
  const lines: PatchLineWire[] = [
    { kind: "meta", text: "--- a/old-a.ts" },
    { kind: "meta", text: "+++ b/a.ts" },
    // A capped preview keeps a header whose counts no longer match the lines below it.
    { kind: "hunk", text: "@@ -1,9 +1,9 @@" },
    { kind: "del", text: "old-a", old: 1 },
    { kind: "add", text: "new-a", new: 1 },
    { kind: "ctx", text: "same", old: 2, new: 2 },
    { kind: "meta", text: "--- a/b.ts" },
    { kind: "meta", text: "+++ b/b.ts" },
    { kind: "hunk", text: "@@ -4 +4 @@" },
    { kind: "add", text: "new-b", new: 4 },
  ];
  expect(patchText(lines)).toBe([
    "diff --git a/old-a.ts b/a.ts", "--- a/old-a.ts", "+++ b/a.ts", "@@ -1,2 +1,2 @@", "-old-a", "+new-a", " same",
    "diff --git a/b.ts b/b.ts", "--- a/b.ts", "+++ b/b.ts", "@@ -0,0 +4,1 @@", "+new-b", "",
  ].join("\n"));
  const files = parsePatchFiles(patchText(lines)).flatMap((p) => p.files);
  expect(files.map((f) => f.name)).toEqual(["a.ts", "b.ts"]);
});
