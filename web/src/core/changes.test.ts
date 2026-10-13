import { describe, expect, it } from "vitest";
import { checkState, commentMessage, commentRange, defaultBase, diffLines, safeUrl, type DiffComment } from "./changes";

const patch = ["diff --git a/src/app.ts b/src/app.ts", "--- a/src/app.ts", "+++ b/src/app.ts", "@@ -10,4 +10,4 @@ class App", " keep", "-old one", "-old two", "+new one", " tail"].join("\n");

describe("diff line comments", () => {
  const lines = diffLines(patch);

  it("drops the file headers, so display lines start at the hunk", () => {
    expect(lines.map((l) => l.kind)).toEqual(["hunk", "ctx", "del", "del", "add", "ctx"]);
  });

  it("names one line by its new number and quotes it with its mark", () => {
    expect(commentRange(lines, 4, 4)).toEqual({ lines: "line 11", quoted: ["+new one"] });
  });

  it("names a range by new numbers and quotes removed lines too", () => {
    expect(commentRange(lines, 5, 1)).toEqual({ lines: "lines 10-12", quoted: [" keep", "-old one", "-old two", "+new one", " tail"] });
  });

  it("names a range of only removed lines by old numbers", () => {
    expect(commentRange(lines, 2, 3)).toEqual({ lines: "old lines 11-12", quoted: ["-old one", "-old two"] });
  });

  it("gives no label for a hunk header alone", () => {
    expect(commentRange(lines, 0, 0).lines).toBe("");
  });

  it("puts every draft in one message: file, lines, quote, comment", () => {
    const at = { id: 1, layer: "unstaged", at: 0 } as const;
    const comments: DiffComment[] = [
      { ...at, path: "src/app.ts", lines: "line 11", quoted: ["+new one"], text: "Rename this." },
      { ...at, id: 2, path: "README.md", lines: "lines 3-4", quoted: [" a", "+b"], text: "Explain why." },
    ];
    expect(commentMessage(comments)).toBe(
      [
        "Review comments on the diff (2):",
        "",
        "src/app.ts (line 11):\n> +new one\nRename this.",
        "",
        "README.md (lines 3-4):\n>  a\n> +b\nExplain why.",
      ].join("\n"),
    );
  });
});

describe("branch base", () => {
  it("prefers the remote default branch, then main or master", () => {
    expect(defaultBase(["refs/heads/feature", "refs/heads/main", "refs/remotes/origin/main"], "feature")).toBe("refs/remotes/origin/main");
    expect(defaultBase(["refs/heads/feature", "refs/heads/master"], "feature")).toBe("refs/heads/master");
  });

  it("never compares a branch with itself", () => {
    expect(defaultBase(["refs/heads/main"], "main")).toBeNull();
    expect(defaultBase(["refs/heads/main", "refs/heads/topic"], "main")).toBe("refs/heads/topic");
  });
});

describe("checks", () => {
  it("reads CheckRun conclusions and StatusContext states", () => {
    expect(checkState({ status: "COMPLETED", conclusion: "SUCCESS" })).toBe("passed");
    expect(checkState({ status: "COMPLETED", conclusion: "FAILURE" })).toBe("failed");
    expect(checkState({ status: "IN_PROGRESS", conclusion: "" })).toBe("pending");
    expect(checkState({ state: "ERROR" })).toBe("failed");
    expect(checkState({ status: "COMPLETED", conclusion: "SKIPPED" })).toBe("skipped");
  });

  it("opens only http(s) links", () => {
    expect(safeUrl("https://github.com/o/r/actions/runs/1")).toBe("https://github.com/o/r/actions/runs/1");
    expect(safeUrl("javascript:alert(1)")).toBeNull();
    expect(safeUrl(undefined)).toBeNull();
  });
});
