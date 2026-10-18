import { useCallback, useEffect, useRef, useState } from "react";

import {
  getBlueprintLibraryFacets,
  listBlueprintLibrary,
  type BlueprintLibraryFacets,
  type BlueprintLibraryQuery,
  type BlueprintLibraryRow,
  type BlueprintLibrarySummary,
} from "../../../api/industry";
import { apiMessage } from "../shared/api-error";

// A workspace rarely owns more than a few hundred blueprints, so the first
// page usually is everything; "Load more" covers the rest.
export const BLUEPRINT_PAGE_SIZE = 500;

export interface BlueprintLibraryState {
  rows: BlueprintLibraryRow[];
  total: number;
  summary: BlueprintLibrarySummary | null;
  facets: BlueprintLibraryFacets | null;
  loading: boolean;
  loadingMore: boolean;
  error: string | null;
  loadMore: () => void;
}

export function useBlueprintLibrary(query: BlueprintLibraryQuery): BlueprintLibraryState {
  const [rows, setRows] = useState<BlueprintLibraryRow[]>([]);
  const [total, setTotal] = useState(0);
  const [summary, setSummary] = useState<BlueprintLibrarySummary | null>(null);
  const [facets, setFacets] = useState<BlueprintLibraryFacets | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const sequence = useRef(0);
  const queryKey = JSON.stringify(query);

  useEffect(() => {
    getBlueprintLibraryFacets()
      .then(setFacets)
      .catch(() => setFacets({ categories: [], groups: [], characters: [], locations: [] }));
  }, []);

  useEffect(() => {
    const current = ++sequence.current;
    setLoading(true);
    setError(null);
    listBlueprintLibrary({ ...query, limit: BLUEPRINT_PAGE_SIZE, offset: 0 })
      .then((page) => {
        if (current !== sequence.current) return;
        setRows(page.rows);
        setTotal(page.total);
        setSummary(page.summary);
      })
      .catch((requestError) => {
        if (current === sequence.current) setError(apiMessage(requestError));
      })
      .finally(() => {
        if (current === sequence.current) setLoading(false);
      });
    // `queryKey` stands in for `query`, which callers rebuild every render.
  }, [queryKey]);

  const loadMore = useCallback(() => {
    if (loadingMore || rows.length >= total) return;
    const current = sequence.current;
    setLoadingMore(true);
    listBlueprintLibrary({ ...query, limit: BLUEPRINT_PAGE_SIZE, offset: rows.length })
      .then((page) => {
        if (current !== sequence.current) return;
        setRows((existing) => [...existing, ...page.rows]);
        setTotal(page.total);
      })
      .catch((requestError) => {
        if (current === sequence.current) setError(apiMessage(requestError));
      })
      .finally(() => setLoadingMore(false));
  }, [loadingMore, queryKey, rows.length, total]);

  return { rows, total, summary, facets, loading, loadingMore, error, loadMore };
}
