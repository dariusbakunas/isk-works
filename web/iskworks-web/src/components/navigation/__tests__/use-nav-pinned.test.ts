import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { useNavPinned } from "../use-nav-pinned";

describe("useNavPinned", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    window.localStorage.clear();
  });

  test("starts collapsed", () => {
    const { result } = renderHook(() => useNavPinned());
    expect(result.current[0]).toBe(false);
  });

  test("remembers the pin across mounts", () => {
    const first = renderHook(() => useNavPinned());
    act(() => first.result.current[1]());
    expect(first.result.current[0]).toBe(true);
    first.unmount();

    const second = renderHook(() => useNavPinned());
    expect(second.result.current[0]).toBe(true);
  });

  test("still toggles when storage throws", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("blocked");
    });

    const { result } = renderHook(() => useNavPinned());
    expect(result.current[0]).toBe(false);
    act(() => result.current[1]());
    expect(result.current[0]).toBe(true);
  });
});
