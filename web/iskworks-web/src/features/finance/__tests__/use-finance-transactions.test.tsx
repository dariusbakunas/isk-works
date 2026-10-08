import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, test, vi } from "vitest";

import type { FinanceQuery, FinanceTransactionPage } from "../../../api/finance";
import { getFinanceTransactions } from "../../../api/finance";
import { useFinanceTransactions } from "../use-finance-transactions";

vi.mock("../../../api/finance", async (importOriginal) => ({
  ...await importOriginal<typeof import("../../../api/finance")>(),
  getFinanceTransactions: vi.fn(),
}));

const query: FinanceQuery = {
  filter: {
    connectionIds: [], dateFrom: null, dateTo: null, search: null,
    transactionTypes: ["marketBuy", "marketSell"], direction: "all", page: 1, pageSize: 2,
  },
  sort: "time",
  order: "desc",
};

function page(number: number, names: string[], totalCount = 4): FinanceTransactionPage {
  return {
    rows: names.map((typeName, index) => ({
      observationId: `${number}-${index}`,
      transactionId: number * 10 + index,
      connectionId: "connection-1",
      characterName: "Valka",
      transactionType: "marketBuy",
      typeId: 34 + index,
      typeName,
      quantity: 1,
      unitPrice: "1.0000",
      totalPrice: "1.0000",
      transactedAt: "2026-08-06T12:00:00Z",
      counterpartyName: null,
      locationName: null,
      regionName: null,
      inventoryRecording: null,
    })),
    summary: { walletBalance: "0.0000", income: "0.0000", expenses: "2.0000", netIsk: "-2.0000", transactionCount: totalCount, averageDailyIsk: "-2.0000" },
    availableCharacters: [],
    totalCount,
    page: number,
    pageSize: 2,
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((onResolve, onReject) => { resolve = onResolve; reject = onReject; });
  return { promise, resolve, reject };
}

beforeEach(() => vi.mocked(getFinanceTransactions).mockReset());

describe("useFinanceTransactions", () => {
  test("appends pages and suppresses duplicate load requests", async () => {
    const second = deferred<FinanceTransactionPage>();
    vi.mocked(getFinanceTransactions)
      .mockResolvedValueOnce(page(1, ["First", "Second"]))
      .mockReturnValueOnce(second.promise);

    const { result } = renderHook(() => useFinanceTransactions(query));
    await waitFor(() => expect(result.current.rows).toHaveLength(2));

    act(() => { result.current.loadMore(); result.current.loadMore(); });
    expect(getFinanceTransactions).toHaveBeenCalledTimes(2);
    second.resolve(page(2, ["Third", "Fourth"]));

    await waitFor(() => expect(result.current.rows.map(({ typeName }) => typeName)).toEqual(["First", "Second", "Third", "Fourth"]));
    expect(result.current.hasMore).toBe(false);
  });

  test("discards stale responses after the query changes", async () => {
    const oldFirst = deferred<FinanceTransactionPage>();
    const newFirst = deferred<FinanceTransactionPage>();
    vi.mocked(getFinanceTransactions).mockReturnValueOnce(oldFirst.promise).mockReturnValueOnce(newFirst.promise);

    const { result, rerender } = renderHook(({ currentQuery }) => useFinanceTransactions(currentQuery), { initialProps: { currentQuery: query } });
    const changedQuery = { ...query, filter: { ...query.filter, search: "Rifter" } };
    rerender({ currentQuery: changedQuery });
    newFirst.resolve(page(1, ["Rifter"], 1));
    await waitFor(() => expect(result.current.rows[0]?.typeName).toBe("Rifter"));

    oldFirst.resolve(page(1, ["Stale"], 1));
    await act(async () => { await Promise.resolve(); });
    expect(result.current.rows[0]?.typeName).toBe("Rifter");
  });

  test("retains rows after a later-page failure and retries", async () => {
    vi.mocked(getFinanceTransactions)
      .mockResolvedValueOnce(page(1, ["First", "Second"]))
      .mockRejectedValueOnce(new Error("temporary failure"))
      .mockResolvedValueOnce(page(2, ["Third", "Fourth"]));

    const { result } = renderHook(() => useFinanceTransactions(query));
    await waitFor(() => expect(result.current.rows).toHaveLength(2));
    act(() => result.current.loadMore());
    await waitFor(() => expect(result.current.error?.message).toBe("temporary failure"));
    expect(result.current.rows).toHaveLength(2);

    act(() => result.current.retryLoadMore());
    await waitFor(() => expect(result.current.rows).toHaveLength(4));
    expect(result.current.error).toBeNull();
  });

  test("patches one row's inventory recording without touching accumulated pages or refetching", async () => {
    vi.mocked(getFinanceTransactions)
      .mockResolvedValueOnce(page(1, ["First", "Second"]))
      .mockResolvedValueOnce(page(2, ["Third", "Fourth"]));

    const { result } = renderHook(() => useFinanceTransactions(query));
    await waitFor(() => expect(result.current.rows).toHaveLength(2));
    act(() => result.current.loadMore());
    await waitFor(() => expect(result.current.rows).toHaveLength(4));

    const recorded = { state: "recorded" as const, recordingId: "rec-1", recordedAt: "2026-08-06T12:00:00Z", revertedAt: null, quantity: 1, totalBasis: "1.0000" };
    act(() => result.current.patchInventoryRecording("1-1", recorded));

    expect(result.current.rows.map(({ typeName }) => typeName)).toEqual(["First", "Second", "Third", "Fourth"]);
    expect(result.current.rows.map(({ inventoryRecording }) => inventoryRecording?.state ?? null)).toEqual([null, "recorded", null, null]);
    expect(getFinanceTransactions).toHaveBeenCalledTimes(2);
    // Later pages still continue from the right offset.
    expect(result.current.hasMore).toBe(false);
  });
});
