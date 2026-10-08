import { Search, Upload, Building2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import {
  listMarketCategories,
  listMarketItems,
  type MarketCategoryNode,
  type MarketItemSummary,
  type MarketScope,
} from "../../../api/industry";
import { InlineAlert, PageHeader, Panel } from "../../../components/primitives";
import { MarketScopeSelector } from "../../../components/market-scope-selector";
import { AddStructureDialog } from "./add-structure-dialog";
import { MarketCategoryTree } from "./market-category-tree";
import { MarketImportDialog } from "./market-import-dialog";
import { MarketItemDetailPanel } from "./market-item-detail-panel";
import { MarketItemRow } from "./market-item-row";
import { apiMessage } from "./shared";

// The Forge / Jita 4-4 -- the default scope shown until the user picks a
// different one via MarketScopeSelector.
const DEFAULT_SCOPE: MarketScope = { regionId: 10_000_002, locationId: 60_003_760 };

// Catalog-wide quick search: capped to one small page -- "refine your
// search" is the intended way to narrow further, not a second paginated
// browser -- with a short debounce and the same 2-character floor the
// Market Scope Selector's own search box uses
// (`search_market_locations`'s `q.chars().count() < 2` convention), rather
// than scanning the whole ~19k-row catalog on the first keystroke.
const SEARCH_PAGE_SIZE = 50;
const SEARCH_DEBOUNCE_MS = 250;
const SEARCH_MIN_LENGTH = 2;

type CategoriesState =
  | { status: "loading" }
  | { status: "error"; message: string }
  | { status: "ready"; data: MarketCategoryNode[] };

type SearchState =
  | { status: "idle" }
  | { status: "loading" }
  | { status: "ready"; rows: MarketItemSummary[]; totalCount: number }
  | { status: "error"; message: string };

export function MarketBrowserPage() {
  const [scope, setScope] = useState<MarketScope>(DEFAULT_SCOPE);
  const [categoriesState, setCategoriesState] = useState<CategoriesState>({ status: "loading" });
  const [marketGroupId, setMarketGroupId] = useState<number | null>(null);
  // Driven by either the tree or catalog search -- no default/first-item
  // auto-selection, since there's no paginated item list to pick a
  // "first row" from in the first place.
  const [selectedItem, setSelectedItem] = useState<{ typeId: number; typeName: string } | null>(null);
  const [importOpen, setImportOpen] = useState(false);
  const [addStructureOpen, setAddStructureOpen] = useState(false);
  // Forces the detail panel to remount and reload -- an import or a newly
  // added/verified structure can change what's known about the currently
  // selected item even though neither touches `typeId`/`scope` directly.
  const [refreshToken, setRefreshToken] = useState(0);

  const [searchQuery, setSearchQuery] = useState("");
  const [searchState, setSearchState] = useState<SearchState>({ status: "idle" });
  const searchSequenceRef = useRef(0);

  useEffect(() => {
    listMarketCategories()
      .then((data) => setCategoriesState({ status: "ready", data }))
      .catch((error) => setCategoriesState({ status: "error", message: apiMessage(error) }));
  }, []);

  useEffect(() => {
    const trimmed = searchQuery.trim();
    // Bumped on every keystroke (even ones that don't end up firing a
    // request) so a response for an older query can never land after a
    // newer one already replaced it in state.
    const sequence = ++searchSequenceRef.current;

    if (trimmed.length < SEARCH_MIN_LENGTH) {
      setSearchState({ status: "idle" });
      return;
    }

    const timer = window.setTimeout(() => {
      setSearchState({ status: "loading" });
      listMarketItems({
        regionId: scope.regionId,
        locationId: scope.locationId,
        search: trimmed,
        page: 1,
        pageSize: SEARCH_PAGE_SIZE,
      })
        .then((page) => {
          if (sequence !== searchSequenceRef.current) return;
          setSearchState({ status: "ready", rows: page.rows, totalCount: page.totalCount });
        })
        .catch((error) => {
          if (sequence !== searchSequenceRef.current) return;
          setSearchState({ status: "error", message: apiMessage(error) });
        });
    }, SEARCH_DEBOUNCE_MS);

    return () => window.clearTimeout(timer);
  }, [searchQuery, scope.regionId, scope.locationId]);

  const searching = searchQuery.trim().length >= SEARCH_MIN_LENGTH;

  function selectItem(typeId: number, typeName: string) {
    setSelectedItem({ typeId, typeName });
  }

  return (
    <>
      <PageHeader eyebrow="Market" title="Market">
        Browse observed market prices by category or search -- no Price Override setup required.
      </PageHeader>
      <div className="mb-3 flex flex-wrap items-center gap-3">
        <MarketScopeSelector onChange={setScope} scope={scope} />
        <button className="iw-button-secondary ml-auto" onClick={() => setAddStructureOpen(true)} type="button">
          <Building2 aria-hidden="true" className="mr-2 h-4 w-4" />
          Add structure
        </button>
        <button className="iw-button-secondary" onClick={() => setImportOpen(true)} type="button">
          <Upload aria-hidden="true" className="mr-2 h-4 w-4" />
          Import market export
        </button>
      </div>

      <MarketImportDialog
        onClose={() => setImportOpen(false)}
        onImported={() => setRefreshToken((value) => value + 1)}
        open={importOpen}
      />
      <AddStructureDialog
        onAdded={() => setRefreshToken((value) => value + 1)}
        onClose={() => setAddStructureOpen(false)}
        open={addStructureOpen}
      />
      <div className="grid min-w-0 gap-3 lg:grid-cols-[280px_minmax(0,1fr)]">
        <Panel className="sticky top-3 flex min-w-0 max-h-[calc(100vh-8rem)] flex-col overflow-hidden">
          <div className="relative mb-1.5 shrink-0">
            <Search
              aria-hidden="true"
              className="pointer-events-none absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted"
            />
            <input
              aria-label="Search market items"
              className="iw-input pl-7"
              onChange={(event) => setSearchQuery(event.target.value)}
              placeholder="Search items..."
              value={searchQuery}
            />
          </div>

          {/* The tree stays mounted (just visually hidden) while searching,
              rather than being removed from the tree via a ternary, so its
              own lazy-loaded per-node item cache and expand/collapse state
              survive clearing the search box instead of forcing a re-fetch
              of every previously-expanded node. */}
          <div className={searching ? "hidden" : "flex min-h-0 flex-1 flex-col"}>
            {categoriesState.status === "loading" ? <p className="iw-muted">Loading categories...</p> : null}
            {categoriesState.status === "error" ? (
              <InlineAlert title="Categories unavailable">{categoriesState.message}</InlineAlert>
            ) : null}
            {categoriesState.status === "ready" ? (
              <MarketCategoryTree
                categories={categoriesState.data}
                onSelect={setMarketGroupId}
                onSelectItem={selectItem}
                scope={scope}
                selectedMarketGroupId={marketGroupId}
                selectedTypeId={selectedItem?.typeId ?? null}
              />
            ) : null}
          </div>

          {searching ? (
            <SearchResultsPane onSelectItem={selectItem} selectedTypeId={selectedItem?.typeId ?? null} state={searchState} />
          ) : null}
        </Panel>

        <div className="min-w-0">
          {selectedItem ? (
            <MarketItemDetailPanel
              categories={categoriesState.status === "ready" ? categoriesState.data : []}
              key={refreshToken}
              locationId={scope.locationId}
              regionId={scope.regionId}
              typeId={selectedItem.typeId}
              typeName={selectedItem.typeName}
            />
          ) : (
            <Panel>
              <p className="iw-muted">Select an item from the market tree.</p>
            </Panel>
          )}
        </div>
      </div>
    </>
  );
}

// Catalog-wide search results -- a flat list, deliberately not a second
// tree/browser: finding one item quickly is the whole point, so results are
// capped to one page and a "refine your search" hint stands in for real
// pagination.
function SearchResultsPane({
  state,
  selectedTypeId,
  onSelectItem,
}: {
  state: SearchState;
  selectedTypeId: number | null;
  onSelectItem: (typeId: number, typeName: string) => void;
}) {
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {state.status === "loading" ? <p className="iw-muted px-1 py-1 text-sm">Searching...</p> : null}
      {state.status === "error" ? <p className="px-1 py-1 text-sm text-danger">{state.message}</p> : null}
      {state.status === "ready" && state.rows.length === 0 ? (
        <p className="iw-muted px-1 py-1 text-sm">No items match this search.</p>
      ) : null}
      {state.status === "ready" && state.rows.length > 0 ? (
        <ul className="flex min-h-0 flex-1 flex-col gap-y-1 overflow-y-auto">
          {state.rows.map((item) => (
            <li key={item.typeId}>
              <MarketItemRow item={item} onSelect={onSelectItem} selected={selectedTypeId === item.typeId} />
            </li>
          ))}
          {state.totalCount > state.rows.length ? (
            <li className="px-1 py-1 text-xs text-muted">More results available — refine your search</li>
          ) : null}
        </ul>
      ) : null}
    </div>
  );
}
