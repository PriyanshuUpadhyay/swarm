import { describe, expect, it } from "vitest";
import { chordLabel } from "./chord";

describe("chordLabel", () => {
  it("writes modifiers in Mac order with glyphs", () => {
    expect(chordLabel("Ctrl+Meta+ArrowDown")).toBe("⌃⌘↓");
    expect(chordLabel("Meta+Shift+]")).toBe("⇧⌘]");
    expect(chordLabel("Shift+Meta+[")).toBe("⇧⌘[");
  });
  it("upper-cases a letter and keeps a bare key", () => {
    expect(chordLabel("Meta+k")).toBe("⌘K");
    expect(chordLabel("Meta+K")).toBe("⌘K");
    expect(chordLabel("j")).toBe("J");
    expect(chordLabel("Enter")).toBe("↩");
  });
});
