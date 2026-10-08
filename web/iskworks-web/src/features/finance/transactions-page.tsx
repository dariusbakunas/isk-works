import {
  Columns3,
  Download,
  RefreshCw,
  Save,
  Search,
  SlidersHorizontal,
  Star,
  Trash2,
  WalletCards,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { useSearchParams } from "react-router";

import {
  deleteSavedFinanceFilter,
  exportFinanceTransactions,
  listSavedFinanceFilters,
  saveFinanceFilter,
  syncFinance,
  type FinanceColumn,
  type FinanceDisplayColumn,
  type FinanceInventoryRecording,
  type FinanceDirection,
  type FinanceFilter,
  type FinanceQuery,
  type FinanceSortColumn,
  type FinanceTransaction,
  type FinanceTransactionPage,
  type FinanceTransactionType,
  type SavedFinanceFilter,
  type SortDirection,
} from "../../api/finance";
import { MoneyAmount, formatIskCompact } from "../../components/money";
import { CharacterName, Private } from "../../observability/private";
import { FinanceCheckbox, FinanceDateField, FinanceFilterSection } from "./finance-filter-controls";
import { InventoryRecordingCell } from "./inventory-recording-cell";
import { parseTransactionsUrlFilter, parseTransactionsUrlLabels } from "./transactions-url-state";
import { useFinanceTransactions } from "./use-finance-transactions";

const allColumns: Array<{ key: FinanceDisplayColumn; label: string; numeric?: boolean }> = [
  { key: "time", label: "Time" },
  { key: "character", label: "Character" },
  { key: "transactionType", label: "Type" },
  { key: "item", label: "Item" },
  { key: "quantity", label: "Qty", numeric: true },
  { key: "unitPrice", label: "Unit price", numeric: true },
  { key: "totalPrice", label: "Total", numeric: true },
  { key: "direction", label: "Direction" },
  { key: "counterparty", label: "Client" },
  { key: "location", label: "Where" },
  { key: "region", label: "Region" },
  { key: "inventory", label: "Inventory" },
];

const defaultVisible = new Set<FinanceDisplayColumn>(allColumns.map(({ key }) => key));

function isoDate(date: Date) {
  return date.toISOString().slice(0, 10);
}

function daysAgo(days: number) {
  const value = new Date();
  value.setUTCDate(value.getUTCDate() - days);
  return isoDate(value);
}

const initialFilter: FinanceFilter = {
  connectionIds: [],
  dateFrom: daysAgo(29),
  dateTo: isoDate(new Date()),
  search: null,
  transactionTypes: ["marketBuy", "marketSell"],
  direction: "all",
  page: 1,
  pageSize: 100,
};

export function TransactionsPage() {
  const [searchParams] = useSearchParams();
  // A link from Analytics can pre-set filters; everything else keeps the defaults.
  const [filter, setFilter] = useState<FinanceFilter>(() => ({
    ...initialFilter,
    ...parseTransactionsUrlFilter(searchParams),
  }));
  const [labels] = useState(() => parseTransactionsUrlLabels(searchParams));
  const [searchText, setSearchText] = useState("");
  const [sort, setSort] = useState<FinanceSortColumn>("time");
  const [order, setOrder] = useState<SortDirection>("desc");
  const [savedFilters, setSavedFilters] = useState<SavedFinanceFilter[]>([]);
  const [visibleColumns, setVisibleColumns] = useState(defaultVisible);
  const [saveName, setSaveName] = useState("");
  const [savingFilter, setSavingFilter] = useState(false);
  const [syncing, setSyncing] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    const timer = window.setTimeout(() => {
      const search = searchText.trim() || null;
      setFilter((current) => current.search === search ? current : { ...current, search, page: 1 });
    }, 250);
    return () => window.clearTimeout(timer);
  }, [searchText]);

  const query = useMemo<FinanceQuery>(() => ({ filter, sort, order }), [filter, order, sort]);
  const transactions = useFinanceTransactions(query);
  const { page, rows } = transactions;
  useEffect(() => {
    void listSavedFinanceFilters().then(setSavedFilters).catch(() => undefined);
  }, []);

  function updateFilter(patch: Partial<FinanceFilter>) {
    setFilter((current) => ({ ...current, ...patch, page: 1 }));
  }

  function changeSort(column: FinanceSortColumn) {
    if (sort === column) setOrder((current) => current === "asc" ? "desc" : "asc");
    else {
      setSort(column);
      setOrder(column === "time" ? "desc" : "asc");
    }
  }

  async function handleSync() {
    setSyncing(true);
    try {
      const outcomes = await syncFinance(filter.connectionIds);
      const failed = outcomes.filter((outcome) => !outcome.succeeded);
      setMessage(failed.length === 0
        ? `Synchronized ${outcomes.length} character${outcomes.length === 1 ? "" : "s"}.`
        : `${outcomes.length - failed.length} synchronized; ${failed.length} failed.`);
      setFilter((current) => ({ ...current }));
    } catch (error) {
      setMessage(errorMessage(error));
    } finally {
      setSyncing(false);
    }
  }

  async function handleExport() {
    try {
      // The Inventory column is display-only; the CSV keeps its transaction columns.
      const columns = allColumns
        .filter(({ key }) => visibleColumns.has(key))
        .map(({ key }) => key)
        .filter((key): key is FinanceColumn => key !== "inventory");
      const blob = await exportFinanceTransactions(query, columns);
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement("a");
      anchor.href = url;
      anchor.download = `isk-works-transactions-${isoDate(new Date())}.csv`;
      anchor.click();
      URL.revokeObjectURL(url);
    } catch (error) {
      setMessage(errorMessage(error));
    }
  }

  async function handleSaveFilter() {
    const name = saveName.trim();
    if (!name) return;
    setSavingFilter(true);
    try {
      const saved = await saveFinanceFilter(name, filter);
      setSavedFilters((current) => [...current.filter(({ id }) => id !== saved.id), saved]);
      setSaveName("");
    } catch (error) {
      setMessage(errorMessage(error));
    } finally {
      setSavingFilter(false);
    }
  }

  return (
    <div className="finance-transactions flex min-h-0 flex-1 flex-col overflow-hidden text-xs">
      {page ? <SummaryStrip page={page} /> : <div className="h-[4.5rem] border-b border-border bg-panel" />}
      <div className="flex min-h-0 flex-1">
        <FiltersPanel
          filter={filter}
          page={page}
          savedFilters={savedFilters}
          saveName={saveName}
          saving={savingFilter}
          onDeleteSaved={async (saved) => {
            await deleteSavedFinanceFilter(saved.id);
            setSavedFilters((current) => current.filter(({ id }) => id !== saved.id));
          }}
          onFilter={updateFilter}
          onReset={() => { setFilter(initialFilter); setSearchText(""); }}
          onSave={() => void handleSaveFilter()}
          onSaveName={setSaveName}
          onUseSaved={(saved) => { setFilter({ ...saved.filter, page: 1 }); setSearchText(saved.filter.search ?? ""); }}
        />
        <main className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
          <div className="flex min-h-10 items-center gap-2 border-b border-border bg-panel px-2">
            <div className="relative min-w-48 max-w-md flex-1">
              <Search aria-hidden="true" className="absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted" />
              <input aria-label="Search transactions" className="iw-input min-h-7 py-1 pl-7 text-xs" onChange={(event) => setSearchText(event.target.value)} placeholder="Search item, character, counterparty, location" role="searchbox" value={searchText} />
            </div>
            <span className="ml-auto whitespace-nowrap text-muted">{rows.length} of {page?.totalCount ?? 0} rows</span>
            <ColumnChooser columns={visibleColumns} onChange={setVisibleColumns} />
            <button className="inline-flex h-7 items-center rounded-[3px] border border-border bg-panel-strong px-2 text-[0.6875rem] font-semibold text-muted transition hover:border-primary/70 hover:text-foreground focus-visible:outline focus-visible:outline-2 focus-visible:outline-primary" onClick={() => void handleExport()} type="button"><Download aria-hidden="true" className="mr-1.5 h-3.5 w-3.5" />Export</button>
            <button className="inline-flex h-7 items-center rounded-[3px] border border-border bg-panel-strong px-2 text-[0.6875rem] font-semibold text-muted transition hover:border-primary/70 hover:text-foreground focus-visible:outline focus-visible:outline-2 focus-visible:outline-primary disabled:cursor-not-allowed disabled:opacity-60" disabled={syncing} onClick={() => void handleSync()} type="button"><RefreshCw aria-hidden="true" className={`mr-1.5 h-3.5 w-3.5 ${syncing ? "animate-spin" : ""}`} />{syncing ? "Syncing" : "Sync"}</button>
          </div>
          {message ? <div className="border-b border-border bg-primary/10 px-3 py-1.5 text-foreground" role="status">{message}</div> : null}
          <ActiveExtraFilters filter={filter} labels={labels} onClear={(patch) => updateFilter(patch)} />
          <TransactionTable
            columns={visibleColumns}
            error={transactions.error}
            hasMore={transactions.hasMore}
            loadingInitial={transactions.loadingInitial}
            loadingMore={transactions.loadingMore}
            order={order}
            rows={rows}
            sort={sort}
            onInventoryChange={transactions.patchInventoryRecording}
            onLoadMore={transactions.loadMore}
            onRetry={transactions.retryLoadMore}
            onSort={changeSort}
          />
        </main>
      </div>
    </div>
  );
}

/** Filters that only a deep link can set (the rail has no control for them). */
function ActiveExtraFilters({ filter, labels, onClear }: {
  filter: FinanceFilter;
  labels: { item?: string; location?: string };
  onClear: (patch: Partial<FinanceFilter>) => void;
}) {
  const chips: Array<{ key: string; label: string; clear: Partial<FinanceFilter> }> = [];
  if (filter.category) chips.push({ key: "category", label: `Category: ${filter.category}`, clear: { category: null } });
  if (filter.typeId != null) chips.push({ key: "item", label: labels.item ? `Item: ${labels.item}` : `Item #${filter.typeId}`, clear: { typeId: null } });
  if (filter.locationId != null) chips.push({ key: "location", label: labels.location ? `Location: ${labels.location}` : `Location #${filter.locationId}`, clear: { locationId: null } });
  if (filter.excludeInventoryBuys) chips.push({ key: "inventory", label: "Excluding Inventory buys", clear: { excludeInventoryBuys: false } });
  if (chips.length === 0) return null;
  return (
    <div className="flex flex-wrap items-center gap-1.5 border-b border-border bg-panel px-3 py-1.5">
      <span className="text-muted">Filtered by</span>
      {chips.map((chip) => (
        <span className="inline-flex items-center gap-1 rounded-[3px] border border-primary/50 bg-primary/10 px-1.5 py-0.5 text-primary" key={chip.key}>
          {chip.label}
          <button aria-label={`Remove ${chip.key} filter`} className="leading-none text-primary/80 hover:text-primary" onClick={() => onClear(chip.clear)} type="button">×</button>
        </span>
      ))}
    </div>
  );
}

function SummaryStrip({ page }: { page: FinanceTransactionPage }) {
  const walletBalanceAvailable = page.availableCharacters.some(({ walletBalance }) => walletBalance !== null);
  const cells = [
    ["Wallet balance", page.summary.walletBalance, `${page.availableCharacters.length} characters`],
    ["Income", page.summary.income, "Filtered period"],
    ["Expenses", page.summary.expenses, "Filtered period"],
    ["Net ISK", page.summary.netIsk, "Filtered period"],
    ["Transactions", String(page.summary.transactionCount), "Filtered rows"],
    ["Average daily", page.summary.averageDailyIsk, "Filtered period"],
  ];
  return (
    <section aria-label="Transaction summary" className="grid grid-cols-3 border-b border-border bg-panel lg:grid-cols-6" data-private="">

      {cells.map(([label, value, detail], index) => (
        <div className="min-w-0 border-r border-border px-3 py-2" key={label}>
          <div className="text-[0.65rem] text-muted">{label}</div>
          <div className={`mt-0.5 truncate font-mono text-sm font-semibold tabular-nums ${index === 1 || index === 3 || index === 5 ? "text-positive" : index === 2 ? "text-destructive" : "text-foreground"}`} title={value}>
            {index === 0 && !walletBalanceAvailable
              ? "Unavailable"
              : index === 4
                ? Number(value).toLocaleString()
                : <MoneyAmount maximumSummaryFractionDigits={2} value={value} />}
          </div>
          <div className="mt-0.5 text-[0.625rem] text-muted">{label === "Wallet balance" ? `${page.availableCharacters.length} character${page.availableCharacters.length === 1 ? "" : "s"}` : detail}</div>
        </div>
      ))}
    </section>
  );
}

function FiltersPanel({ filter, page, savedFilters, saveName, saving, onDeleteSaved, onFilter, onReset, onSave, onSaveName, onUseSaved }: {
  filter: FinanceFilter;
  page: FinanceTransactionPage | null;
  savedFilters: SavedFinanceFilter[];
  saveName: string;
  saving: boolean;
  onDeleteSaved: (saved: SavedFinanceFilter) => Promise<void>;
  onFilter: (patch: Partial<FinanceFilter>) => void;
  onReset: () => void;
  onSave: () => void;
  onSaveName: (name: string) => void;
  onUseSaved: (saved: SavedFinanceFilter) => void;
}) {
  const [openSections, setOpenSections] = useState({
    characters: true,
    dateRange: true,
    direction: true,
    type: true,
    saved: true,
  });
  function toggleSection(section: keyof typeof openSections) {
    setOpenSections((current) => ({ ...current, [section]: !current[section] }));
  }
  function toggleConnection(id: string) {
    const selected = filter.connectionIds.length === 0
      ? page?.availableCharacters.map(({ connectionId }) => connectionId) ?? []
      : filter.connectionIds;
    const next = selected.includes(id)
      ? selected.filter((value) => value !== id)
      : [...selected, id];
    onFilter({ connectionIds: next });
  }
  function toggleType(type: FinanceTransactionType) {
    const next = filter.transactionTypes.includes(type)
      ? filter.transactionTypes.filter((value) => value !== type)
      : [...filter.transactionTypes, type];
    onFilter({ transactionTypes: next });
  }
  return (
    <aside className="hidden w-52 shrink-0 overflow-y-auto border-r border-border bg-panel lg:block">
      <div className="sticky top-0 z-10 flex h-9 items-center border-b border-border bg-panel px-3">
        <SlidersHorizontal aria-hidden="true" className="mr-1.5 h-3.5 w-3.5 text-primary" />
        <span className="font-semibold">Filters</span>
        <button className="ml-auto text-[0.625rem] font-medium text-primary hover:underline focus-visible:outline focus-visible:outline-2 focus-visible:outline-primary" onClick={onReset} type="button">Reset</button>
      </div>
      <div className="space-y-4 px-3 py-3">
        <FinanceFilterSection id="finance-characters" onToggle={() => toggleSection("characters")} open={openSections.characters} title="Characters">
          <div className="space-y-1.5">
            {page?.availableCharacters.map((character) => (
              <FinanceCheckbox
                checked={filter.connectionIds.length === 0 || filter.connectionIds.includes(character.connectionId)}
                key={character.connectionId}
                onChange={() => toggleConnection(character.connectionId)}
              >
                <CharacterName className="block truncate font-medium text-foreground" name={character.characterName} />
                <span className="block font-mono text-[0.625rem] text-muted" data-private="">{character.walletBalance ? `${formatIskCompact(character.walletBalance)} ISK` : "Balance unavailable"}</span>
              </FinanceCheckbox>
            ))}
          </div>
        </FinanceFilterSection>

        <FinanceFilterSection id="finance-date-range" onToggle={() => toggleSection("dateRange")} open={openSections.dateRange} title="Date range">
          <div className="space-y-2">
            <FinanceDateField id="finance-date-from" label="From" onChange={(value) => onFilter({ dateFrom: value || null })} value={filter.dateFrom ?? ""} />
            <FinanceDateField id="finance-date-to" label="To" onChange={(value) => onFilter({ dateTo: value || null })} value={filter.dateTo ?? ""} />
            <div className="grid grid-cols-4 gap-1 pt-0.5">
              {[["Today", 0], ["7d", 6], ["30d", 29], ["90d", 89]].map(([label, days]) => (
                <button className="rounded-[3px] border border-border bg-panel-strong px-1 py-1 text-[0.625rem] text-muted transition hover:border-primary hover:text-foreground focus-visible:outline focus-visible:outline-2 focus-visible:outline-primary" key={label} onClick={() => onFilter({ dateFrom: daysAgo(Number(days)), dateTo: isoDate(new Date()) })} type="button">{label}</button>
              ))}
            </div>
          </div>
        </FinanceFilterSection>

        <FinanceFilterSection id="finance-direction" onToggle={() => toggleSection("direction")} open={openSections.direction} title="Direction">
          <div className="grid grid-cols-3 gap-1">
            {(["all", "income", "expense"] as FinanceDirection[]).map((direction) => (
              <button className={`rounded-[3px] border px-1 py-1 text-[0.625rem] capitalize transition focus-visible:outline focus-visible:outline-2 focus-visible:outline-primary ${filter.direction === direction ? "border-primary bg-primary/10 text-primary" : "border-border bg-panel-strong text-muted hover:border-primary/70 hover:text-foreground"}`} key={direction} onClick={() => onFilter({ direction })} type="button">{direction}</button>
            ))}
          </div>
        </FinanceFilterSection>

        <FinanceFilterSection id="finance-type" meta={`${filter.transactionTypes.length}/2`} onToggle={() => toggleSection("type")} open={openSections.type} title="Type">
          <div className="space-y-1.5">
            <div className="flex gap-2 text-[0.625rem]">
              <button className="text-primary hover:underline focus-visible:outline focus-visible:outline-2 focus-visible:outline-primary" onClick={() => onFilter({ transactionTypes: ["marketBuy", "marketSell"] })} type="button">All</button>
              <button className="text-primary hover:underline focus-visible:outline focus-visible:outline-2 focus-visible:outline-primary" onClick={() => onFilter({ transactionTypes: [] })} type="button">None</button>
            </div>
            <FinanceCheckbox checked={filter.transactionTypes.includes("marketBuy")} onChange={() => toggleType("marketBuy")}><span className="font-medium text-destructive">Market buy</span></FinanceCheckbox>
            <FinanceCheckbox checked={filter.transactionTypes.includes("marketSell")} onChange={() => toggleType("marketSell")}><span className="font-medium text-positive">Market sell</span></FinanceCheckbox>
          </div>
        </FinanceFilterSection>

        <FinanceFilterSection id="finance-saved" onToggle={() => toggleSection("saved")} open={openSections.saved} title="Saved filters">
          <div className="space-y-1">
            {savedFilters.map((saved) => (
              <div className="flex items-center rounded-[3px] border border-border bg-panel-strong" key={saved.id}>
                <button className="flex min-w-0 flex-1 items-center gap-1.5 px-2 py-1.5 text-left text-muted transition hover:text-foreground focus-visible:outline focus-visible:outline-2 focus-visible:outline-primary" onClick={() => onUseSaved(saved)} type="button">
                  <Star aria-hidden="true" className="h-3 w-3 shrink-0 text-warning" />
                  <Private className="truncate">{saved.name}</Private>
                </button>
                <button aria-label="Delete saved filter" className="p-1.5 text-muted transition hover:text-destructive focus-visible:outline focus-visible:outline-2 focus-visible:outline-primary" onClick={() => void onDeleteSaved(saved)} title="Delete saved filter" type="button"><Trash2 className="h-3 w-3" /></button>
              </div>
            ))}
            <div className="flex pt-1">
              <input aria-label="Saved filter name" className="iw-input min-h-8 rounded-r-none text-xs" onChange={(event) => onSaveName(event.target.value)} placeholder="Filter name" value={saveName} />
              <button aria-label="Save current filter" className="rounded-r border border-l-0 border-border px-2 text-primary transition hover:bg-primary/10 disabled:text-muted" disabled={saving || !saveName.trim()} onClick={onSave} title="Save current filter" type="button"><Save className="h-3.5 w-3.5" /></button>
            </div>
          </div>
        </FinanceFilterSection>
      </div>
    </aside>
  );
}

function ColumnChooser({ columns, onChange }: { columns: Set<FinanceDisplayColumn>; onChange: (columns: Set<FinanceDisplayColumn>) => void }) {
  const [open, setOpen] = useState(false);
  return (
    <details className="relative" onToggle={(event) => setOpen(event.currentTarget.open)}>
      <summary aria-expanded={open} aria-label="Columns" className={`inline-flex h-7 cursor-pointer list-none items-center rounded-[3px] border bg-panel-strong px-2 text-[0.6875rem] font-semibold transition focus-visible:outline focus-visible:outline-2 focus-visible:outline-primary ${open ? "border-primary text-primary" : "border-border text-muted hover:border-primary/70 hover:text-foreground"}`} role="button"><Columns3 aria-hidden="true" className="mr-1.5 h-3.5 w-3.5" />Columns</summary>
      <div className="absolute right-0 z-40 mt-1 w-72 rounded-[3px] border border-border bg-panel-strong p-3 shadow-xl">
        <p className="mb-2 text-[0.6875rem] font-semibold text-muted">Visible columns</p>
        <div className="grid grid-cols-2 gap-x-4 gap-y-2" data-testid="finance-column-options">
          {allColumns.map(({ key, label }) => (
            <FinanceCheckbox
              checked={columns.has(key)}
              key={key}
              onChange={() => {
                const next = new Set(columns);
                if (next.has(key) && next.size > 1) next.delete(key);
                else next.add(key);
                onChange(next);
              }}
            >
              <span className="text-[0.6875rem] font-medium text-muted">{label}</span>
            </FinanceCheckbox>
          ))}
        </div>
      </div>
    </details>
  );
}

function TransactionTable({ columns, error, hasMore, loadingInitial, loadingMore, order, rows, sort, onInventoryChange, onLoadMore, onRetry, onSort }: {
  columns: Set<FinanceDisplayColumn>;
  error: Error | null;
  hasMore: boolean;
  loadingInitial: boolean;
  loadingMore: boolean;
  order: SortDirection;
  rows: FinanceTransaction[];
  sort: FinanceSortColumn;
  onLoadMore: () => void;
  onRetry: () => void;
  onSort: (column: FinanceSortColumn) => void;
  onInventoryChange: (observationId: string, recording: FinanceInventoryRecording) => void;
}) {
  const visible = allColumns.filter(({ key }) => columns.has(key));
  const scrollRootRef = useRef<HTMLDivElement>(null);
  const sentinelRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const root = scrollRootRef.current;
    const sentinel = sentinelRef.current;
    if (!root || !sentinel || !hasMore || error) return;
    const observer = new IntersectionObserver(
      (entries) => { if (entries.some(({ isIntersecting }) => isIntersecting)) onLoadMore(); },
      { root, rootMargin: "0px 0px 240px 0px" },
    );
    observer.observe(sentinel);
    return () => observer.disconnect();
  }, [error, hasMore, onLoadMore]);

  return (
    <div className="min-h-0 flex-1 overflow-auto" data-finance-scroll-viewport ref={scrollRootRef}>
      <table aria-label="Wallet transactions" className="finance-table w-full min-w-[72rem] table-fixed border-collapse">
        <thead className="sticky top-0 z-20 bg-panel-strong"><tr>{visible.map((column) => <th className={column.numeric ? "text-right" : "text-left"} data-finance-column={column.key} key={column.key}>{column.key === "inventory" ? <span className="block px-2 py-1.5 text-[0.65rem] font-semibold uppercase text-muted">{column.label}</span> : <button className="flex w-full items-center gap-1 px-2 py-1.5 text-left text-[0.65rem] font-semibold uppercase text-muted" onClick={() => onSort(column.key as FinanceSortColumn)} type="button"><span className={column.numeric ? "ml-auto" : ""}>{column.label}</span>{sort === column.key ? <span aria-label={order === "asc" ? "ascending" : "descending"}>{order === "asc" ? "↑" : "↓"}</span> : null}</button>}</th>)}</tr></thead>
        <tbody data-private="">{rows.map((row) => <TransactionRow columns={visible.map(({ key }) => key)} key={row.observationId} onInventoryChange={onInventoryChange} row={row} />)}</tbody>
      </table>
      {loadingInitial && rows.length === 0
        ? <div className="p-4 text-center text-muted">Loading transactions...</div>
        : error && rows.length === 0
          ? <div className="p-4 text-center text-destructive">{error.message}</div>
          : rows.length === 0
            ? <div className="p-6 text-center text-muted"><WalletCards className="mx-auto mb-2 h-5 w-5" />No real EVE market transactions match these filters.</div>
            : null}
      <div className="h-px" data-finance-load-sentinel ref={sentinelRef} />
      {loadingMore ? <div className="p-3 text-center text-muted">Loading more transactions...</div> : null}
      {error && rows.length > 0 ? <div className="flex items-center justify-center gap-2 p-3 text-destructive"><span>{error.message}</span><button className="iw-button-secondary min-h-7" onClick={onRetry} type="button">Retry</button></div> : null}
      {!loadingInitial && !loadingMore && !hasMore && rows.length > 0 ? <div className="p-3 text-center text-muted">All matching transactions loaded.</div> : null}
    </div>
  );
}

function TransactionRow({ columns, row, onInventoryChange }: { columns: FinanceDisplayColumn[]; row: FinanceTransaction; onInventoryChange: (observationId: string, recording: FinanceInventoryRecording) => void }) {
  const incoming = row.transactionType === "marketSell";
  const values: Record<FinanceDisplayColumn, React.ReactNode> = {
    time: <time dateTime={row.transactedAt}>{new Intl.DateTimeFormat(undefined, { dateStyle: "short", timeStyle: "short" }).format(new Date(row.transactedAt))}</time>,
    character: row.characterName,
    transactionType: <span className={`rounded border px-1.5 py-0.5 ${incoming ? "border-positive/30 text-positive" : "border-destructive/30 text-destructive"}`}>{incoming ? "Market sell" : "Market buy"}</span>,
    item: <span className="font-medium text-foreground">{row.typeName}</span>,
    quantity: row.quantity.toLocaleString(),
    unitPrice: <MoneyAmount showCurrency={false} value={row.unitPrice} />,
    totalPrice: <MoneyAmount className={incoming ? "text-positive" : "text-destructive"} showCurrency={false} value={row.totalPrice} />,
    direction: <span className={incoming ? "text-positive" : "text-destructive"}>{incoming ? "↗ Income" : "↘ Expense"}</span>,
    counterparty: row.counterpartyName ?? "—",
    location: row.locationName ?? "—",
    region: row.regionName ?? "—",
    inventory: <InventoryRecordingCell onChange={onInventoryChange} row={row} />,
  };
  return <tr className="border-b border-border hover:bg-panel">{columns.map((column) => <td className={`${["quantity", "unitPrice", "totalPrice"].includes(column) ? "text-right font-mono tabular-nums" : ""} truncate px-2 py-1.5 text-muted`} data-finance-column={column} key={column} title={typeof values[column] === "string" ? String(values[column]) : undefined}>{values[column]}</td>)}</tr>;
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : "Finance request failed.";
}
