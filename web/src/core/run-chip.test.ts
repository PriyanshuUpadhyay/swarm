// The run chip: its label, the outcome of a draft, and when Apply is blocked.

import { describe, expect, it } from "vitest";
import { chair, session } from "./chat-fixtures";
import { accountColor, agentInput, waitsForTurn, applyBlock, applyLabel, chooseModel, cliName, levelWord, limitText, outcomeText, runLabel, runTip, switchOutcome, usageLabel, usageTitle } from "./composer";
import type { Meter } from "./settings";


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
  });

  it("says what Apply does in the words of design A, and waits for the turn while the agent works", () => {
    const now = run("claude", "opus", "high", "work");
    const say = (to: ReturnType<typeof run>, carry: "summary" | "clean" = "summary", working = false) => outcomeText(switchOutcome(now, to), now, to, carry, { label: "Sonnet 5.5", working });
    expect(say(now)).toBe("Pick a change to see what will happen.");
    expect(say(run("claude", "sonnet", "high", "work"))).toBe("Restarts this chat on Sonnet 5.5 · High. History stays. The next reply re-reads the whole chat once, so it costs more.");
    expect(say(run("claude", "opus", "xhigh", "work"))).toBe("Restarts this chat on Extra high thinking. History stays. The next reply re-reads the whole chat once, so it costs more.");
    expect(say(run("claude", "opus", "high", "home"))).toBe("Restarts this chat under home. History stays. Usage now counts against home. The next reply may re-read the chat once.");
    expect(say(run("agy", "gemini", null))).toBe("Starts a new AGY chat on Sonnet 5.5 with a summary of this one. This chat stays as it is.");
    expect(say(run("agy", "gemini", null), "clean")).toBe("Starts a new AGY chat on Sonnet 5.5 with nothing from this one. This chat stays as it is.");
    expect(say(run("claude", "sonnet", "high", "work"), "summary", true)).toBe("Waits for the current turn to end. Then restarts this chat on Sonnet 5.5 · High. History stays. The next reply re-reads the whole chat once, so it costs more.");
    expect([applyLabel("handoff", false), applyLabel("handoff", true), applyLabel("resume", true), applyLabel("resume", false), applyLabel("none", false)]).toEqual(["Start new chat", "Start new chat after this turn", "Apply after this turn", "Restart and apply", "Apply"]);
  });

  it("keeps a queued change while the agent works, waits on a question, or another switch of the chat runs", () => {
    const idle = agentInput(session(), [chair({ provider: "claude", state: "done" })]);
    expect(waitsForTurn(idle, undefined)).toBe(false);
    expect(waitsForTurn(agentInput(session(), [chair({ provider: "claude", state: "working" })]), undefined)).toBe(true);
    expect(waitsForTurn(agentInput(session(), [chair({ provider: "claude", state: "waiting" })]), undefined)).toBe(true);
    expect(waitsForTurn(idle, { old: "s", phase: "summarizing", canCancel: true, error: null, carry: "summary" })).toBe(true);
    expect(waitsForTurn(idle, { old: "s", phase: "failed", canCancel: false, error: "x", carry: "summary" })).toBe(false);
    // A snapshot with no row for the chat says nothing about its turn, so the change keeps waiting.
    expect(waitsForTurn(agentInput(session(), []), undefined)).toBe(true);
  });

  it("names each thinking level in plain words and keeps an unknown id as written", () => {
    expect(["low", "medium", "high", "xhigh", "max", "ultra", "minimal", "turbo"].map(levelWord)).toEqual(["Low", "Medium", "High", "Extra high", "Max", "Ultra", "Minimal", "turbo"]);
  });

  it("shows what is left in each limit window of one account, and the summary when no meter is read", () => {
    const meter = (account: string | null, window: string, minutes: number | null, used: number | null): Meter => ({ provider: "claude", account, label: "x", window, window_minutes: minutes, used_pct: used, reset_time_seconds: null, state: "fresh", reason: null, reason_code: null, as_of_seconds: null });
    const meters = [meter("work", "5h", 300, 38), meter("work", "7d", 10080, 60), meter("home", "5h", 300, 5), { ...meter("work", "primary", 300, 1), provider: "codex" }];
    expect(limitText(meters, "claude", "work", null)).toEqual({ text: "5h 62% · wk 40%", left: 40 });
    expect(limitText(meters, "codex", "work", null)).toEqual({ text: "5h 99%", left: 99 });
    expect(limitText(meters, "claude", "other", "38% left")).toEqual({ text: "38% left", left: null });
    // A meter with no percent read shows the summary too.
    expect(limitText([meter("work", "fb", null, null)], "claude", "work", "38% left")).toEqual({ text: "38% left", left: null });
    expect(limitText([meter("work", "fb", null, null)], "claude", "work", null)).toEqual({ text: "", left: null });
    // A named Codex limit keeps its id, so two limits do not read the same.
    const spark = { ...meter("work", "codex_spark:primary", 300, 10), provider: "codex" };
    expect(limitText([spark, { ...meter("work", "primary", 300, 40), provider: "codex" }], "codex", "work", null).text).toBe("codex_spark 5h 90% · 5h 60%");
  });

  it("gives an account the same color each time, and its name, model and level in the chip's tooltip", () => {
    expect(accountColor("work")).toBe(accountColor("work"));
    expect(accountColor("work")).toMatch(/^oklch\(/);
    expect(runTip({ provider: "claude", model: "claude-opus-5-5", effort: "xhigh", account: "work" })).toBe("Claude account: work\nclaude-opus-5-5 · Extra high");
    expect(runTip({ provider: "pi", model: null, effort: null, account: null })).toBe("Pi login\nDefault model");
    expect(runTip({ provider: "claude", model: "opus", effort: null, account: "work" }, "me@example.com")).toBe("Claude account: work (me@example.com)\nopus");
  });

  it("names the meter Context and says what its number means", () => {
    expect(usageLabel({ context: { context_tokens: 41_200 }, remaining_percent: 79 })).toBe("Context 79% left");
    expect(usageTitle({ context: { context_tokens: 41_200 }, remaining_percent: 79 })).toBe("Context of this chat: 79% of the window is left.");
    expect(usageTitle({ context: { context_tokens: 41_200 } })).toBe("Context of this chat: 41K tokens.");
    expect(usageTitle(null)).toBe("The context of this chat is not known yet.");
  });

  it("names each CLI the same way in the chip, the list and the outcome", () => {
    expect([cliName("agy"), cliName("pi"), cliName("claude"), cliName("codex")]).toEqual(["AGY", "Pi", "Claude", "Codex"]);
    expect(runLabel(agentInput(session({ chair_provider: "agy" }), [chair({ provider: "agy" })]))).toBe("AGY · Default model");
  });

  it("blocks Apply with no model or with no known CLI, but not while the agent works", () => {
    const idle = agentInput(session(), [chair({ provider: "claude" })]);
    expect(applyBlock(idle, run("claude", "b"))).toBeNull();
    // A change while the agent works waits for the turn to end (design A).
    expect(applyBlock({ ...idle, running: true }, run("claude", "b"))).toBeNull();
    expect(applyBlock(idle, run("claude", null))).toBe("Pick a model.");
    expect(applyBlock({ ...idle, provider: null }, run("claude", "b"))).toBe("This chat does not record its CLI, so it cannot switch.");
  });

  it("names an unknown CLI by its id, also one that is an Object key", () => {
    expect(cliName("constructor")).toBe("constructor");
    expect(cliName("gemini")).toBe("gemini");
  });

  it("picks a model with a level it takes and keeps the account picked for its CLI", () => {
    const claude = { id: "claude", efforts: ["low", "medium", "high"], default_effort: "medium" };
    const codex = { id: "codex", efforts: ["low", "medium", "high"], default_effort: "medium" };
    const current = run("claude", "a", "high", "work");
    // A model that takes only low and ultra gets one of those, not the CLI's default.
    expect(chooseModel(current, current, codex, { id: "sol", efforts: ["low", "ultra"] }).effort).toBe("low");
    // An account picked in the popover stays when another model of the same CLI is picked.
    const picked = { ...current, account: "personal" };
    expect(chooseModel(current, picked, claude, "b").account).toBe("personal");
    // Back from another CLI, the chat's own account comes back.
    expect(chooseModel(current, run("codex", "x"), claude, "b").account).toBe("work");
    expect(chooseModel(current, current, codex, "x").account).toBeNull();
    expect(chooseModel(current, current, { id: "agy", efforts: [], default_effort: "" }, { id: "claude-sonnet-4-6", efforts: [] }).effort).toBeNull();
    // A model's own default comes before the CLI's.
    expect(chooseModel(current, run("codex", "x", "minimal"), codex, { id: "deep", efforts: ["low", "high"], default_effort: "high" }).effort).toBe("high");
    // A level the model lists but the CLI's launch does not take is never picked.
    expect(chooseModel(current, run("codex", "x", "minimal"), codex, { id: "mini", efforts: ["minimal", "high"] }).effort).toBe("high");
  });
});
