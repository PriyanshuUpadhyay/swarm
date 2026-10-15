import { describe, expect, it } from "vitest";
import { takeToken, forgetToken, TOKEN_KEY } from "./api";
import { chatFromPath, chatPath, resolveChat, shortId } from "./route";

const A = "01a13ee5-8197-7981-885d-e42964700e0a";
const B = "01a13e88-1f89-7482-a057-f7b478887e9e";

describe("chat route", () => {
  it("reads and writes /chat/<id>", () => {
    expect(chatFromPath(`/chat/${A}`)).toBe(A);
    expect(chatFromPath(`/chat/${A}/`)).toBe(A);
    expect(chatFromPath("/")).toBeNull();
    expect(chatFromPath("/chat/")).toBeNull();
    expect(chatFromPath("/chat/a/b")).toBeNull();
    expect(chatFromPath("/chat/%E0%A4%A")).toBeNull();
    expect(chatPath(A)).toBe(`/chat/${A}`);
    expect(chatPath(null)).toBe("/");
  });

  it("opens a chat by its full id or a unique short id", () => {
    expect(resolveChat(A, [A, B])).toBe(A);
    expect(resolveChat("01a13ee5", [A, B])).toBe(A);
    expect(resolveChat("01a13e", [A, B])).toBeNull();
    expect(resolveChat("nope", [A, B])).toBeNull();
    expect(shortId(A, [A, B])).toBe("01a13ee5");
    const twin = "01a13ee5-8197-7000-0000-000000000000";
    expect(shortId(A, [A, twin])).toBe("01a13ee5-8197-79");
    expect(resolveChat(shortId(A, [A, twin]), [A, twin])).toBe(A);
  });
});

describe("token", () => {
  const memory = () => {
    const items = new Map<string, string>();
    return {
      getItem: (k: string) => items.get(k) ?? null,
      setItem: (k: string, v: string) => void items.set(k, v),
      removeItem: (k: string) => void items.delete(k),
    };
  };
  const hist = () => {
    const calls: string[] = [];
    return { calls, state: null, replaceState: (_s: unknown, _t: string, url?: string | URL | null) => void calls.push(String(url)) };
  };

  it("saves the link's token, clears the hash, and reads it back on a reload", () => {
    const saved = memory();
    const h = hist();
    expect(takeToken({ hash: "#token=abc", pathname: "/chat/x", search: "" }, h, saved)).toBe("abc");
    expect(h.calls).toEqual(["/chat/x"]);
    expect(saved.getItem(TOKEN_KEY)).toBe("abc");
    expect(takeToken({ hash: "", pathname: "/", search: "" }, hist(), saved)).toBe("abc");
    forgetToken(saved);
    expect(takeToken({ hash: "", pathname: "/", search: "" }, hist(), saved)).toBeNull();
  });

  it("works when storage throws or is missing", () => {
    const broken = {
      getItem: () => {
        throw new Error("blocked");
      },
      setItem: () => {
        throw new Error("blocked");
      },
      removeItem: () => {
        throw new Error("blocked");
      },
    };
    expect(takeToken({ hash: "#token=abc", pathname: "/", search: "" }, hist(), broken)).toBe("abc");
    expect(takeToken({ hash: "", pathname: "/", search: "" }, hist(), broken)).toBeNull();
    expect(takeToken({ hash: "", pathname: "/", search: "" }, hist())).toBeNull();
    expect(() => forgetToken(broken)).not.toThrow();
  });
});
