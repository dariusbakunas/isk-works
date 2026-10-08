import { afterEach, describe, expect, it, vi } from "vitest";
import {
  readEpicParam,
  rememberEpic,
  rememberedEpic,
  resolveEpicSelection,
  withEpicParam,
} from "../epic-url-state";

const OPEN = new Set(["epic-a", "epic-b"]);

describe("epic URL state", () => {
  afterEach(() => {
    window.localStorage.clear();
    vi.restoreAllMocks();
  });

  it("prefers the URL's Epic over the remembered one", () => {
    expect(resolveEpicSelection("epic-a", "epic-b", OPEN)).toBe("epic-a");
  });

  it("falls back to the remembered Epic when the URL names none", () => {
    expect(resolveEpicSelection(null, "epic-b", OPEN)).toBe("epic-b");
  });

  it("shows No Epic for an unknown or closed Epic in the URL", () => {
    expect(resolveEpicSelection("epic-closed", "epic-b", OPEN)).toBeNull();
  });

  it("forgets a remembered Epic that is no longer open", () => {
    expect(resolveEpicSelection(null, "epic-closed", OPEN)).toBeNull();
    expect(resolveEpicSelection(null, null, OPEN)).toBeNull();
  });

  it("reads and writes the epic param without touching others", () => {
    const params = new URLSearchParams("view=plan");
    const withEpic = withEpicParam(params, "epic-a");
    expect(withEpic.toString()).toBe("view=plan&epic=epic-a");
    expect(readEpicParam(withEpic)).toBe("epic-a");
    expect(withEpicParam(withEpic, null).toString()).toBe("view=plan");
    expect(readEpicParam(new URLSearchParams("epic="))).toBeNull();
  });

  it("remembers the choice per Build", () => {
    rememberEpic("build-1", "epic-a");
    rememberEpic("build-2", "epic-b");
    expect(rememberedEpic("build-1")).toBe("epic-a");
    expect(rememberedEpic("build-2")).toBe("epic-b");
    rememberEpic("build-1", null);
    expect(rememberedEpic("build-1")).toBeNull();
  });

  it("works without browser storage", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    expect(() => rememberEpic("build-1", "epic-a")).not.toThrow();
    expect(rememberedEpic("build-1")).toBeNull();
  });
});
