import { useCallback, useEffect, useRef, useState } from "react";

import {
  queryAssets,
  type AssetBrowserFacets,
  type AssetBrowserSummary,
  type FlatAssetPage,
  type FlatAssetQuery,
  type FlatAssetRow,
} from "../../api/assets";

/** A loaded page whose summary and facets come from the first page of the query. */
export type LoadedAssetPage = FlatAssetPage & { summary: AssetBrowserSummary; facets: AssetBrowserFacets };

const emptySummary: AssetBrowserSummary = {
  locationCount: 0,
  characterCount: 0,
  stackCount: 0,
  totalQuantity: 0,
  totalPackagedVolume: "0",
  latestObservedAt: null,
  unresolvedLocationCount: 0,
  syncStates: [],
};

const emptyFacets: AssetBrowserFacets = {
  characters: [],
  locations: [],
  assetKinds: [],
  groups: [],
  blueprintKinds: [],
  reconciliationStates: [],
};

export function useAssetsQuery(query: FlatAssetQuery, enabled = true) {
  const [page, setPage] = useState<LoadedAssetPage | null>(null);
  const [rows, setRows] = useState<FlatAssetRow[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  const generation = useRef(0);
  const inFlight = useRef(false);
  const pageRef = useRef<LoadedAssetPage | null>(null);
  const rowsRef = useRef<FlatAssetRow[]>([]);
  const queryRef = useRef(query);
  const enabledRef = useRef(enabled);
  queryRef.current = query;
  enabledRef.current = enabled;

  const loadInitial = useCallback(() => {
    if (!enabledRef.current) return;
    const currentGeneration = ++generation.current;
    inFlight.current = true;
    setLoading(true);
    setLoadingMore(false);
    setError(null);
    void queryAssets({ ...queryRef.current, cursor: undefined })
      .then((result) => {
        if (generation.current !== currentGeneration) return;
        const loaded = {
          ...result,
          summary: result.summary ?? emptySummary,
          facets: result.facets ?? emptyFacets,
        };
        pageRef.current = loaded;
        rowsRef.current = result.rows;
        setPage(loaded);
        setRows(result.rows);
      })
      .catch((cause: unknown) => {
        if (generation.current === currentGeneration) setError(asError(cause));
      })
      .finally(() => {
        if (generation.current !== currentGeneration) return;
        inFlight.current = false;
        setLoading(false);
      });
  }, []);

  useEffect(() => {
    if (enabled) {
      loadInitial();
      return;
    }
    generation.current += 1;
    inFlight.current = false;
    setLoading(false);
    setLoadingMore(false);
    setError(null);
  }, [enabled, loadInitial, query]);

  const loadMore = useCallback(() => {
    const currentPage = pageRef.current;
    if (!currentPage?.nextCursor || inFlight.current) return;
    const currentGeneration = generation.current;
    inFlight.current = true;
    setLoadingMore(true);
    setError(null);
    void queryAssets({ ...queryRef.current, cursor: currentPage.nextCursor })
      .then((result) => {
        if (generation.current !== currentGeneration) return;
        const seen = new Set(rowsRef.current.map((row) => `${row.connectionId}:${row.eveItemId}`));
        const appended = [...rowsRef.current, ...result.rows.filter((row) => !seen.has(`${row.connectionId}:${row.eveItemId}`))];
        rowsRef.current = appended;
        pageRef.current = {
          ...result,
          rows: appended,
          summary: result.summary ?? currentPage.summary,
          facets: result.facets ?? currentPage.facets,
        };
        setPage(pageRef.current);
        setRows(appended);
      })
      .catch((cause: unknown) => {
        if (generation.current === currentGeneration) setError(asError(cause));
      })
      .finally(() => {
        if (generation.current !== currentGeneration) return;
        inFlight.current = false;
        setLoadingMore(false);
      });
  }, []);

  const visiblePage = enabled || !page ? page : {
    ...page,
    rows: [],
    total: 0,
    nextCursor: null,
    summary: {
      ...page.summary,
      locationCount: 0,
      characterCount: 0,
      stackCount: 0,
      totalQuantity: 0,
      totalPackagedVolume: "0",
    },
  };

  return {
    page: visiblePage,
    rows: enabled ? rows : [],
    loading: enabled ? loading : false,
    loadingMore: enabled ? loadingMore : false,
    error: enabled ? error : null,
    hasMore: enabled && Boolean(page?.nextCursor),
    loadMore,
    retry: loadMore,
    refresh: loadInitial,
  };
}

function asError(cause: unknown) {
  return cause instanceof Error ? cause : new Error("Asset request failed.");
}
