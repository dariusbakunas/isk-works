import { Columns3, Download, RefreshCw, Search, SlidersHorizontal } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";

import { exportAssets, syncAssets, type AssetBlueprintKindFilter, type AssetColumn, type AssetKindFilter, type AssetReconciliationFilter, type AssetSortColumn, type AssetSortDirection, type FlatAssetQuery, type FlatAssetRow } from "../../api/assets";
import { EveCharacterPortrait } from "../../components/eve-character-portrait";
import { EveTypeImage } from "../../components/eve-type-image";
import { CharacterName } from "../../observability/private";
import { FinanceCheckbox, FinanceFilterSection } from "../finance/finance-filter-controls";
import { formatAssetDecimal } from "./asset-format";
import { AssetInspector } from "./asset-inspector";
import { useAssetsQuery } from "./use-assets-query";

const columns: Array<{ key: AssetColumn; label: string; numeric?: boolean }> = [
  { key: "item", label: "Item" }, { key: "quantity", label: "Quantity", numeric: true },
  { key: "packagedVolume", label: "Volume", numeric: true }, { key: "character", label: "Character" },
  { key: "location", label: "Location" }, { key: "container", label: "Container" },
  { key: "group", label: "Group" }, { key: "status", label: "Status" }, { key: "observed", label: "Observed" },
];
const assetKinds: AssetKindFilter[] = ["blueprint", "material", "ship", "container", "other"];
const blueprintKinds: AssetBlueprintKindFilter[] = ["original", "copy", "unknown"];
const reconciliationStates: AssetReconciliationFilter[] = ["matched", "difference", "noAccountingRecord"];

export function AssetsWorkspacePage() {
  const [searchText, setSearchText] = useState("");
  const [search, setSearch] = useState("");
  const [connectionIds, setConnectionIds] = useState<string[]>([]);
  const [noCharacters, setNoCharacters] = useState(false);
  const [locationIds, setLocationIds] = useState<number[]>([]);
  const [noLocations, setNoLocations] = useState(false);
  const [kinds, setKinds] = useState<AssetKindFilter[]>([]);
  const [noKinds, setNoKinds] = useState(false);
  const [groupIds, setGroupIds] = useState<number[]>([]);
  const [noGroups, setNoGroups] = useState(false);
  const [blueprints, setBlueprints] = useState<AssetBlueprintKindFilter[]>([]);
  const [reconciliation, setReconciliation] = useState<AssetReconciliationFilter[]>([]);
  const [sort, setSort] = useState<AssetSortColumn>("item");
  const [order, setOrder] = useState<AssetSortDirection>("asc");
  const [visible, setVisible] = useState(new Set<AssetColumn>(columns.map(({ key }) => key)));
  const [selected, setSelected] = useState<FlatAssetRow | null>(null);
  const [syncing, setSyncing] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const workspace = useRef<HTMLElement>(null);
  useEffect(() => { const timer = window.setTimeout(() => setSearch(searchText.trim()), 250); return () => window.clearTimeout(timer); }, [searchText]);
  const query = useMemo<FlatAssetQuery>(() => ({ search, connectionIds, locationIds, assetKinds: kinds, groupIds, blueprintKinds: blueprints, reconciliationStates: reconciliation, sort, order, limit: 100 }), [blueprints, connectionIds, groupIds, kinds, locationIds, order, reconciliation, search, sort]);
  const filtersEnabled = !noCharacters && !noLocations && !noKinds && !noGroups;
  const queryKey = useMemo(() => `${filtersEnabled}:${JSON.stringify(query)}`, [filtersEnabled, query]);
  const assets = useAssetsQuery(query, filtersEnabled);
  const page = assets.page;
  useEffect(() => {
    if (workspace.current) workspace.current.scrollTop = 0;
  }, [queryKey]);
  function reset() { setSearchText(""); setSearch(""); setConnectionIds([]); setNoCharacters(false); setLocationIds([]); setNoLocations(false); setKinds([]); setNoKinds(false); setGroupIds([]); setNoGroups(false); setBlueprints([]); setReconciliation([]); setSort("item"); setOrder("asc"); }
  function changeSort(next: AssetSortColumn) { if (next === sort) setOrder((value) => value === "asc" ? "desc" : "asc"); else { setSort(next); setOrder(next === "observed" ? "desc" : "asc"); } }
  async function handleSync() { setSyncing(true); try { const outcomes = await syncAssets(connectionIds); const failed = outcomes.filter(({ succeeded }) => !succeeded); setMessage(failed.length ? `${outcomes.length - failed.length} synchronized; ${failed.length} failed.` : `Synchronized ${outcomes.length} character${outcomes.length === 1 ? "" : "s"}.`); assets.refresh(); } catch (error) { setMessage(error instanceof Error ? error.message : "Asset synchronization failed."); } finally { setSyncing(false); } }
  async function handleExport() { const blob = await exportAssets(query, columns.filter(({ key }) => visible.has(key)).map(({ key }) => key)); const url = URL.createObjectURL(blob); const anchor = document.createElement("a"); anchor.href = url; anchor.download = "isk-works-assets.csv"; anchor.click(); URL.revokeObjectURL(url); }
  return (
    <section className="relative -mx-3 -my-3 flex h-[calc(var(--iw-viewport-h)-2.75rem-var(--iw-footer-h,0px))] min-h-0 flex-col overflow-clip bg-background text-xs sm:-mx-4 sm:-my-4" data-assets-workspace ref={workspace}>
      <header className="flex min-h-10 items-center gap-2 border-b border-border bg-panel px-3"><h1 className="text-sm font-semibold">Assets</h1><span className="text-muted">Real synchronized EVE holdings</span></header>
      {page ? <Summary page={page} /> : <div className="h-[3.75rem] border-b border-border bg-panel" />}
      <div className="flex min-h-0 flex-1">
        <aside className="hidden w-52 shrink-0 overflow-y-auto border-r border-border bg-panel lg:block">
          <div className="sticky top-0 z-10 flex h-9 items-center border-b border-border bg-panel px-3"><SlidersHorizontal className="mr-1.5 h-3.5 w-3.5 text-primary" /><span className="font-semibold">Filters</span><button className="ml-auto text-[0.625rem] text-primary" onClick={reset} type="button">Reset</button></div>
          <AssetFilters blueprints={blueprints} connectionIds={connectionIds} groupIds={groupIds} kinds={kinds} locationIds={locationIds} noCharacters={noCharacters} noGroups={noGroups} noKinds={noKinds} noLocations={noLocations} page={page} reconciliation={reconciliation} setBlueprints={setBlueprints} setConnectionIds={setConnectionIds} setGroupIds={setGroupIds} setKinds={setKinds} setLocationIds={setLocationIds} setNoCharacters={setNoCharacters} setNoGroups={setNoGroups} setNoKinds={setNoKinds} setNoLocations={setNoLocations} setReconciliation={setReconciliation} />
        </aside>
        <main className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
          <div className="flex min-h-10 items-center gap-2 border-b border-border bg-panel px-2"><div className="relative min-w-48 max-w-md flex-1"><Search className="absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted" /><input aria-label="Search assets" className="iw-input min-h-7 py-1 pl-7 text-xs" onChange={(event) => setSearchText(event.target.value)} placeholder="Search item, character, location, or container" value={searchText} /></div><span className="ml-auto whitespace-nowrap text-muted">{assets.rows.length} of {page?.total ?? 0} rows</span><ColumnChooser onChange={setVisible} visible={visible} /><Action icon={<Download className="h-3.5 w-3.5" />} label="Export" onClick={() => void handleExport()} /><Action disabled={syncing} icon={<RefreshCw className={`h-3.5 w-3.5 ${syncing ? "animate-spin" : ""}`} />} label={syncing ? "Syncing" : "Sync"} onClick={() => void handleSync()} /></div>
          {message ? <div className="border-b border-border bg-primary/10 px-3 py-1.5" role="status">{message}</div> : null}
          <AssetTable assets={assets} onSelect={setSelected} onSort={changeSort} order={order} resetKey={queryKey} selected={selected} sort={sort} visible={visible} />
        </main>
      </div>
      {selected ? <AssetInspector asset={selected} onClose={() => setSelected(null)} /> : null}
    </section>
  );
}

function Summary({ page }: { page: NonNullable<ReturnType<typeof useAssetsQuery>["page"]> }) { const cells = [["Characters", page.summary.characterCount.toLocaleString()], ["Locations", page.summary.locationCount.toLocaleString()], ["Asset stacks", page.summary.stackCount.toLocaleString()], ["Packaged volume", `${formatAssetDecimal(page.summary.totalPackagedVolume)} m³`], ["Last synchronized", page.summary.latestObservedAt ? new Date(page.summary.latestObservedAt).toLocaleString() : "—"]]; return <section aria-label="Asset summary" className="grid grid-cols-5 border-b border-border bg-panel">{cells.map(([label, value]) => <div className="min-w-0 border-r border-border px-3 py-2" key={label}><div className="text-[0.625rem] text-muted">{label}</div><div className="mt-0.5 truncate font-mono text-xs font-semibold tabular-nums" title={value}>{value}</div></div>)}</section>; }

function AssetFilters({ blueprints, connectionIds, groupIds, kinds, locationIds, noCharacters, noGroups, noKinds, noLocations, page, reconciliation, setBlueprints, setConnectionIds, setGroupIds, setKinds, setLocationIds, setNoCharacters, setNoGroups, setNoKinds, setNoLocations, setReconciliation }: { blueprints: AssetBlueprintKindFilter[]; connectionIds: string[]; groupIds: number[]; kinds: AssetKindFilter[]; locationIds: number[]; noCharacters: boolean; noGroups: boolean; noKinds: boolean; noLocations: boolean; page: ReturnType<typeof useAssetsQuery>["page"]; reconciliation: AssetReconciliationFilter[]; setBlueprints: (value: AssetBlueprintKindFilter[]) => void; setConnectionIds: (value: string[]) => void; setGroupIds: (value: number[]) => void; setKinds: (value: AssetKindFilter[]) => void; setLocationIds: (value: number[]) => void; setNoCharacters: (value: boolean) => void; setNoGroups: (value: boolean) => void; setNoKinds: (value: boolean) => void; setNoLocations: (value: boolean) => void; setReconciliation: (value: AssetReconciliationFilter[]) => void }) {
  const [open, setOpen] = useState({ characters: true, locations: true, kinds: true, groups: false, blueprints: false, reconciliation: false });
  const toggle = (key: keyof typeof open) => setOpen((current) => ({ ...current, [key]: !current[key] }));
  return (
    <div className="space-y-4 px-3 py-3">
      <FinanceFilterSection id="asset-characters" onToggle={() => toggle("characters")} open={open.characters} title="Characters"><SelectionActions label="characters" onAll={() => { setNoCharacters(false); setConnectionIds([]); }} onNone={() => { setNoCharacters(true); setConnectionIds([]); }} /><div className="space-y-1.5">{page?.facets.characters.map((item) => <FinanceCheckbox checked={!noCharacters && (connectionIds.length === 0 || connectionIds.includes(item.value))} key={item.value} onChange={() => { if (noCharacters) { setNoCharacters(false); setConnectionIds([item.value]); } else setConnectionIds(toggleValue(connectionIds, item.value, page.facets.characters.map(({ value }) => value))); }}><span className="block truncate font-medium">{item.label}</span><span className="font-mono text-[0.625rem] text-muted">{item.count.toLocaleString()} stacks</span></FinanceCheckbox>)}</div></FinanceFilterSection>
      <FinanceFilterSection id="asset-locations" onToggle={() => toggle("locations")} open={open.locations} title="Locations"><SelectionActions label="locations" onAll={() => { setNoLocations(false); setLocationIds([]); }} onNone={() => { setNoLocations(true); setLocationIds([]); }} /><div className="max-h-56 space-y-1.5 overflow-y-auto">{page?.facets.locations.map((item) => { const id = Number(item.value); return <FinanceCheckbox checked={!noLocations && (locationIds.length === 0 || locationIds.includes(id))} key={item.value} onChange={() => { if (noLocations) { setNoLocations(false); setLocationIds([id]); } else setLocationIds(toggleNumber(locationIds, id, page.facets.locations.map(({ value }) => Number(value)))); }}><span className="block truncate" title={item.label}>{item.label}</span></FinanceCheckbox>; })}</div></FinanceFilterSection>
      <FinanceFilterSection id="asset-kinds" onToggle={() => toggle("kinds")} open={open.kinds} title="Asset kind"><SelectionActions label="asset kinds" onAll={() => { setNoKinds(false); setKinds([]); }} onNone={() => { setNoKinds(true); setKinds([]); }} /><div className="space-y-1.5">{assetKinds.map((value) => <FinanceCheckbox checked={!noKinds && (kinds.length === 0 || kinds.includes(value))} key={value} onChange={() => { if (noKinds) { setNoKinds(false); setKinds([value]); } else setKinds(toggleValue(kinds, value, assetKinds)); }}><span className="capitalize">{labelStatus(value)}</span></FinanceCheckbox>)}</div></FinanceFilterSection>
      <FinanceFilterSection id="asset-groups" onToggle={() => toggle("groups")} open={open.groups} title="Item group"><SelectionActions label="item groups" onAll={() => { setNoGroups(false); setGroupIds([]); }} onNone={() => { setNoGroups(true); setGroupIds([]); }} /><div className="max-h-56 space-y-1.5 overflow-y-auto">{page?.facets.groups.map((item) => { const id = Number(item.value); return <FinanceCheckbox checked={!noGroups && (groupIds.length === 0 || groupIds.includes(id))} key={item.value} onChange={() => { if (noGroups) { setNoGroups(false); setGroupIds([id]); } else setGroupIds(toggleNumber(groupIds, id, page.facets.groups.map(({ value }) => Number(value)))); }}><span className="block truncate" title={item.label}>{item.label}</span></FinanceCheckbox>; })}</div></FinanceFilterSection>
      <FinanceFilterSection id="asset-blueprints" onToggle={() => toggle("blueprints")} open={open.blueprints} title="Blueprint kind"><FilterChecks all={blueprintKinds} selected={blueprints} setSelected={setBlueprints} /></FinanceFilterSection>
      <FinanceFilterSection id="asset-reconciliation" onToggle={() => toggle("reconciliation")} open={open.reconciliation} title="Reconciliation"><FilterChecks all={reconciliationStates} selected={reconciliation} setSelected={setReconciliation} /></FinanceFilterSection>
    </div>
  );
}

function SelectionActions({ label, onAll, onNone }: { label: string; onAll: () => void; onNone: () => void }) {
  return <div className="mb-1.5 flex gap-2 text-[0.625rem]"><button aria-label={`Select all ${label}`} className="text-primary hover:text-foreground" onClick={onAll} type="button">All</button><button aria-label={`Select no ${label}`} className="text-primary hover:text-foreground" onClick={onNone} type="button">None</button></div>;
}

function FilterChecks<T extends string>({ all, selected, setSelected }: { all: T[]; selected: T[]; setSelected: (value: T[]) => void }) {
  return <div className="space-y-1.5">{all.map((value) => <FinanceCheckbox checked={selected.length === 0 || selected.includes(value)} key={value} onChange={() => setSelected(toggleValue(selected, value, all))}><span className="capitalize">{labelStatus(value)}</span></FinanceCheckbox>)}</div>;
}

function AssetTable({ assets, onSelect, onSort, order, resetKey, selected, sort, visible }: { assets: ReturnType<typeof useAssetsQuery>; onSelect: (row: FlatAssetRow) => void; onSort: (column: AssetSortColumn) => void; order: AssetSortDirection; resetKey: string; selected: FlatAssetRow | null; sort: AssetSortColumn; visible: Set<AssetColumn> }) {
  const shown = columns.filter(({ key }) => visible.has(key));
  const root = useRef<HTMLDivElement>(null);
  const sentinel = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (root.current) root.current.scrollTop = 0;
  }, [resetKey]);
  useEffect(() => {
    const viewport = root.current;
    const marker = sentinel.current;
    if (!viewport || !marker || !assets.hasMore || assets.error) return;
    const observer = new IntersectionObserver(
      (entries) => { if (entries.some(({ isIntersecting }) => isIntersecting)) assets.loadMore(); },
      { root: viewport, rootMargin: "0px 0px 240px 0px" },
    );
    observer.observe(marker);
    return () => observer.disconnect();
  }, [assets.error, assets.hasMore, assets.loadMore]);
  return (
    <div className="min-h-0 flex-1 overflow-auto" data-asset-scroll-viewport ref={root}>
      <table aria-label="Synchronized assets" className="w-full min-w-[76rem] table-fixed border-collapse">
        <thead className="sticky top-0 z-20 bg-panel-strong"><tr>{shown.map((column) => <th className={column.numeric ? "text-right" : "text-left"} key={column.key}><button className="flex w-full items-center gap-1 px-2 py-1.5 text-[0.65rem] font-semibold uppercase text-muted" onClick={() => onSort(column.key)} type="button"><span className={column.numeric ? "ml-auto" : ""}>{column.label}</span>{sort === column.key ? order === "asc" ? "↑" : "↓" : null}</button></th>)}</tr></thead>
        <tbody>{assets.rows.map((row) => <AssetRow columns={shown.map(({ key }) => key)} key={`${row.connectionId}:${row.eveItemId}`} onClick={() => onSelect(row)} row={row} selected={selected?.connectionId === row.connectionId && selected?.eveItemId === row.eveItemId} />)}</tbody>
      </table>
      {assets.loading && !assets.rows.length ? <State>Loading synchronized assets...</State> : null}
      {!assets.loading && !assets.rows.length && !assets.error ? <State>No synchronized assets match these filters.</State> : null}
      <div className="h-px" ref={sentinel} />
      {assets.loadingMore ? <State>Loading more assets...</State> : null}
      {assets.error ? <div className="flex justify-center gap-2 p-3 text-destructive"><span>{assets.error.message}</span>{assets.rows.length ? <button className="iw-button-secondary min-h-7" onClick={assets.retry} type="button">Retry</button> : null}</div> : null}
    </div>
  );
}

function AssetRow({ columns: shown, onClick, row, selected }: { columns: AssetColumn[]; onClick: () => void; row: FlatAssetRow; selected: boolean }) { const values: Record<AssetColumn, React.ReactNode> = { item: <span className="flex min-w-0 items-center gap-2"><EveTypeImage size={32} typeId={row.typeId} typeName={row.typeName ?? "Unknown item"} variation={row.blueprint?.kind === "copy" ? "bpc" : row.blueprint ? "bp" : "icon"} /><span className="truncate font-medium text-foreground">{row.typeName ?? "—"}</span></span>, quantity: row.quantity.toLocaleString(), packagedVolume: row.totalPackagedVolume ? `${formatAssetDecimal(row.totalPackagedVolume)} m³` : "—", character: <span className="flex items-center gap-1.5"><EveCharacterPortrait characterId={row.characterId} characterName={row.characterName} size={32} /><CharacterName className="truncate" name={row.characterName} /></span>, location: row.locationName ?? "—", container: row.containerName ?? "—", group: row.groupName ?? "—", status: labelStatus(row.reconciliation.state), observed: new Date(row.observedAt).toLocaleString() }; return <tr aria-selected={selected} className={`cursor-pointer border-b border-border hover:bg-panel ${selected ? "bg-primary/10" : ""}`} onClick={onClick}>{shown.map((column) => <td className={`${column === "quantity" || column === "packagedVolume" ? "text-right font-mono tabular-nums" : ""} truncate px-2 py-1.5 text-muted`} key={column} title={typeof values[column] === "string" ? String(values[column]) : undefined}>{values[column]}</td>)}</tr>; }

function ColumnChooser({ onChange, visible }: { onChange: (value: Set<AssetColumn>) => void; visible: Set<AssetColumn> }) { return <details className="relative"><summary className="inline-flex h-7 cursor-pointer list-none items-center rounded-[3px] border border-border bg-panel-strong px-2 text-[0.6875rem] font-semibold text-muted"><Columns3 className="mr-1.5 h-3.5 w-3.5" />Columns</summary><div className="absolute right-0 z-50 mt-1 grid w-64 grid-cols-2 gap-2 rounded-[3px] border border-border bg-panel-strong p-3 shadow-xl">{columns.map(({ key, label }) => <FinanceCheckbox checked={visible.has(key)} key={key} onChange={() => { const next = new Set(visible); if (next.has(key) && next.size > 1) next.delete(key); else next.add(key); onChange(next); }}><span className="text-[0.6875rem] text-muted">{label}</span></FinanceCheckbox>)}</div></details>; }
function Action({ disabled, icon, label, onClick }: { disabled?: boolean; icon: React.ReactNode; label: string; onClick: () => void }) { return <button className="inline-flex h-7 items-center gap-1.5 rounded-[3px] border border-border bg-panel-strong px-2 text-[0.6875rem] font-semibold text-muted hover:border-primary/70 hover:text-foreground disabled:opacity-60" disabled={disabled} onClick={onClick} type="button">{icon}{label}</button>; }
function State({ children }: { children: React.ReactNode }) { return <div className="p-4 text-center text-muted">{children}</div>; }
function toggleValue<T>(selected: T[], value: T, all: T[]) { const effective = selected.length ? selected : all; const next = effective.includes(value) ? effective.filter((item) => item !== value) : [...effective, value]; return next.length === all.length ? [] : next; }
function toggleNumber(selected: number[], value: number, all: number[]) { return toggleValue(selected, value, all); }
function labelStatus(value: string) { return value.replace(/([A-Z])/g, " $1").replace(/^./, (letter) => letter.toUpperCase()); }
