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
    const { stream, calls } = fakeApp(["listed"]);
    await stream.show("listed");
    expect(calls).toEqual(["open listed"]);
  });

  it("reads a newer snapshot for a chat it does not list yet, and opens no unknown chat", async () => {
    const fresh = fakeApp([], ["newcomer"]);
    await fresh.stream.show("newcomer");
    expect(fresh.calls).toEqual(["refresh", "open newcomer"]);
    const gone = fakeApp(["listed"]);
    await gone.stream.show("stranger");
    expect(gone.calls).toEqual(["refresh"]);
  });

  it("a show frame from the stream opens the chat, and one with no id does nothing", async () => {
    const { stream, calls } = fakeApp(["listed"]);
    stream.onSse({ event: "show", data: JSON.stringify({ session: "listed" }) });
    stream.onSse({ event: "show", data: "{}" });
    await Promise.resolve();
    expect(calls).toEqual(["open listed"]);
  });
});
