import { useCallback, useState } from "react";

// No backend or per-workspace precedent exists for "recent selections"
// anywhere in this app, and this app has no workspace-switching UI, so a single,
// unscoped localStorage key is simplest -- convenience UX, not domain
// state, so it's fine for it to live per-browser rather than per-workspace.
const STORAGE_KEY = "iskworks:market-scope-recents";
const MAX_RECENTS = 5;

export type MarketScopeRecentKind = "hub" | "npcStation" | "structure" | "region";

export interface MarketScopeRecent {
  kind: MarketScopeRecentKind;
  regionId: number;
  // Absent for a region-wide selection ("All locations").
  locationId?: number;
  label: string;
  subLabel?: string;
}

function recentIdentity(recent: Pick<MarketScopeRecent, "kind" | "regionId" | "locationId">): string {
  return `${recent.kind}:${recent.regionId}:${recent.locationId ?? ""}`;
}

function readRecents(): MarketScopeRecent[] {
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) return [];
    const parsed: unknown = JSON.parse(raw);
    return Array.isArray(parsed) ? (parsed as MarketScopeRecent[]) : [];
  } catch {
    return [];
  }
}

function writeRecents(recents: MarketScopeRecent[]): void {
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(recents));
  } catch {
    // Storage unavailable (private browsing, quota exceeded) -- recents
    // are a convenience, not a requirement; fail silently rather than
    // breaking scope selection over it.
  }
}

/**
 * Most-recently-used market scopes, capped at 5, deduplicated by
 * `(kind, regionId, locationId)` -- re-selecting an existing recent moves
 * it to the front instead of duplicating it.
 */
export function useMarketScopeRecents(): {
  recents: MarketScopeRecent[];
  pushRecent: (recent: MarketScopeRecent) => void;
} {
  const [recents, setRecents] = useState<MarketScopeRecent[]>(() => readRecents());

  const pushRecent = useCallback((recent: MarketScopeRecent) => {
    setRecents((current) => {
      const identity = recentIdentity(recent);
      const next = [recent, ...current.filter((existing) => recentIdentity(existing) !== identity)].slice(
        0,
        MAX_RECENTS,
      );
      writeRecents(next);
      return next;
    });
  }, []);

  return { recents, pushRecent };
}
