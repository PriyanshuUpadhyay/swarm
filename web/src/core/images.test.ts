import { describe, expect, it } from "vitest";
import { imagePaths } from "./images";

describe("imagePaths", () => {
  it("finds each pasted image once, and no other path", () => {
    const a = "/Users/me/.swarm/uploads/01a14393-5ca8-7e23-a4ba-1fc23d60c7b7.png";
    const b = "/Users/me/.swarm/uploads/01a14393-5ca8-7e23-a4ba-1fc23d60c7b8.webp";
    expect(imagePaths(`look at ${a} and ${b}, then ${a}`)).toEqual([a, b]);
    expect(imagePaths("/tmp/uploads/shot.png /Users/me/pic.png")).toEqual([]);
  });
});
