import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

import type { AnalyticsQuery, FinanceAnalytics } from "../../../../api/finance-analytics";
import { getFinanceAnalytics } from "../../../../api/finance-analytics";
import { emptyAnalytics } from "./fixtures";
import { useFinanceAnalytics } from "../use-finance-analytics";

vi.mock("../../../../api/finance-analytics", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../../../api/finance-analytics")>()),
  getFinanceAnalytics: vi.fn(),
}));

const query: AnalyticsQuery = {
  connectionIds: [],
  dateFrom: "2026-09-01",
  dateTo: "2026-09-30",
  granularity: "week",
  comparePrevious: true,
  category: null,
  excludeInventoryBuys: false,
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

afterEach(() => {
  vi.resetAllMocks();
});

describe("useFinanceAnalytics", () => {
  test("loads once and exposes the payload", async () => {
    vi.mocked(getFinanceAnalytics).mockResolvedValue(emptyAnalytics());
    const { result } = renderHook(() => useFinanceAnalytics(query));

    expect(result.current.loading).toBe(true);
    expect(result.current.data).toBeNull();
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.data).not.toBeNull();
    expect(result.current.error).toBeNull();
  });

  test("keeps the previous data visible while a new query loads", async () => {
    const second = deferred<FinanceAnalytics>();
    vi.mocked(getFinanceAnalytics)
      .mockResolvedValueOnce(emptyAnalytics({ transactionCount: 7 }))
      .mockReturnValueOnce(second.promise);
    const { result, rerender } = renderHook(({ q }) => useFinanceAnalytics(q), { initialProps: { q: query } });
    await waitFor(() => expect(result.current.data?.kpis.transactionCount).toBe(7));

    rerender({ q: { ...query, granularity: "month" } });

    await waitFor(() => expect(result.current.loading).toBe(true));
    expect(result.current.data?.kpis.transactionCount).toBe(7);
    await act(async () => second.resolve(emptyAnalytics({ transactionCount: 9 })));
    await waitFor(() => expect(result.current.data?.kpis.transactionCount).toBe(9));
  });

  test("ignores a stale response that resolves after a newer query", async () => {
    const slow = deferred<FinanceAnalytics>();
    vi.mocked(getFinanceAnalytics)
      .mockReturnValueOnce(slow.promise)
      .mockResolvedValueOnce(emptyAnalytics({ transactionCount: 2 }));
    const { result, rerender } = renderHook(({ q }) => useFinanceAnalytics(q), { initialProps: { q: query } });

    rerender({ q: { ...query, category: "Ships" } });
    await waitFor(() => expect(result.current.data?.kpis.transactionCount).toBe(2));
    await act(async () => slow.resolve(emptyAnalytics({ transactionCount: 99 })));

    expect(result.current.data?.kpis.transactionCount).toBe(2);
  });

  test("aborts the in-flight request when the query changes", async () => {
    const signals: AbortSignal[] = [];
    vi.mocked(getFinanceAnalytics).mockImplementation((_query, signal) => {
      signals.push(signal!);
      return new Promise(() => undefined);
    });
    const { rerender } = renderHook(({ q }) => useFinanceAnalytics(q), { initialProps: { q: query } });
    rerender({ q: { ...query, comparePrevious: false } });

    expect(signals).toHaveLength(2);
    expect(signals[0].aborted).toBe(true);
    expect(signals[1].aborted).toBe(false);
  });

  test("surfaces an error and lets the caller retry", async () => {
    vi.mocked(getFinanceAnalytics)
      .mockRejectedValueOnce(new Error("boom"))
      .mockResolvedValueOnce(emptyAnalytics());
    const { result } = renderHook(() => useFinanceAnalytics(query));

    await waitFor(() => expect(result.current.error?.message).toBe("boom"));
    expect(result.current.loading).toBe(false);

    act(() => result.current.retry());
    await waitFor(() => expect(result.current.data).not.toBeNull());
    expect(result.current.error).toBeNull();
  });
});
