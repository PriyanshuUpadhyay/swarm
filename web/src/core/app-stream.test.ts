import { describe, expect, it } from "vitest";
import type { App } from "./app";
import { AppStream } from "./app-stream";

/** An App stand-in whose snapshot lists `ids`, and lists `later` too after one refresh. */
function fakeApp(ids: string[], later: string[] = []) {
  const calls: string[] = [];
  let listed = ids;
  const app = {
    chatIds: () => listed,
    navigation: { refreshSnapshot: async () => (calls.push("refresh"), (listed = [...ids, ...later])) },
    openChat: (id: string) => calls.push(`open ${id}`),
  };
  return { stream: new AppStream(app as unknown as App), calls };
}

describe("show frame", () => {
  it("opens a listed chat at once", async () => {
    const { stream, calls } = fakeApp(["a1"]);
    await stream.show("a1");
    expect(calls).toEqual(["open a1"]);
  });

  it("reads a newer snapshot for a chat it does not list yet, and opens no unknown chat", async () => {
    const fresh = fakeApp([], ["b2"]);
    await fresh.stream.show("b2");
    expect(fresh.calls).toEqual(["refresh", "open b2"]);
    const gone = fakeApp(["a1"]);
    await gone.stream.show("zz");
    expect(gone.calls).toEqual(["refresh"]);
  });
});
