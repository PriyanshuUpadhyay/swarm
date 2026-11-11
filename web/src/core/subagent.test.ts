import { expect, test } from "vitest";
import { closeSubagent, currentSubagent, openSubagent, subagentAgent, subagentChatId, subscribeSubagent } from "./subagent";

test("a subagent call opens its child chat and Open parent closes it", () => {
  expect(subagentAgent("chair", "toolu_1")).toBe("chair~sub~toolu_1");
  expect(subagentChatId("s1", "chair", "toolu_1")).toBe("s1:chair~sub~toolu_1");
  let seen = 0;
  const stop = subscribeSubagent(() => seen++);
  openSubagent({ chatId: "s1", callId: "toolu_1", title: "Explore · Find the parser" });
  expect(currentSubagent()?.callId).toBe("toolu_1");
  closeSubagent();
  expect(currentSubagent()).toBeNull();
  expect(seen).toBe(2);
  stop();
});
