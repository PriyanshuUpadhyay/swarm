// The run chip: its label, the outcome of a draft, and when Apply is blocked.

import { describe, expect, it } from "vitest";
import { chair, session } from "./chat-fixtures";
import { agentInput, applyBlock, cliName, outcomeText, runLabel, switchOutcome } from "./composer";


describe("run chip", () => {
  const run = (provider: string, model: string | null, effort: string | null = null, account: string | null = null) => ({ provider, model, effort, account });

  it("names the account, model and thinking, and the CLI where no account is", () => {
    expect(runLabel(agentInput(session(), [chair({ provider: "claude", model: "claude-sonnet-5-5", effort: "high", account: "work" })]))).toBe("work · claude-sonnet-5-5 · high");
    expect(runLabel(agentInput(session({ chair_provider: "pi" }), [chair({ provider: "pi" })]))).toBe("Pi · Default model");
  });

  it("resumes inside one CLI and hands off to another CLI or Codex account", () => {
    expect(switchOutcome(run("claude", "a"), run("claude", "a"))).toBe("none");
    expect(switchOutcome(run("claude", "a"), run("claude", "b", "high", "work"))).toBe("resume");
    // One pi session runs Claude and GPT models.
    expect(switchOutcome(run("pi", "anthropic/x"), run("pi", "openai-codex/y"))).toBe("resume");
    expect(switchOutcome(run("codex", "a", null, "one"), run("codex", "a", null, "two"))).toBe("handoff");
    expect(switchOutcome(run("claude", "a"), run("agy", "a"))).toBe("handoff");
    expect(outcomeText("handoff", run("agy", "a"), "clean")).toBe("A new AGY chat starts with nothing from this one.");
  });

  it("names each CLI the same way in the chip, the list and the outcome", () => {
    expect([cliName("agy"), cliName("pi"), cliName("claude"), cliName("codex")]).toEqual(["AGY", "Pi", "Claude", "Codex"]);
    expect(runLabel(agentInput(session({ chair_provider: "agy" }), [chair({ provider: "agy" })]))).toBe("AGY · Default model");
  });

  it("blocks Apply while the agent works, with no model, or with no known CLI", () => {
    const idle = agentInput(session(), [chair({ provider: "claude" })]);
    expect(applyBlock(idle, run("claude", "b"))).toBeNull();
    expect(applyBlock({ ...idle, running: true }, run("claude", "b"))).toBe("Wait for this turn to end, or stop it.");
    expect(applyBlock(idle, run("claude", null))).toBe("Pick a model.");
    expect(applyBlock({ ...idle, provider: null }, run("claude", "b"))).toBe("This chat does not record its CLI, so it cannot switch.");
  });
});
