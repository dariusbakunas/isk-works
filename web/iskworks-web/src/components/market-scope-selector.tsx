import { ChevronDown, Search, X } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";

import {
  listMarketHubs,
  listMarketRegionLocations,
  listMarketRegions,
  listMarketStructures,
  searchMarketLocations,
  type MarketAccessState,
  type MarketHubSummary,
  type MarketLocation,
  type MarketLocationSearchResult,
  type MarketRegion,
  type MarketScope,
  type MarketStructureSummary,
  type ScopeFreshness,
} from "../api/industry";
import { useDebouncedLookup } from "../hooks/use-debounced-lookup";
import { useMarketScopeRecents, type MarketScopeRecent } from "../hooks/use-market-scope-recents";
import { Badge, type Tone } from "./primitives";

type Tab = "hubs" | "structures" | "regions";
type ResultKind = "hub" | "npcStation" | "structure" | "region";

type StructuresState =
  | { status: "idle" }
  | { status: "loading" }
  | { status: "ready"; data: MarketStructureSummary[] }
  | { status: "error" };

// Matches the popover's own `w-[480px]`. The popover is portaled to
// `document.body` and positioned with `fixed` coordinates computed from the
// trigger button's viewport rect -- necessary because it's rendered inside
// arbitrary containers (e.g. `BuildSettingsDialog`'s own scrollable,
// `overflow-y-auto` panel), and a `position: absolute` popover gets clipped
// by *any* ancestor with `overflow` set, regardless of viewport space.
const POPOVER_WIDTH = 480;

function computePopoverPosition(trigger: HTMLElement): { top: number; left: number; maxHeight: number } {
  const rect = trigger.getBoundingClientRect();
  const left = Math.min(Math.max(8, rect.left), window.innerWidth - POPOVER_WIDTH - 8);
  const top = rect.bottom + 4;
  const maxHeight = Math.min(window.innerHeight * 0.7, window.innerHeight - top - 8);
  return { top, left, maxHeight };
}

/**
 * Market Scope Selector -- reused by both the Market Browser header and
 * (future) Build pricing configuration, so there is one shared component
 * for picking a market scope. It offers a search box across every location
 * source, a Recent list, and Major Hubs / My Structures / Regions tabs; the
 * plain region-then-location browser lives on as the Regions tab's
 * content.
 *
 * Selecting a concrete hub, structure, or search result commits and closes
 * immediately -- it's already an unambiguous scope. The Regions tab alone
 * keeps the Apply/Cancel draft pattern, because picking a region by itself
 * is intermediate state (you still need to pick a location within it, or
 * explicitly choose "All locations").
 */
export function MarketScopeSelector({
  scope,
  onChange,
}: {
  scope: MarketScope;
  onChange: (scope: MarketScope) => void;
}) {
  const [open, setOpen] = useState(false);
  const [tab, setTab] = useState<Tab>("hubs");
  const [query, setQuery] = useState("");
  const [regions, setRegions] = useState<MarketRegion[]>([]);
  // Locations for the *committed* scope's region -- only needed to resolve
  // the trigger button's own label, kept separate from the draft list below
  // so opening the popover and browsing other regions never flickers it.
  const [activeLocations, setActiveLocations] = useState<MarketLocation[]>([]);
  const [draftRegionId, setDraftRegionId] = useState(scope.regionId);
  const [draftLocationId, setDraftLocationId] = useState(scope.locationId);
  const [draftLocations, setDraftLocations] = useState<MarketLocation[]>([]);
  const [hubs, setHubs] = useState<MarketHubSummary[] | null>(null);
  const [structuresState, setStructuresState] = useState<StructuresState>({ status: "idle" });
  const [position, setPosition] = useState<{ top: number; left: number; maxHeight: number } | null>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const popoverRef = useRef<HTMLDivElement>(null);
  const { recents, pushRecent } = useMarketScopeRecents();

  const search = useDebouncedLookup<MarketLocationSearchResult>(
    query,
    searchMarketLocations,
    () => {
      /* A failed search just shows no results -- not worth its own error UI
         in a popover this small. */
    },
    { enabled: open },
  );
  const searching = query.trim().length > 0;

  useEffect(() => {
    listMarketRegions()
      .then(setRegions)
      .catch(() => setRegions([]));
  }, []);

  useEffect(() => {
    listMarketRegionLocations(scope.regionId)
      .then(setActiveLocations)
      .catch(() => setActiveLocations([]));
  }, [scope.regionId]);

  // One fresh "session" per open: reset every draft/tab/query/cache back to
  // its starting point so a stale search or a tab left open from last time
  // never leaks into the next visit.
  useEffect(() => {
    if (!open) return;
    setTab("hubs");
    setQuery("");
    setDraftRegionId(scope.regionId);
    setDraftLocationId(scope.locationId);
    setHubs(null);
    setStructuresState({ status: "idle" });
  }, [open, scope.regionId, scope.locationId]);

  useEffect(() => {
    if (!open || hubs !== null) return;
    listMarketHubs()
      .then(setHubs)
      .catch(() => setHubs([]));
  }, [open, hubs]);

  useEffect(() => {
    if (!open || tab !== "structures" || structuresState.status !== "idle") return;
    setStructuresState({ status: "loading" });
    listMarketStructures()
      .then((data) => setStructuresState({ status: "ready", data }))
      .catch(() => setStructuresState({ status: "error" }));
  }, [open, tab, structuresState.status]);

  useEffect(() => {
    if (!open) return;
    if (draftRegionId === scope.regionId) {
      setDraftLocations(activeLocations);
      return;
    }
    listMarketRegionLocations(draftRegionId)
      .then(setDraftLocations)
      .catch(() => setDraftLocations([]));
  }, [open, draftRegionId, scope.regionId, activeLocations]);

  useEffect(() => {
    if (!open) return;
    function handlePointerDown(event: PointerEvent) {
      const target = event.target as Node;
      if (rootRef.current?.contains(target)) return;
      if (popoverRef.current?.contains(target)) return;
      setOpen(false);
    }
    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") setOpen(false);
    }
    document.addEventListener("pointerdown", handlePointerDown);
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("pointerdown", handlePointerDown);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [open]);

  // Positioned via `fixed` coordinates recomputed from the trigger's live
  // viewport rect (not CSS `absolute`) so the portaled popover tracks the
  // trigger through scrolling/resizing of whatever container it's in --
  // including a scrolled `BuildSettingsDialog`.
  useLayoutEffect(() => {
    if (!open || !triggerRef.current) {
      setPosition(null);
      return;
    }
    const trigger = triggerRef.current;
    setPosition(computePopoverPosition(trigger));
    function reposition() {
      setPosition(computePopoverPosition(trigger));
    }
    window.addEventListener("resize", reposition);
    window.addEventListener("scroll", reposition, true);
    return () => {
      window.removeEventListener("resize", reposition);
      window.removeEventListener("scroll", reposition, true);
    };
  }, [open]);

  const regionName = regions.find((region) => region.regionId === scope.regionId)?.regionName ?? "…";
  const locationName =
    scope.locationId === undefined
      ? "All locations"
      : (activeLocations.find((location) => location.locationId === scope.locationId)?.locationName ?? "…");

  function commit(nextScope: MarketScope, recent: MarketScopeRecent) {
    onChange(nextScope);
    pushRecent(recent);
    setOpen(false);
  }

  function selectHub(hub: MarketHubSummary) {
    commit(
      { regionId: hub.regionId, locationId: hub.locationId },
      { kind: "hub", regionId: hub.regionId, locationId: hub.locationId, label: hub.shortName, subLabel: hub.regionName },
    );
  }

  function selectStructure(structure: MarketStructureSummary) {
    commit(
      { regionId: structure.regionId ?? scope.regionId, locationId: structure.locationId },
      {
        kind: "structure",
        regionId: structure.regionId ?? scope.regionId,
        locationId: structure.locationId,
        label: structure.locationName,
        subLabel: structure.solarSystemName ?? undefined,
      },
    );
  }

  function selectSearchResult(result: MarketLocationSearchResult) {
    if (result.kind === "region") {
      commit(
        { regionId: result.regionId, locationId: undefined },
        { kind: "region", regionId: result.regionId, label: result.displayName },
      );
      return;
    }
    commit(
      { regionId: result.regionId ?? scope.regionId, locationId: result.locationId },
      {
        kind: result.kind,
        regionId: result.regionId ?? scope.regionId,
        locationId: result.locationId,
        label: result.displayName,
        subLabel: result.solarSystemName ?? result.regionName ?? undefined,
      },
    );
  }

  function selectRecent(recent: MarketScopeRecent) {
    commit({ regionId: recent.regionId, locationId: recent.locationId }, recent);
  }

  function applyRegionDraft() {
    const location = draftLocations.find((candidate) => candidate.locationId === draftLocationId);
    onChange({ regionId: draftRegionId, locationId: draftLocationId });
    pushRecent(
      location
        ? {
            kind: location.kind,
            regionId: draftRegionId,
            locationId: draftLocationId,
            label: location.locationName,
            subLabel: location.solarSystemName,
          }
        : {
            kind: "region",
            regionId: draftRegionId,
            label: regions.find((region) => region.regionId === draftRegionId)?.regionName ?? "Region",
          },
    );
    setOpen(false);
  }

  return (
    <div className="relative inline-block" ref={rootRef}>
      <button
        aria-expanded={open}
        aria-haspopup="dialog"
        className="iw-button-secondary"
        onClick={() => setOpen((value) => !value)}
        ref={triggerRef}
        type="button"
      >
        <span className="max-w-56 truncate">
          {regionName} · {locationName}
        </span>
        <ChevronDown aria-hidden="true" className="ml-1.5 h-3.5 w-3.5" />
      </button>
      {open && position ? createPortal(
        <div
          aria-label="Select market scope"
          className="iw-panel-strong fixed z-[80] flex w-[480px] flex-col border p-0 shadow-2xl"
          ref={popoverRef}
          role="dialog"
          style={{ top: position.top, left: position.left, maxHeight: position.maxHeight }}
        >
          <div className="flex items-center justify-between border-b border-border px-3 py-2">
            <span className="text-sm font-semibold">Select market scope</span>
            <button aria-label="Close" className="iw-icon-button" onClick={() => setOpen(false)} type="button">
              <X aria-hidden="true" className="h-4 w-4" />
            </button>
          </div>

          <div className="border-b border-border p-2">
            <div className="relative">
              <Search aria-hidden="true" className="pointer-events-none absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted" />
              <input
                autoFocus
                aria-label="Search regions, systems, stations, structures"
                className="iw-input pl-7"
                onChange={(event) => setQuery(event.target.value)}
                placeholder="Search regions, systems, stations, structures…"
                value={query}
              />
            </div>
          </div>

          <div className="min-h-0 flex-1 overflow-y-auto">
            {searching ? (
              <SearchResults results={search.results} searching={search.searching} onSelect={selectSearchResult} />
            ) : (
              <>
                {recents.length > 0 ? (
                  <div className="border-b border-border p-2">
                    <p className="iw-eyebrow mb-1 px-1">Recent</p>
                    <ul>
                      {recents.map((recent) => (
                        <li key={`${recent.kind}:${recent.regionId}:${recent.locationId ?? ""}`}>
                          <LocationRow
                            kind={recent.kind}
                            name={recent.label}
                            onClick={() => selectRecent(recent)}
                            subLabel={recent.subLabel}
                          />
                        </li>
                      ))}
                    </ul>
                  </div>
                ) : null}

                <div aria-label="Market location source" className="flex gap-2 border-b border-border p-2" role="tablist">
                  <button
                    className={tab === "hubs" ? "iw-button-primary" : "iw-button-secondary"}
                    onClick={() => setTab("hubs")}
                    role="tab"
                    aria-selected={tab === "hubs"}
                    type="button"
                  >
                    Major Hubs
                  </button>
                  <button
                    className={tab === "structures" ? "iw-button-primary" : "iw-button-secondary"}
                    onClick={() => setTab("structures")}
                    role="tab"
                    aria-selected={tab === "structures"}
                    type="button"
                  >
                    My Structures
                  </button>
                  <button
                    className={tab === "regions" ? "iw-button-primary" : "iw-button-secondary"}
                    onClick={() => setTab("regions")}
                    role="tab"
                    aria-selected={tab === "regions"}
                    type="button"
                  >
                    Regions
                  </button>
                </div>

                {tab === "hubs" ? <HubsTab hubs={hubs} onSelect={selectHub} /> : null}
                {tab === "structures" ? <StructuresTab onSelect={selectStructure} state={structuresState} /> : null}
                {tab === "regions" ? (
                  <RegionsTab
                    draftLocationId={draftLocationId}
                    draftLocations={draftLocations}
                    draftRegionId={draftRegionId}
                    onSelectLocation={setDraftLocationId}
                    onSelectRegion={(regionId) => {
                      setDraftRegionId(regionId);
                      setDraftLocationId(undefined);
                    }}
                    regions={regions}
                  />
                ) : null}
              </>
            )}
          </div>

          {!searching && tab === "regions" ? (
            <div className="flex justify-end gap-2 border-t border-border p-2">
              <button className="iw-button-secondary" onClick={() => setOpen(false)} type="button">
                Cancel
              </button>
              <button className="iw-button-primary" onClick={applyRegionDraft} type="button">
                Apply scope
              </button>
            </div>
          ) : null}
        </div>,
        document.body,
      ) : null}
    </div>
  );
}

function SearchResults({
  results,
  searching,
  onSelect,
}: {
  results: MarketLocationSearchResult[];
  searching: boolean;
  onSelect: (result: MarketLocationSearchResult) => void;
}) {
  if (searching) {
    return <p className="iw-muted p-3 text-sm">Searching…</p>;
  }
  if (results.length === 0) {
    return <p className="iw-muted p-3 text-sm">No locations match this search.</p>;
  }
  return (
    <ul className="p-2">
      {results.map((result) => (
        <li key={`${result.kind}:${result.kind === "region" ? result.regionId : result.locationId}`}>
          <LocationRow
            accessState={result.kind === "structure" ? result.accessState : undefined}
            freshness={result.kind === "region" ? undefined : result.freshness}
            kind={result.kind}
            name={result.displayName}
            onClick={() => onSelect(result)}
            subLabel={
              result.kind === "region"
                ? undefined
                : (result.solarSystemName ?? undefined) && (result.regionName ?? undefined)
                  ? `${result.solarSystemName} · ${result.regionName}`
                  : (result.regionName ?? undefined)
            }
          />
        </li>
      ))}
    </ul>
  );
}

function HubsTab({ hubs, onSelect }: { hubs: MarketHubSummary[] | null; onSelect: (hub: MarketHubSummary) => void }) {
  if (hubs === null) {
    return <p className="iw-muted p-3 text-sm">Loading hubs…</p>;
  }
  return (
    <ul className="p-2">
      {hubs.map((hub) => (
        <li key={hub.locationId}>
          <LocationRow
            freshness={hub.freshness}
            kind="hub"
            name={hub.shortName}
            onClick={() => onSelect(hub)}
            subLabel={`${hub.solarSystemName} · ${hub.regionName}`}
          />
        </li>
      ))}
    </ul>
  );
}

function StructuresTab({
  state,
  onSelect,
}: {
  state: StructuresState;
  onSelect: (structure: MarketStructureSummary) => void;
}) {
  if (state.status === "idle" || state.status === "loading") {
    return <p className="iw-muted p-3 text-sm">Loading structures…</p>;
  }
  if (state.status === "error") {
    return <p className="iw-muted p-3 text-sm">Could not load known structures.</p>;
  }
  if (state.data.length === 0) {
    return (
      <p className="iw-muted p-3 text-sm">
        No known structures yet. Use "Add structure" on the Market Browser page to add one.
      </p>
    );
  }
  return (
    <ul className="p-2">
      {state.data.map((structure) => (
        <li key={structure.locationId}>
          <LocationRow
            accessState={structure.accessState}
            freshness={structure.freshness}
            kind="structure"
            name={structure.locationName}
            onClick={() => onSelect(structure)}
            subLabel={
              structure.solarSystemName && structure.regionName
                ? `${structure.solarSystemName} · ${structure.regionName}${
                    structure.accessState === "confirmed" && structure.accessCharacterName
                      ? ` · via ${structure.accessCharacterName}`
                      : ""
                  }`
                : undefined
            }
          />
        </li>
      ))}
    </ul>
  );
}

function RegionsTab({
  regions,
  draftRegionId,
  draftLocationId,
  draftLocations,
  onSelectRegion,
  onSelectLocation,
}: {
  regions: MarketRegion[];
  draftRegionId: number;
  draftLocationId: number | undefined;
  draftLocations: MarketLocation[];
  onSelectRegion: (regionId: number) => void;
  onSelectLocation: (locationId: number | undefined) => void;
}) {
  return (
    <div className="flex">
      <div className="w-1/2 border-r border-border p-2">
        <p className="iw-eyebrow mb-1 px-1">Region</p>
        <ul className="max-h-64 overflow-y-auto">
          {regions.map((region) => (
            <li key={region.regionId}>
              <button
                className={`block w-full truncate rounded px-2 py-1 text-left text-sm ${
                  draftRegionId === region.regionId ? "bg-primary/10 text-primary" : "hover:bg-panel"
                }`}
                onClick={() => onSelectRegion(region.regionId)}
                type="button"
              >
                {region.regionName}
              </button>
            </li>
          ))}
        </ul>
      </div>
      <div className="w-1/2 p-2">
        <p className="iw-eyebrow mb-1 px-1">Location</p>
        <ul className="max-h-64 overflow-y-auto">
          <li>
            <button
              className={`block w-full truncate rounded px-2 py-1 text-left text-sm ${
                draftLocationId === undefined ? "bg-primary/10 text-primary" : "hover:bg-panel"
              }`}
              onClick={() => onSelectLocation(undefined)}
              type="button"
            >
              All locations
            </button>
          </li>
          {draftLocations.map((location) => (
            <li key={location.locationId}>
              <button
                className={`block w-full truncate rounded px-2 py-1 text-left text-sm ${
                  draftLocationId === location.locationId ? "bg-primary/10 text-primary" : "hover:bg-panel"
                }`}
                onClick={() => onSelectLocation(location.locationId)}
                type="button"
              >
                {location.locationName}
              </button>
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}

const KIND_BADGE: Record<ResultKind, { label: string; tone: Tone }> = {
  hub: { label: "HUB", tone: "primary" },
  npcStation: { label: "NPC", tone: "muted" },
  // Reuses the "positive" (teal) and "reaction" (purple) tones rather than
  // growing the shared Tone system for two badges local to this selector --
  // "reaction" is otherwise unused today (reserved for future
  // reaction-facility UI, a different screen entirely, so there's no
  // on-screen collision risk).
  structure: { label: "UPWELL", tone: "positive" },
  region: { label: "REGION", tone: "reaction" },
};

function LocationRow({
  kind,
  name,
  subLabel,
  freshness,
  accessState,
  onClick,
}: {
  kind: ResultKind;
  name: string;
  subLabel?: string;
  freshness?: ScopeFreshness;
  accessState?: MarketAccessState;
  onClick: () => void;
}) {
  const badge = KIND_BADGE[kind];
  return (
    <button className="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left hover:bg-panel" onClick={onClick} type="button">
      <Badge square tone={badge.tone}>
        {badge.label}
      </Badge>
      <span className="min-w-0 flex-1">
        <span className="block truncate text-sm">{name}</span>
        {subLabel ? <span className="iw-muted block truncate text-xs">{subLabel}</span> : null}
      </span>
      {accessState === "expired" ? (
        <span className="shrink-0 text-xs text-warning">Access expired</span>
      ) : freshness ? (
        <FreshnessIndicator freshness={freshness} />
      ) : null}
    </button>
  );
}

function FreshnessIndicator({ freshness }: { freshness: ScopeFreshness }) {
  if (freshness.trackedTypeCount === 0) {
    return <span className="iw-muted shrink-0 text-xs">No data</span>;
  }
  const age = formatAge(freshness.mostRecentObservedAt);
  const label =
    freshness.observedTypeCount === freshness.trackedTypeCount
      ? age
      : `${freshness.observedTypeCount}/${freshness.trackedTypeCount} · ${age}`;
  return <span className={`shrink-0 text-xs ${freshness.observedTypeCount > 0 ? "text-positive" : "text-warning"}`}>{label}</span>;
}

// Kept local rather than importing features/industry/market-browser's own
// copy -- this is a top-level shared component (also used by Build pricing
// configuration), so it shouldn't depend on feature-local code.
function formatAge(value: string | null): string {
  if (value === null) return "No data";
  const seconds = Math.max(0, Math.floor((Date.now() - new Date(value).getTime()) / 1000));
  if (seconds < 60) return "just now";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 48) return `${hours}h ago`;
  return `${Math.floor(hours / 24)}d ago`;
}
