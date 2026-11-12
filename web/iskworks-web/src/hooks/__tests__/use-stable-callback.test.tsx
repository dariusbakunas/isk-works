import { renderHook } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";

import { useStableCallback } from "../use-stable-callback";

describe("useStableCallback", () => {
  test("keeps one identity across renders and calls the latest callback", () => {
    const first = vi.fn((value: number) => value + 1);
    const second = vi.fn((value: number) => value * 10);
    const { result, rerender } = renderHook(({ callback }) => useStableCallback(callback), {
      initialProps: { callback: first },
    });
    const stable = result.current;

    rerender({ callback: second });

    expect(result.current).toBe(stable);
    expect(stable(3)).toBe(30);
    expect(first).not.toHaveBeenCalled();
    expect(second).toHaveBeenCalledWith(3);
  });
});
