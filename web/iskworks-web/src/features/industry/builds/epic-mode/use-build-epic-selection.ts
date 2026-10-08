import { useEffect, useRef, useState } from "react";
import { useSearchParams } from "react-router";
import { listOrders, type OrderSummary } from "../../../../api/industry/orders";
import {
  readEpicParam,
  rememberEpic,
  rememberedEpic,
  resolveEpicSelection,
  withEpicParam,
} from "./epic-url-state";

/** An Epic the Plan view can show: open (not canceled, not archived),
 * created from this Build, with a version-3 frozen plan. */
export function isSelectableEpic(order: OrderSummary, buildId: string): boolean {
  return (
    order.sourceBuildId === buildId
    && order.canceledAt === null
    && order.archivedAt === null
    && (order.planningSnapshotVersion ?? 1) >= 3
  );
}

export interface BuildEpicSelection {
  /** This Build's open Epics, oldest first. Empty while loading. */
  epics: OrderSummary[];
  /** `null` = No Epic (free stock). */
  selectedEpicId: string | null;
  /** `adjust` edits the same URL update (e.g. switch to the Plan tab), so
   * two param changes never race. */
  selectEpic: (epicId: string | null, adjust?: (params: URLSearchParams) => void) => void;
  loading: boolean;
}

/**
 * The Epic the Build's Plan view shows, kept in `?epic=` and remembered per
 * Build. Only a root Build has Epics: a child producer Build lists none and
 * always shows free stock.
 */
export function useBuildEpicSelection(buildId: string | null): BuildEpicSelection {
  const [searchParams, setSearchParams] = useSearchParams();
  const [epics, setEpics] = useState<OrderSummary[] | null>(null);

  useEffect(() => {
    setEpics(null);
    if (!buildId) return;
    let cancelled = false;
    listOrders("active")
      .then((orders) => {
        if (cancelled) return;
        setEpics(
          orders
            .filter((order) => isSelectableEpic(order, buildId))
            .sort((a, b) => a.createdAt.localeCompare(b.createdAt)),
        );
      })
      // Without the list there is nothing to select: show free stock.
      .catch(() => {
        if (!cancelled) setEpics([]);
      });
    return () => {
      cancelled = true;
    };
  }, [buildId]);

  // Set once a stale `?epic=` link was dropped: that visit shows free
  // stock, so the remembered choice must not slip back in once the param
  // is gone.
  const [ignoreRemembered, setIgnoreRemembered] = useState(false);

  const urlEpicId = readEpicParam(searchParams);
  const selectedEpicId = epics === null || !buildId
    ? null
    : resolveEpicSelection(
      urlEpicId,
      ignoreRemembered ? null : rememberedEpic(buildId),
      new Set(epics.map((epic) => epic.id)),
    );

  // Once the list is known, make the URL say what is shown (drop a stale
  // id, or surface the remembered choice). At most once per URL: React
  // Router commits the replacement in a transition, so re-renders keep
  // seeing the old params until it lands.
  const normalizedSearch = useRef<string | null>(null);
  useEffect(() => {
    if (epics === null || selectedEpicId === urlEpicId) return;
    const search = searchParams.toString();
    if (normalizedSearch.current === search) return;
    normalizedSearch.current = search;
    if (urlEpicId !== null && selectedEpicId === null) setIgnoreRemembered(true);
    setSearchParams(withEpicParam(searchParams, selectedEpicId), { replace: true });
  }, [epics, selectedEpicId, urlEpicId, searchParams, setSearchParams]);

  function selectEpic(epicId: string | null, adjust?: (params: URLSearchParams) => void) {
    if (!buildId) return;
    rememberEpic(buildId, epicId);
    setIgnoreRemembered(false);
    const next = withEpicParam(searchParams, epicId);
    adjust?.(next);
    setSearchParams(next);
  }

  return { epics: epics ?? [], selectedEpicId, selectEpic, loading: epics === null };
}
