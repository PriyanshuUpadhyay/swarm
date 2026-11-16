import { describe, expect, it } from "vitest";
import type { Block, Display, Inline } from "./api";
import { fold } from "./fold-display";
import { units } from "./transcript";

const t = (value: string): Inline => ({ type: "text", value });
const para = (...value: Inline[]): Block => ({ type: "paragraph", value });
const filler = "It adds more words that a skimming reader does not need to see at first.";

// A reply shaped like a review result: an opening line, a section with a decision, and its options.
const reply: Display = {
  blocks: [
    para(t(`Round 4 found 3 problems, and I fixed them in commit `), { type: "code", value: "650cc86b" }, t(`. ${filler} ${filler}`)),
    { type: "heading", value: { depth: 2, children: [t("Decision 2. A reply can wait behind queued text")] } },
    para(t(`A CLI that waits for its cursor position can time out, e.g. after 4.5 s. ${filler} ${filler}`)),
    {
      type: "list",
      value: {
        ordered: false,
        start: null,
        tight: true,
        items: [
          [para({ type: "strong", value: [t("Option A accepts this as a known limit.")] }, t(` ${filler}`))],
          [para({ type: "strong", value: [t("Option B gives the answers their own queue.")] }, t(` ${filler} ${filler}`))],
        ],
      },
    },
  ],
};

describe("fold", () => {
  it("keeps each first sentence at Points, and the headings with one line each at Outline", () => {
    const points = units(fold(reply, "points")!);
    expect(points).toBe(
      "Round 4 found 3 problems, and I fixed them in commit 650cc86b. …" +
        "Decision 2. A reply can wait behind queued text" +
        "A CLI that waits for its cursor position can time out, e.g. after 4.5 s. …" +
        "Option A accepts this as a known limit. …" +
        "Option B gives the answers their own queue. …",
    );
    const outline = units(fold(reply, "outline")!);
    expect(outline).toBe(
      "Round 4 found 3 problems, and I fixed them in commit 650cc86b. …" +
        "Decision 2. A reply can wait behind queued text" +
        "A CLI that waits for its cursor position can time out, e.g. after 4.5 s. …",
    );
  });

  it("offers no fold for a short reply", () => {
    expect(fold({ blocks: [para(t("Done. The tests pass."))] }, "points")).toBeNull();
  });
});
