import { useCallback, useEffect, useRef, useState } from "react";

import {
  getFinanceTransactions,
  type FinanceInventoryRecording,
  type FinanceQuery,
  type FinanceTransaction,
  type FinanceTransactionPage,
} from "../../api/finance";

export interface FinanceTransactionsState {
  page: FinanceTransactionPage | null;
  rows: FinanceTransaction[];
  loadingInitial: boolean;
  loadingMore: boolean;
  hasMore: boolean;
  error: Error | null;
  loadMore(): void;
  retryLoadMore(): void;
  /**
   * Replaces one loaded row's inventory recording with server-confirmed state.
   * Touches only that row: accumulated pages, filters, sort and scroll stay.
   */
  patchInventoryRecording(observationId: string, recording: FinanceInventoryRecording): void;
}

export function useFinanceTransactions(query: FinanceQuery): FinanceTransactionsState {
  const [page, setPage] = useState<FinanceTransactionPage | null>(null);
  const [rows, setRows] = useState<FinanceTransaction[]>([]);
  const [loadingInitial, setLoadingInitial] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  const generationRef = useRef(0);
  const inFlightRef = useRef(false);
  const pageRef = useRef<FinanceTransactionPage | null>(null);
  const rowsRef = useRef<FinanceTransaction[]>([]);
  const queryRef = useRef(query);
  queryRef.current = query;

  useEffect(() => {
    const generation = ++generationRef.current;
    inFlightRef.current = true;
    setLoadingInitial(true);
    setLoadingMore(false);
    setError(null);

    void getFinanceTransactions({ ...query, filter: { ...query.filter, page: 1 } })
      .then((result) => {
        if (generation !== generationRef.current) return;
        pageRef.current = result;
        rowsRef.current = result.rows;
        setPage(result);
        setRows(result.rows);
      })
      .catch((cause: unknown) => {
        if (generation === generationRef.current) setError(asError(cause));
      })
      .finally(() => {
        if (generation !== generationRef.current) return;
        inFlightRef.current = false;
        setLoadingInitial(false);
      });
  }, [query]);

  const loadMore = useCallback(() => {
    const currentPage = pageRef.current;
    const currentRows = rowsRef.current;
    if (!currentPage || inFlightRef.current || currentRows.length >= currentPage.totalCount) return;

    const generation = generationRef.current;
    const nextPage = Math.floor(currentRows.length / currentPage.pageSize) + 1;
    inFlightRef.current = true;
    setLoadingMore(true);
    setError(null);

    void getFinanceTransactions({
      ...queryRef.current,
      filter: { ...queryRef.current.filter, page: nextPage },
    })
      .then((result) => {
        if (generation !== generationRef.current) return;
        const appended = [...rowsRef.current, ...result.rows];
        rowsRef.current = appended;
        setRows(appended);
      })
      .catch((cause: unknown) => {
        if (generation === generationRef.current) setError(asError(cause));
      })
      .finally(() => {
        if (generation !== generationRef.current) return;
        inFlightRef.current = false;
        setLoadingMore(false);
      });
  }, []);

  const patchInventoryRecording = useCallback(
    (observationId: string, recording: FinanceInventoryRecording) => {
      const patched = rowsRef.current.map((row) =>
        row.observationId === observationId ? { ...row, inventoryRecording: recording } : row);
      rowsRef.current = patched;
      setRows(patched);
    },
    [],
  );

  return {
    page,
    rows,
    patchInventoryRecording,
    loadingInitial,
    loadingMore,
    hasMore: page !== null && rows.length < page.totalCount,
    error,
    loadMore,
    retryLoadMore: loadMore,
  };
}

function asError(cause: unknown) {
  return cause instanceof Error ? cause : new Error("Finance request failed.");
}
