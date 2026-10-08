import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { useDebouncedLookup } from "../use-debounced-lookup";

afterEach(() => {
  vi.useRealTimers();
});

describe("useDebouncedLookup", () => {
  test("waits 250ms and requires two trimmed characters", async () => {
    vi.useFakeTimers();
    const search = vi.fn().mockResolvedValue([{ id: 1 }]);
    const { result, rerender } = renderHook(
      ({ query }) => useDebouncedLookup(query, search, vi.fn()),
      { initialProps: { query: "r" } },
    );

    await act(() => vi.advanceTimersByTimeAsync(300));
    expect(search).not.toHaveBeenCalled();

    rerender({ query: " ri " });
    await act(() => vi.advanceTimersByTimeAsync(249));
    expect(search).not.toHaveBeenCalled();
    await act(() => vi.advanceTimersByTimeAsync(1));
    expect(search).toHaveBeenCalledWith("ri");
    await act(() => Promise.resolve());
    expect(result.current.results).toEqual([{ id: 1 }]);
  });

  test("restarts the timer while the query changes", async () => {
    vi.useFakeTimers();
    const search = vi.fn().mockResolvedValue([]);
    const { rerender } = renderHook(
      ({ query }) => useDebouncedLookup(query, search, vi.fn()),
      { initialProps: { query: "ri" } },
    );

    await act(() => vi.advanceTimersByTimeAsync(200));
    rerender({ query: "rift" });
    await act(() => vi.advanceTimersByTimeAsync(249));
    expect(search).not.toHaveBeenCalled();
    await act(() => vi.advanceTimersByTimeAsync(1));
    expect(search).toHaveBeenCalledTimes(1);
    expect(search).toHaveBeenCalledWith("rift");
  });

  test("ignores stale responses and clears results below the threshold", async () => {
    vi.useFakeTimers();
    let resolveRifter: (value: Array<{ id: number }>) => void = () => {};
    let resolveRift: (value: Array<{ id: number }>) => void = () => {};
    const search = vi.fn((query: string) => new Promise<Array<{ id: number }>>((resolve) => {
      if (query === "ri") resolveRifter = resolve;
      else resolveRift = resolve;
    }));
    const { result, rerender } = renderHook(
      ({ query }) => useDebouncedLookup(query, search, vi.fn()),
      { initialProps: { query: "ri" } },
    );

    await act(() => vi.advanceTimersByTimeAsync(250));
    rerender({ query: "rift" });
    await act(() => vi.advanceTimersByTimeAsync(250));
    await act(async () => resolveRift([{ id: 2 }]));
    expect(result.current.results).toEqual([{ id: 2 }]);
    await act(async () => resolveRifter([{ id: 1 }]));
    expect(result.current.results).toEqual([{ id: 2 }]);

    rerender({ query: "r" });
    expect(result.current.results).toEqual([]);
    expect(result.current.searching).toBe(false);
  });

  test("clears results and reports the latest search error", async () => {
    const error = new Error("lookup failed");
    const onError = vi.fn();
    const search = vi.fn().mockRejectedValue(error);
    const { result } = renderHook(() => useDebouncedLookup("ri", search, onError));

    await waitFor(() => expect(onError).toHaveBeenCalledWith(error));
    expect(result.current.results).toEqual([]);
    expect(result.current.searching).toBe(false);
  });

  test("minLength: 0 searches on an empty query", async () => {
    vi.useFakeTimers();
    const search = vi.fn().mockResolvedValue([{ id: 1 }]);
    renderHook(() => useDebouncedLookup("", search, vi.fn(), { minLength: 0 }));

    await act(() => vi.advanceTimersByTimeAsync(250));
    expect(search).toHaveBeenCalledWith("");
  });

  test("enabled: false skips searching regardless of query length", async () => {
    vi.useFakeTimers();
    const search = vi.fn().mockResolvedValue([{ id: 1 }]);
    const { result, rerender } = renderHook(
      ({ enabled }) => useDebouncedLookup("rifter", search, vi.fn(), { enabled }),
      { initialProps: { enabled: false } },
    );

    await act(() => vi.advanceTimersByTimeAsync(300));
    expect(search).not.toHaveBeenCalled();
    expect(result.current.results).toEqual([]);

    rerender({ enabled: true });
    await act(() => vi.advanceTimersByTimeAsync(250));
    expect(search).toHaveBeenCalledWith("rifter");
  });
});
