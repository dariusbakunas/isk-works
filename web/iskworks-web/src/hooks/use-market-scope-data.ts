import {
  listMarketHubs,
  listMarketRegionLocations,
  listMarketRegions,
  listMarketStructures,
  type MarketHubSummary,
  type MarketLocation,
  type MarketRegion,
  type MarketStructureSummary,
} from "../api/industry";
import { useLoad } from "./use-load";

export type StructuresState =
  | { status: "idle" }
  | { status: "loading" }
  | { status: "ready"; data: MarketStructureSummary[] }
  | { status: "error" };

export interface MarketScopeDataRequest {
  /** The committed scope's region: its locations name the trigger button. */
  regionId: number;
  /** The region being browsed in the Regions tab. */
  draftRegionId: number;
  /** Whether the popover is open. Hubs and draft locations load only then. */
  open: boolean;
  /** Bumped on every open, so each visit refetches hubs and structures. */
  session: number;
  /** Whether this session has visited the My Structures tab. */
  structuresWanted: boolean;
}

export interface MarketScopeData {
  regions: MarketRegion[];
  /** Locations of the committed scope's region. */
  activeLocations: MarketLocation[];
  /** Locations of the draft region (the committed region's, when equal). */
  draftLocations: MarketLocation[];
  /** `null` while loading. */
  hubs: MarketHubSummary[] | null;
  structures: StructuresState;
}

/**
 * Everything the market scope selector shows. A failed list reads as empty
 * (or, for structures, as an error state) rather than surfacing its own
 * error UI in a popover this small.
 */
export function useMarketScopeData({
  regionId,
  draftRegionId,
  open,
  session,
  structuresWanted,
}: MarketScopeDataRequest): MarketScopeData {
  const regions = useLoad(() => listMarketRegions(), "regions");
  const active = useLoad(() => listMarketRegionLocations(regionId), `active:${regionId}`);
  const browsingOtherRegion = open && draftRegionId !== regionId;
  const draft = useLoad(
    browsingOtherRegion ? () => listMarketRegionLocations(draftRegionId) : null,
    `draft:${browsingOtherRegion ? draftRegionId : ""}`,
  );
  const hubs = useLoad(open ? () => listMarketHubs() : null, `hubs:${open ? session : ""}`);
  const wantStructures = open && structuresWanted;
  const structures = useLoad(
    wantStructures ? () => listMarketStructures() : null,
    `structures:${wantStructures ? session : ""}`,
  );

  const activeLocations = orEmpty(active);
  return {
    regions: orEmpty(regions),
    activeLocations,
    draftLocations: browsingOtherRegion ? orEmpty(draft) : activeLocations,
    hubs: hubs.loading ? null : orEmpty(hubs),
    structures: !wantStructures
      ? { status: "idle" }
      : structures.loading
        ? { status: "loading" }
        : structures.error || structures.data === null
          ? { status: "error" }
          : { status: "ready", data: structures.data },
  };
}

function orEmpty<T>(loaded: { data: T[] | null; error: Error | null }): T[] {
  return loaded.error ? [] : (loaded.data ?? []);
}
