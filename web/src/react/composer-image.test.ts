import { describe, expect, it } from "vitest";
import { pickImage } from "./composer-image";

const file = (name: string, type: string) => new File(["x"], name, { type });

describe("pickImage", () => {
  it("takes a dropped PNG and refuses a video or an SVG with a message", () => {
    expect(pickImage([file("a.png", "image/png")])).toHaveProperty("image");
    expect(pickImage([file("clip.mov", "video/quicktime")])).toHaveProperty("refusal");
    expect(pickImage([file("logo.svg", "image/svg+xml")])).toHaveProperty("refusal");
  });
});
