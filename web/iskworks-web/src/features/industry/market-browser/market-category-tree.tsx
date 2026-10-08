import { ChevronDown, ChevronRight } from "lucide-react";
import { useRef, useState } from "react";

import {
  listMarketItems,
  type MarketCategoryNode,
  type MarketItemSummary,
  type MarketScope,
} from "../../../api/industry";
import { MarketItemRow } from "./market-item-row";
import { apiMessage } from "./shared";

// Matches the backend's MAX_MARKET_ITEM_PAGE_SIZE -- a rendering/perf cap
// on one node's first load, not a hard browsing limit (see the "+N more"
// affordance below, which could become an incremental
// "load more" without changing this shape).
const NODE_ITEM_PAGE_SIZE = 200;

type NodeItemsState =
  | { status: "loading" }
  | { status: "ready"; items: MarketItemSummary[]; totalCount: number }
  | { status: "error"; message: string };

export function MarketCategoryTree({
  categories,
  selectedMarketGroupId,
  onSelect,
  selectedTypeId,
  onSelectItem,
  scope,
}: {
  categories: MarketCategoryNode[];
  selectedMarketGroupId: number | null;
  onSelect: (marketGroupId: number | null) => void;
  selectedTypeId: number | null;
  onSelectItem: (typeId: number, typeName: string) => void;
  scope: MarketScope;
}) {
  // Keyed by marketGroupId, shared across the whole tree so a node's items
  // stay cached across collapse/re-expand and independently of every
  // other node. `fetchedGroupsRef` (a plain, synchronously-updated Set,
  // not React state) is the actual single-flight guard -- it's checked
  // and marked in the same tick a node is expanded, before any request
  // goes out, so two rapid clicks on the same node before a re-render
  // can't both trigger a fetch. Each fetch's callback closes over its own
  // `marketGroupId` and writes only to that key, so responses can never
  // cross-populate a different node's entry regardless of resolve order.
  const [itemsByGroup, setItemsByGroup] = useState<Record<number, NodeItemsState>>({});
  const fetchedGroupsRef = useRef<Set<number>>(new Set());

  function ensureItemsLoaded(marketGroupId: number) {
    if (fetchedGroupsRef.current.has(marketGroupId)) return;
    fetchedGroupsRef.current.add(marketGroupId);
    setItemsByGroup((current) => ({ ...current, [marketGroupId]: { status: "loading" } }));
    listMarketItems({
      regionId: scope.regionId,
      locationId: scope.locationId,
      marketGroupId,
      page: 1,
      pageSize: NODE_ITEM_PAGE_SIZE,
    })
      .then((data) => {
        setItemsByGroup((current) => ({
          ...current,
          [marketGroupId]: { status: "ready", items: data.rows, totalCount: data.totalCount },
        }));
      })
      .catch((error) => {
        setItemsByGroup((current) => ({
          ...current,
          [marketGroupId]: { status: "error", message: apiMessage(error) },
        }));
        // A failure isn't cached forever -- clearing the single-flight
        // guard lets the next expand retry instead of being stuck.
        fetchedGroupsRef.current.delete(marketGroupId);
      });
  }

  return (
    <nav aria-label="Market categories" className="flex h-full min-w-0 flex-col">
      <button
        className={`iw-eyebrow mb-1.5 block w-full shrink-0 rounded px-1.5 py-1 text-left ${
          selectedMarketGroupId === null ? "bg-primary/10 text-primary" : "hover:bg-panel-strong"
        }`}
        onClick={() => onSelect(null)}
        type="button"
      >
        All categories
      </button>
      <ul className="flex min-h-0 flex-1 flex-col gap-y-1 overflow-y-auto">
        {categories.map((category) => (
          <CategoryNode
            category={category}
            depth={0}
            ensureItemsLoaded={ensureItemsLoaded}
            itemsByGroup={itemsByGroup}
            key={category.marketGroupId}
            onSelect={onSelect}
            onSelectItem={onSelectItem}
            selectedMarketGroupId={selectedMarketGroupId}
            selectedTypeId={selectedTypeId}
          />
        ))}
      </ul>
    </nav>
  );
}

function CategoryNode({
  category,
  depth,
  selectedMarketGroupId,
  onSelect,
  selectedTypeId,
  onSelectItem,
  itemsByGroup,
  ensureItemsLoaded,
}: {
  category: MarketCategoryNode;
  depth: number;
  selectedMarketGroupId: number | null;
  onSelect: (marketGroupId: number | null) => void;
  selectedTypeId: number | null;
  onSelectItem: (typeId: number, typeName: string) => void;
  itemsByGroup: Record<number, NodeItemsState>;
  ensureItemsLoaded: (marketGroupId: number) => void;
}) {
  // Collapsed by default at every depth -- the tree can be several levels
  // deep (Ships > Battlecruisers > Advanced Battlecruisers > Command
  // Ships > ...), so auto-expanding top-level nodes made the panel grow
  // page-length-tall on first load instead of staying a compact, own-
  // scrolling browser.
  const [expanded, setExpanded] = useState(false);
  const hasChildren = category.children.length > 0;
  // `itemCount` is already descendant-inclusive (own + every child's), so
  // a node with no children and a zero count structurally cannot have any
  // direct items either -- skip the chevron for it exactly as before,
  // without needing a separate "does this node have direct items" flag
  // from the backend.
  const expandable = hasChildren || category.itemCount > 0;
  const selected = selectedMarketGroupId === category.marketGroupId;
  const itemsState = itemsByGroup[category.marketGroupId];

  function toggleExpanded() {
    setExpanded((value) => {
      const next = !value;
      if (next) ensureItemsLoaded(category.marketGroupId);
      return next;
    });
  }

  const leafPaddingLeft = `${(depth + 1) * 14 + 28}px`;

  return (
    <li>
      <div
        className={`flex min-w-0 items-center gap-1 rounded px-1 py-1 text-sm ${
          selected ? "bg-primary/10 text-primary" : "hover:bg-panel-strong"
        }`}
        style={{ paddingLeft: `${depth * 14 + 4}px` }}
      >
        {expandable ? (
          <button
            aria-expanded={expanded}
            aria-label={expanded ? `Collapse ${category.name}` : `Expand ${category.name}`}
            className="grid h-5 w-5 shrink-0 place-items-center text-muted hover:text-foreground"
            onClick={toggleExpanded}
            type="button"
          >
            {expanded ? <ChevronDown className="h-3.5 w-3.5" /> : <ChevronRight className="h-3.5 w-3.5" />}
          </button>
        ) : (
          <span className="h-5 w-5 shrink-0" />
        )}
        <button
          className="flex min-w-0 flex-1 items-center gap-1.5 text-left"
          onClick={() => onSelect(category.marketGroupId)}
          type="button"
        >
          <span className="min-w-0 flex-1 truncate">{category.name}</span>
          {category.itemCount > 0 ? (
            <span className="shrink-0 font-mono text-xs text-muted">{category.itemCount.toLocaleString()}</span>
          ) : null}
        </button>
      </div>
      {expanded ? (
        <ul className="flex flex-col gap-y-1">
          {category.children.map((child) => (
            <CategoryNode
              category={child}
              depth={depth + 1}
              ensureItemsLoaded={ensureItemsLoaded}
              itemsByGroup={itemsByGroup}
              key={child.marketGroupId}
              onSelect={onSelect}
              onSelectItem={onSelectItem}
              selectedMarketGroupId={selectedMarketGroupId}
              selectedTypeId={selectedTypeId}
            />
          ))}
          {itemsState?.status === "loading" ? (
            <li className="py-1 text-xs text-muted" style={{ paddingLeft: leafPaddingLeft }}>
              Loading items...
            </li>
          ) : null}
          {itemsState?.status === "error" ? (
            <li className="py-1 text-xs text-danger" style={{ paddingLeft: leafPaddingLeft }}>
              {itemsState.message}
            </li>
          ) : null}
          {itemsState?.status === "ready"
            ? itemsState.items.map((item) => (
                <li key={item.typeId} style={{ paddingLeft: leafPaddingLeft }}>
                  <MarketItemRow item={item} onSelect={onSelectItem} selected={selectedTypeId === item.typeId} />
                </li>
              ))
            : null}
          {itemsState?.status === "ready" && itemsState.totalCount > itemsState.items.length ? (
            <li className="py-1 text-xs text-muted" style={{ paddingLeft: leafPaddingLeft }}>
              +{(itemsState.totalCount - itemsState.items.length).toLocaleString()} more
            </li>
          ) : null}
        </ul>
      ) : null}
    </li>
  );
}
