import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { useNavRecents } from "../use-nav-recents";

describe("useNavRecents", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    window.localStorage.clear();
  });

  test("records the registry page a location belongs to, newest first, without duplicates", () => {
    const { result } = renderHook(() => useNavRecents());

    act(() => result.current.recordVisit("/builds/123"));
    act(() => result.current.recordVisit("/inventory/runway"));
    act(() => result.current.recordVisit("/builds"));

    expect(result.current.recents).toEqual(["/builds", "/inventory/runway"]);
  });

  test("keeps the last five and ignores locations outside the registry", () => {
    const { result } = renderHook(() => useNavRecents());

    for (const path of ["/board", "/jobs", "/assets", "/market", "/finance", "/skills", "/nowhere"]) {
      act(() => result.current.recordVisit(path));
    }

    expect(result.current.recents).toEqual(["/skills", "/finance", "/market", "/assets", "/jobs"]);
  });

  test("survives a remount", () => {
    const first = renderHook(() => useNavRecents());
    act(() => first.result.current.recordVisit("/board"));
    first.unmount();

    expect(renderHook(() => useNavRecents()).result.current.recents).toEqual(["/board"]);
  });

  test("works without storage", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("blocked");
    });

    const { result } = renderHook(() => useNavRecents());
    act(() => result.current.recordVisit("/board"));
    expect(result.current.recents).toEqual(["/board"]);
  });
});
