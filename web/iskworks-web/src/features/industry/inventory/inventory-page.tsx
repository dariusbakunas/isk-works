import { ChevronsDown, ChevronsUp, Download, MoreHorizontal, PackagePlus, Plus, Scale, Search, ShoppingCart, Upload } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { useSearchParams } from "react-router";

import { exportInventory, getInventoryItem, listInventory, type InventoryItem } from "../../../api/inventory";
import { listPriceSources, type PriceSource } from "../../../api/industry";
import { DropdownMenu } from "../../../components/dropdown-menu";
import { EmptyState, InlineAlert, PageHeader, Panel } from "../../../components/primitives";
import { InventoryAdjustmentPanel } from "./inventory-adjustment-panel";
import { InventoryEsiHoldingsModal } from "./inventory-esi-holdings-modal";
import { groupInventoryItems, InventoryOperationalTable } from "./inventory-operational-table";
import {
  filterScope,
  INVENTORY_FILTERS,
  matchesInventoryFilter,
  matchesInventorySearch,
  type InventoryFilterId,
} from "./inventory-filters";
import { InventoryInspector } from "./inventory-inspector";
import { InventoryPostingPanel } from "./inventory-posting-panel";
import { ImportInventoryModal } from "./inventory-import-modal";
import { apiMessage, type LoadState } from "./shared";

function downloadJson(filename: string, data: unknown) {
  const blob = new Blob([JSON.stringify(data, null, 2)], { type: "application/json" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  link.click();
  URL.revokeObjectURL(url);
}

export function InventoryPage() {
  const [params, setParams] = useSearchParams();
  const [state, setState] = useState<LoadState<InventoryItem[]>>({ status: "loading" });
  const [sources, setSources] = useState<PriceSource[]>([]);
  const [exportError, setExportError] = useState("");
  const [importOpen, setImportOpen] = useState(false);
  const [search, setSearch] = useState("");
  const [filter, setFilter] = useState<InventoryFilterId>("tracked");
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(() => new Set());
  // A deep-linked ?item= can name a type outside the currently-fetched
  // scope (e.g. an untracked ESI-only type while viewing the default
  // Tracked tab -- the whole point of "View in Inventory" links from
  // Assets). Fetched directly only when the summary isn't already in
  // `items`, so the common case (selecting a visible row) stays free.
  const [deepLinkedItem, setDeepLinkedItem] = useState<InventoryItem | null>(null);
  // The ESI holdings drill-down is a contextual overlay, deliberately not
  // tied to `?item=`/`intent` -- opening/closing it must never touch
  // search, filter, category expansion, or the selected item.
  const [holdingsItem, setHoldingsItem] = useState<InventoryItem | null>(null);
  // Bumped (never reused) when "Review adjustment" is clicked from the
  // holdings modal, so InventoryInspector knows to open its existing
  // Review Discrepancy panel even if the item was already selected.
  const scope = filterScope(filter);
  const sourceId = params.get("priceSourceId") ?? "";
  const intent = params.get("intent");
  const selectedTypeId = params.has("item") ? Number(params.get("item")) : null;

  async function handleExport() {
    setExportError("");
    try {
      const data = await exportInventory();
      downloadJson(`iskworks-inventory-${new Date().toISOString().slice(0, 10)}.json`, data);
    } catch (error) {
      setExportError(apiMessage(error));
    }
  }

  // Only the latest load may update the list: a slower response for a
  // scope or price source the user already switched away from (or a refresh
  // overtaken by another) is dropped instead of overwriting newer data.
  const latestLoad = useRef(0);

  function load() {
    const request = ++latestLoad.current;
    setState({ status: "loading" });
    Promise.all([listInventory(sourceId || undefined, scope), listPriceSources()])
      .then(([items, loadedSources]) => {
        if (request !== latestLoad.current) return;
        setState({ status: "ready", data: items });
        setSources(loadedSources);
      })
      .catch((error) => {
        if (request !== latestLoad.current) return;
        setState({ status: "error", message: apiMessage(error) });
      });
  }

  useEffect(() => {
    load();
    return () => {
      latestLoad.current += 1;
    };
  }, [sourceId, scope]);

  const items = state.status === "ready" ? state.data : [];
  const visibleItems = useMemo(
    () => items.filter((item) => matchesInventorySearch(item, search) && matchesInventoryFilter(item, filter)),
    [items, search, filter],
  );
  const groupKeys = useMemo(() => groupInventoryItems(visibleItems).map((group) => group.key), [visibleItems]);
  const allExpanded = groupKeys.length > 0 && groupKeys.every((key) => expandedGroups.has(key));
  const allCollapsed = groupKeys.every((key) => !expandedGroups.has(key));

  const selectedItem = selectedTypeId === null
    ? null
    : items.find((item) => item.balance.key.typeId === selectedTypeId) ?? deepLinkedItem;

  useEffect(() => {
    if (selectedTypeId === null || items.some((item) => item.balance.key.typeId === selectedTypeId)) {
      setDeepLinkedItem(null);
      return;
    }
    let cancelled = false;
    getInventoryItem(selectedTypeId, sourceId || undefined)
      .then((detail) => { if (!cancelled) setDeepLinkedItem(detail); })
      .catch(() => { if (!cancelled) setDeepLinkedItem(null); });
    return () => { cancelled = true; };
    // `items` intentionally excluded: only re-run when the target type or
    // price source changes, not on every scope/filter-driven list reload.
  }, [selectedTypeId, sourceId]);

  function openPosting(kind: "opening" | "purchase", typeId?: number) {
    const next = new URLSearchParams(params);
    next.delete("item");
    next.set("intent", kind === "opening" ? "opening-balance" : "record-purchase");
    if (typeId) next.set("typeId", String(typeId));
    else next.delete("typeId");
    setParams(next);
  }

  function openAdjustment(typeId?: number) {
    const next = new URLSearchParams(params);
    next.delete("item");
    next.set("intent", "adjust-inventory");
    if (typeId) next.set("typeId", String(typeId));
    else next.delete("typeId");
    setParams(next);
  }

  function toggleGroup(groupKey: string, expanded: boolean) {
    setExpandedGroups((current) => {
      const next = new Set(current);
      if (expanded) next.add(groupKey);
      else next.delete(groupKey);
      return next;
    });
  }

  return (
    <>
      <PageHeader eyebrow="Accounting inventory" title="Inventory">
        Owner-wide item balances, exact historical cost, and current planning-price comparisons.
      </PageHeader>
      <div className="mb-3 flex flex-wrap items-end justify-between gap-3">
        <label className="min-w-56 text-sm font-semibold">
          Price Override
          <select
            className="iw-input mt-1"
            value={sourceId}
            onChange={(event) => {
              const next = new URLSearchParams(params);
              if (event.target.value) next.set("priceSourceId", event.target.value);
              else next.delete("priceSourceId");
              setParams(next);
            }}
          >
            <option value="">Workspace default</option>
            {sources.map((source) => <option key={source.id} value={source.id}>{source.name}</option>)}
          </select>
        </label>
        <div className="flex flex-wrap gap-2">
          <DropdownMenu
            icon={<Plus aria-hidden="true" className="h-4 w-4" />}
            items={[
              {
                key: "opening-balance",
                label: "Opening Balance",
                icon: <PackagePlus aria-hidden="true" className="h-4 w-4" />,
                onSelect: () => openPosting("opening"),
              },
              {
                key: "record-purchase",
                label: "Purchase",
                icon: <ShoppingCart aria-hidden="true" className="h-4 w-4" />,
                onSelect: () => openPosting("purchase"),
              },
              {
                key: "adjust-inventory",
                label: "Adjustment",
                icon: <Scale aria-hidden="true" className="h-4 w-4" />,
                onSelect: () => openAdjustment(),
              },
            ]}
            label="Record"
            variant="primary"
          />
          <DropdownMenu
            align="end"
            icon={<MoreHorizontal aria-hidden="true" className="h-4 w-4" />}
            items={[
              {
                key: "export",
                label: "Export",
                icon: <Download aria-hidden="true" className="h-4 w-4" />,
                onSelect: handleExport,
              },
              {
                key: "import",
                label: "Import",
                icon: <Upload aria-hidden="true" className="h-4 w-4" />,
                onSelect: () => setImportOpen(true),
              },
            ]}
            label="More actions"
            variant="icon"
          />
        </div>
      </div>
      <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
        <div className="flex flex-wrap items-center gap-2">
          <label className="relative">
            <Search aria-hidden="true" className="pointer-events-none absolute left-2 top-1/2 h-4 w-4 -translate-y-1/2 text-muted" />
            <span className="sr-only">Search inventory</span>
            <input
              className="iw-input w-56 pl-8"
              onChange={(event) => setSearch(event.target.value)}
              placeholder="Search inventory..."
              type="search"
              value={search}
            />
          </label>
          <div aria-label="Inventory filters" className="flex flex-wrap gap-2" role="tablist">
            {INVENTORY_FILTERS.map((option) => (
              <button
                aria-selected={filter === option.id}
                className={filter === option.id ? "iw-button-primary" : "iw-button-secondary"}
                key={option.id}
                onClick={() => setFilter(option.id)}
                role="tab"
                type="button"
              >
                {option.label}
              </button>
            ))}
          </div>
        </div>
        <div className="flex flex-wrap gap-2">
          <button
            aria-label="Expand all groups"
            className="iw-button-secondary"
            disabled={allExpanded}
            onClick={() => setExpandedGroups(new Set(groupKeys))}
            type="button"
          >
            <ChevronsDown aria-hidden="true" className="h-4 w-4" />
            Expand All
          </button>
          <button
            aria-label="Collapse all groups"
            className="iw-button-secondary"
            disabled={allCollapsed}
            onClick={() => setExpandedGroups(new Set())}
            type="button"
          >
            <ChevronsUp aria-hidden="true" className="h-4 w-4" />
            Collapse All
          </button>
        </div>
      </div>
      {exportError ? <div className="mb-4"><InlineAlert title="Export failed">{exportError}</InlineAlert></div> : null}
      {importOpen ? (
        <ImportInventoryModal
          onClose={() => setImportOpen(false)}
          onImported={load}
        />
      ) : null}
      {intent && intent !== "adjust-inventory" ? (
        <InventoryPostingPanel
          kind={intent === "opening-balance" ? "opening" : "purchase"}
          items={items}
          onCancel={() => {
            const next = new URLSearchParams(params);
            next.delete("intent");
            next.delete("typeId");
            setParams(next);
          }}
          onSaved={() => {
            const next = new URLSearchParams(params);
            next.delete("intent");
            next.delete("typeId");
            setParams(next);
            load();
          }}
          preselectedTypeId={Number(params.get("typeId") || 0)}
        />
      ) : null}
      {intent === "adjust-inventory" ? (
        <InventoryAdjustmentPanel
          items={items}
          onCancel={() => {
            const next = new URLSearchParams(params);
            next.delete("intent");
            next.delete("typeId");
            setParams(next);
          }}
          onSaved={() => {
            const next = new URLSearchParams(params);
            next.delete("intent");
            next.delete("typeId");
            setParams(next);
            load();
          }}
          preselectedTypeId={Number(params.get("typeId") || 0)}
        />
      ) : null}
      {state.status === "loading" ? <Panel>Loading Inventory...</Panel> : null}
      {state.status === "error" ? <InlineAlert title="Inventory unavailable">{state.message}</InlineAlert> : null}
      {state.status === "ready" && items.length === 0 && scope === "tracked" ? (
        <Panel>
          <EmptyState
            title="No inventory recorded"
            action={<button className="iw-button-primary" onClick={() => openPosting("opening")}>Add Opening Balance</button>}
          >
            Add starting inventory or record a purchase before historical costs are available.
          </EmptyState>
        </Panel>
      ) : null}
      {state.status === "ready" && items.length === 0 && scope === "untracked" ? (
        <Panel>
          <EmptyState title="Nothing untracked">
            ESI hasn't observed any type outside your accounting Inventory right now.
          </EmptyState>
        </Panel>
      ) : null}
      {state.status === "ready" && items.length > 0 && visibleItems.length === 0 ? (
        <Panel>
          <EmptyState title="No matching inventory">
            No items match the current search{scope === "tracked" ? " and filter" : ""}.
          </EmptyState>
        </Panel>
      ) : null}
      {visibleItems.length > 0 ? (
        <InventoryOperationalTable
          expandedGroups={expandedGroups}
          items={visibleItems}
          onSelectItem={(typeId) => {
            const next = new URLSearchParams(params);
            next.set("item", String(typeId));
            setParams(next);
          }}
          onToggleGroup={toggleGroup}
          onViewEsiHoldings={setHoldingsItem}
          selectedTypeId={selectedTypeId}
        />
      ) : null}
      {selectedItem ? (
        <InventoryInspector
          item={selectedItem}
          onChanged={load}
          onClose={() => {
            const next = new URLSearchParams(params);
            next.delete("item");
            setParams(next);
          }}
          onOpenPosting={(kind) => openPosting(kind, selectedItem.balance.key.typeId)}
          onOpenAdjustment={() => openAdjustment(selectedItem.balance.key.typeId)}
          onViewEsiHoldings={() => setHoldingsItem(selectedItem)}
          priceSourceId={sourceId || undefined}
        />
      ) : null}
      {holdingsItem ? (
        <InventoryEsiHoldingsModal
          item={holdingsItem}
          onChanged={load}
          onClose={() => setHoldingsItem(null)}
        />
      ) : null}
    </>
  );
}
