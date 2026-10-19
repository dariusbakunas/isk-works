import { useCallback, useEffect, useRef, useState } from "react";

import {
  getBlueprintLibraryFacets,
  listBlueprintLibrary,
  type BlueprintLibraryFacets,
  type BlueprintLibraryPage,
  type BlueprintLibraryQuery,
  type BlueprintLibraryRow,
  type BlueprintLibrarySummary,
} from "../../../api/industry";
import { apiMessage } from "../shared/api-error";

// A workspace rarely owns more than a few hundred blueprints, so the first
// page usually is everything; "Load more" covers the rest.
export const BLUEPRINT_PAGE_SIZE = 500;

async function fetchAllPages(query: BlueprintLibraryQuery): Promise<BlueprintLibraryPage> {
  const first = await listBlueprintLibrary({ ...query, limit: BLUEPRINT_PAGE_SIZE, offset: 0 });
  const rows = [...first.rows];
  let total = first.total;
  while (rows.length < total) {
    const next = await listBlueprintLibrary({ ...query, limit: BLUEPRINT_PAGE_SIZE, offset: rows.length });
    if (next.rows.length === 0) break;
    rows.push(...next.rows);
    total = next.total;
  }
  return { ...first, rows, total };
}

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

/**
 * `loadAll` fetches every page before showing any: grouping happens on the
 * client, and a group built from a partial page under-reports its rows.
 */
export function useBlueprintLibrary(query: BlueprintLibraryQuery, loadAll = false): BlueprintLibraryState {
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
    (loadAll ? fetchAllPages(query) : listBlueprintLibrary({ ...query, limit: BLUEPRINT_PAGE_SIZE, offset: 0 }))
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
  }, [queryKey, loadAll]);

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
