import { describe, expect, it } from "vitest";
import type { App } from "./app";
import type { Chat } from "./app-types";
import type { PatchInfo, Row } from "./api";
import { AppStream } from "./app-stream";
import { patchOf, patchRows, textRows } from "./transcript";

// Hand values include file headers, so painted indices differ from list indices.
const diff = {
  lines: [
    { kind: "meta", text: "diff --git a/x b/x" },
    { kind: "meta", text: "--- a/x" },
    { kind: "meta", text: "+++ b/x" },
    { kind: "hunk", text: "@@ -3,2 +3,2 @@" },
    { kind: "ctx", text: "keep", old: 3, new: 3 },
    { kind: "del", text: "old", old: 4 },
    { kind: "add", text: "new", new: 4 },
  ],
  split: [{ h: 3 }, { l: 4, r: 4 }, { l: 5, r: 6 }],
} satisfies PatchInfo;

describe("patch wire", () => {
  it("keeps a frame entry on the preview that the chat paints", () => {
    const upsert: Row = {
      id: "edit", kind: "diff", text: "x", order: [1, 0],
      diff: { id: "patch", path: "x", patch: "server text", added: 1, removed: 1,
        omitted_lines: 0, shortened_lines: 0, full_lines: 7, full_bytes: 80 },
    };
    const chat = { id: "s", key: "s:chair", sessionId: "s", state: "ready", rows: [],
      end: 0, version: 0, shift: false, liveEnded: false } as unknown as Chat;
    const app = {
      chats: new Map([["s", chat]]), openId: "s", frame: 0, notify: () => {},
      pending: new Map([["s:chair", [{ key: "s:chair", end_offset: 10, upsert: [upsert], patch: { patch: diff } }]]]),
    };
    new AppStream(app as unknown as App).flush();
    expect(patchOf(chat.rows[0].diff!)).toEqual(diff);
  });

  it("keeps source indices when unified hides metadata and split pairs lines", () => {
    expect(patchRows(diff, false)).toEqual([{ i: 3 }, { i: 4 }, { i: 5 }, { i: 6 }]);
    expect(patchRows(diff, true)).toEqual([{ h: 3 }, { l: 4, r: 4 }, { l: 5, r: 6 }]);
  });

  it("uses server kinds for text lines and paints plain text without an entry", () => {
    const text = "@@ -1 +1 @@\n--- comment\n+new\n";
    expect(textRows({ kinds: ["hunk", "del", "add", "ctx"] }, text)).toEqual([
      { kind: "hunk", text: "@@ -1 +1 @@" },
      { kind: "del", text: "--- comment" },
      { kind: "add", text: "+new" },
      { kind: "ctx", text: "" },
    ]);
    expect(textRows(undefined, text)).toEqual([
      { kind: null, text: "@@ -1 +1 @@" },
      { kind: null, text: "--- comment" },
      { kind: null, text: "+new" },
      { kind: null, text: "" },
    ]);
  });
});
