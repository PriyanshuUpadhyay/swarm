import { describe, expect, it } from "vitest";
import type { Api } from "./api";
import { RunCatalog } from "./run-catalog";

describe("run catalog", () => {
  it("reads each CLI's models once, and again after a failed read", async () => {
    let reads = 0;
    const api = {
      get: async () => {
        reads += 1;
        if (reads === 1) throw new Error("codex is busy");
        return { models: [] };
      },
    } as unknown as Api;
    const runs = new RunCatalog(() => api);
    await expect(runs.models("codex")).rejects.toThrow("codex is busy");
    await runs.models("codex");
    await runs.models("codex");
    expect(reads).toBe(2);
  });

  it("forgets an email that the next accounts read no longer gives", async () => {
    const reads = [
      [{ name: "work", email: "work@example.com" }, { name: "home", email: "home@example.com" }],
      [{ name: "work", email: null }],
    ];
    const api = { get: async () => ({ accounts: reads.shift() }) } as unknown as Api;
    const runs = new RunCatalog(() => api);
    await runs.accounts("claude");
    expect(runs.email("claude", "work")).toBe("work@example.com");
    await runs.accounts("claude");
    expect([runs.email("claude", "work"), runs.email("claude", "home")]).toEqual([null, null]);
  });
});
