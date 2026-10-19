import { Search, SlidersHorizontal } from "lucide-react";
import { useEffect, useMemo, useState, type ReactNode } from "react";
import { Link, useSearchParams } from "react-router";

import type {
  BlueprintFilterOption,
  BlueprintLibraryKind,
  BlueprintLibraryRow,
  BlueprintLibrarySort,
} from "../../../api/industry";
import { EveTypeImage } from "../../../components/eve-type-image";
import {
  OperationalTable,
  OperationalTableGroup,
  OperationalTableRow,
  type OperationalColumn,
} from "../../../components/operational-table";
import { Badge } from "../../../components/primitives";
import { CharacterName } from "../../../observability/private";
import { FinanceCheckbox, FinanceFilterSection } from "../../finance/finance-filter-controls";
import { BuildsSectionTabs } from "../builds/builds-section-tabs";
import { BlueprintInspector } from "./blueprint-inspector";
import { formatEfficiency, formatRuns, groupBlueprints, hasUnlimitedRuns, isUsable } from "./blueprint-format";
import {
  DEFAULT_VIEW,
  blueprintLibraryQuery,
  blueprintLibraryViewParams,
  hasActiveFilters,
  parseBlueprintLibraryView,
  type BlueprintGrouping,
  type BlueprintLibraryView,
} from "./blueprint-library-url-state";
import { useBlueprintLibrary } from "./use-blueprint-library";

const COLUMN_DEFS: Array<{ key: string; label: string; width: string; sort: BlueprintLibrarySort; numeric?: boolean; hideBelow?: "tablet" | "desktop" }> = [
  { key: "blueprint", label: "Blueprint", width: "minmax(240px,1fr)", sort: "blueprint" },
  { key: "product", label: "Product", width: "180px", sort: "product", hideBelow: "tablet" },
  { key: "kind", label: "Kind", width: "76px", sort: "kind" },
  { key: "me", label: "ME", width: "48px", sort: "me", numeric: true },
  { key: "te", label: "TE", width: "48px", sort: "te", numeric: true },
  { key: "runs", label: "Runs", width: "64px", sort: "runs", numeric: true },
  { key: "character", label: "Character", width: "140px", sort: "character", hideBelow: "desktop" },
  { key: "location", label: "Location", width: "220px", sort: "location", hideBelow: "desktop" },
];

const KIND_OPTIONS: Array<{ value: BlueprintLibraryKind; label: string }> = [
  { value: "original", label: "Originals" },
  { value: "copy", label: "Copies" },
];

export function BlueprintsPage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const view = useMemo(() => parseBlueprintLibraryView(searchParams), [searchParams]);
  const [searchText, setSearchText] = useState(view.search);
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const library = useBlueprintLibrary(blueprintLibraryQuery(view));

  function update(patch: Partial<BlueprintLibraryView>) {
    setSearchParams(blueprintLibraryViewParams({ ...view, ...patch }), { replace: true });
  }

  useEffect(() => {
    const timer = window.setTimeout(() => {
      if (searchText.trim() !== view.search) update({ search: searchText.trim() });
    }, 250);
    return () => window.clearTimeout(timer);
    // Only typing schedules a search; URL changes are already applied.
  }, [searchText]);

  function reset() {
    setSearchText("");
    setSearchParams(blueprintLibraryViewParams({ ...DEFAULT_VIEW, groupBy: view.groupBy }), { replace: true });
  }

  function toggleSort(sort: BlueprintLibrarySort) {
    if (view.sort === sort) update({ order: view.order === "asc" ? "desc" : "asc" });
    else update({ sort, order: sort === "me" || sort === "te" || sort === "runs" || sort === "observed" ? "desc" : "asc" });
  }

  const columns = useMemo<OperationalColumn[]>(() => COLUMN_DEFS.map((column) => ({
    key: column.key,
    label: column.label,
    width: column.width,
    align: column.numeric ? "right" : "left",
    numeric: column.numeric,
    hideBelow: column.hideBelow,
    sort: { direction: view.sort === column.sort ? view.order : null, onToggle: () => toggleSort(column.sort) },
  })), [view]);
  const groups = useMemo(() => groupBlueprints(library.rows, view.groupBy), [library.rows, view.groupBy]);
  const selected = library.rows.find((row) => row.observationId === selectedKey) ?? null;
  const filtered = hasActiveFilters(view);
  const ownsNothing = library.summary !== null && library.summary.originals + library.summary.copies === 0;

  return (
    <section
      className="relative -mx-3 -my-3 flex h-[calc(var(--iw-viewport-h)-2.75rem-var(--iw-footer-h,0px))] min-h-0 flex-col overflow-clip bg-background text-xs sm:-mx-4 sm:-my-4"
      data-blueprints-workspace
    >
      <header className="flex min-h-10 items-center gap-2 border-b border-border bg-panel px-3">
        <h1 className="text-sm font-semibold">Builds</h1>
        <BuildsSectionTabs className="ml-2 self-stretch" />
        <span className="ml-2 hidden text-muted sm:inline">Blueprints your characters own</span>
      </header>
      <Summary library={library} />
      <div className="flex min-h-0 flex-1">
        <aside aria-label="Blueprint filters" className="hidden w-52 shrink-0 overflow-y-auto border-r border-border bg-panel lg:block">
          <div className="sticky top-0 z-10 flex h-9 items-center border-b border-border bg-panel px-3">
            <SlidersHorizontal aria-hidden="true" className="mr-1.5 h-3.5 w-3.5 text-primary" />
            <span className="font-semibold">Filters</span>
            <button className="ml-auto text-[0.625rem] text-primary disabled:text-muted" disabled={!filtered} onClick={reset} type="button">Reset</button>
          </div>
          <Filters library={library} onChange={update} view={view} />
        </aside>
        <main className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
          <div className="flex min-h-10 flex-wrap items-center gap-2 border-b border-border bg-panel px-2 py-1">
            <div className="relative min-w-48 max-w-md flex-1">
              <Search aria-hidden="true" className="absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted" />
              <input
                aria-label="Search blueprints"
                className="iw-input min-h-7 py-1 pl-7 text-xs"
                onChange={(event) => setSearchText(event.target.value)}
                placeholder="Search blueprint, product, character, or location"
                value={searchText}
              />
            </div>
            <label className="flex items-center gap-1.5 text-muted">
              Group by
              <select
                aria-label="Group blueprints by"
                className="iw-input min-h-7 w-auto py-0.5 text-xs"
                onChange={(event) => update({ groupBy: event.target.value as BlueprintGrouping })}
                value={view.groupBy}
              >
                <option value="category">Product category</option>
                <option value="location">Location</option>
                <option value="none">Nothing</option>
              </select>
            </label>
            <span className="ml-auto whitespace-nowrap text-muted" role="status">
              {library.rows.length.toLocaleString()} of {library.total.toLocaleString()} blueprints
            </span>
          </div>
          {library.error ? <div className="border-b border-border bg-destructive/10 px-3 py-1.5 text-destructive" role="alert">{library.error}</div> : null}
          <div aria-busy={library.loading} className={`min-h-0 flex-1 overflow-auto transition-opacity ${library.loading && library.rows.length > 0 ? "opacity-60" : ""}`}>
            {library.loading && library.rows.length === 0 ? (
              <State>Loading blueprints...</State>
            ) : ownsNothing ? (
              <State>
                No synced blueprints yet. Blueprints sync with each character&apos;s assets; check{" "}
                <Link className="text-primary hover:underline" to="/characters">Characters</Link>.
              </State>
            ) : library.rows.length === 0 ? (
              <State>
                No blueprints match these filters.{" "}
                <button className="text-primary hover:underline" onClick={reset} type="button">Clear filters</button>
              </State>
            ) : (
              <OperationalTable
                ariaLabel="Owned blueprints"
                columns={columns}
                onSelectRow={setSelectedKey}
                selectedRowKey={selectedKey}
              >
                {groups.map((group) => (
                  <OperationalTableGroup
                    groupKey={group.key}
                    itemCount={group.rows.length}
                    key={group.key}
                    label={group.label}
                    labelCase={view.groupBy === "location" ? "normal" : "upper"}
                  >
                    {group.rows.map((row) => (
                      <OperationalTableRow
                        cells={cells(row)}
                        key={row.observationId}
                        rowKey={row.observationId}
                        status={isUsable(row) ? "neutral" : "warning"}
                      />
                    ))}
                  </OperationalTableGroup>
                ))}
              </OperationalTable>
            )}
            {library.rows.length < library.total && !library.loading ? (
              <div className="flex justify-center p-3">
                <button className="iw-button-secondary min-h-7" disabled={library.loadingMore} onClick={library.loadMore} type="button">
                  {library.loadingMore ? "Loading..." : "Load more"}
                </button>
              </div>
            ) : null}
          </div>
        </main>
      </div>
      {selected ? <BlueprintInspector blueprint={selected} onClose={() => setSelectedKey(null)} /> : null}
    </section>
  );
}

function cells(row: BlueprintLibraryRow): Record<string, ReactNode> {
  return {
    blueprint: (
      <span className="flex min-w-0 items-center gap-2">
        <EveTypeImage size={32} typeId={row.blueprintTypeId} typeName={row.blueprintName} variation={row.kind === "copy" ? "bpc" : "bp"} />
        <span className="min-w-0">
          <span className="block truncate font-medium text-foreground" title={row.blueprintName}>{row.blueprintName}</span>
          {row.usedByBuilds.length > 0 ? (
            <span className="block text-[0.625rem] text-muted">
              Used by {row.usedByBuilds.length} build{row.usedByBuilds.length === 1 ? "" : "s"}
            </span>
          ) : null}
        </span>
      </span>
    ),
    product: <span className="block truncate" title={row.productGroupName ?? undefined}>{row.productName ?? "—"}</span>,
    kind: row.activity === "reaction" ? (
      <span title="Reaction formula">
        <Badge square tone="reaction">Reaction</Badge>
      </span>
    ) : (
      <span title={row.kind === "original" ? "Original" : row.kind === "copy" ? "Copy" : "ESI did not say whether this is an original or a copy"}>
        <Badge square tone={row.kind === "original" ? "primary" : row.kind === "copy" ? "batch" : "muted"}>{row.kind === "original" ? "BPO" : row.kind === "copy" ? "BPC" : "?"}</Badge>
      </span>
    ),
    me: formatEfficiency(row, row.materialEfficiency),
    te: formatEfficiency(row, row.timeEfficiency),
    runs: <span title={hasUnlimitedRuns(row) ? "Unlimited runs" : undefined}>{formatRuns(row)}</span>,
    character: row.characterName ? <CharacterName className="block truncate" name={row.characterName} /> : "—",
    location: <span className="block truncate" title={row.locationName ?? undefined}>{row.locationName ?? `Location ${row.locationId}`}</span>,
  };
}

function Summary({ library }: { library: ReturnType<typeof useBlueprintLibrary> }) {
  const summary = library.summary;
  const items: Array<[string, string]> = [
    ["Originals", summary ? summary.originals.toLocaleString() : "—"],
    ["Copies", summary ? summary.copies.toLocaleString() : "—"],
    ["Characters", summary ? summary.characters.toLocaleString() : "—"],
    ["Last synced", summary?.latestObservedAt ? new Date(summary.latestObservedAt).toLocaleString() : "—"],
  ];
  return (
    <section aria-label="Blueprint summary" className="grid grid-cols-2 border-b border-border bg-panel sm:grid-cols-4">
      {items.map(([label, value]) => (
        <div className="min-w-0 border-r border-border px-3 py-2" key={label}>
          <div className="text-[0.625rem] text-muted">{label}</div>
          <div className="mt-0.5 truncate font-mono text-xs font-semibold tabular-nums" title={value}>{value}</div>
        </div>
      ))}
    </section>
  );
}

function Filters({ library, onChange, view }: { library: ReturnType<typeof useBlueprintLibrary>; onChange: (patch: Partial<BlueprintLibraryView>) => void; view: BlueprintLibraryView }) {
  const [open, setOpen] = useState({ kind: true, categories: true, groups: false, characters: true, locations: false, efficiency: false });
  const toggle = (key: keyof typeof open) => setOpen((current) => ({ ...current, [key]: !current[key] }));
  const facets = library.facets;
  // Groups follow the chosen categories, so "Ship" narrows to hull groups.
  const groups = facets?.groups.filter((group) => view.categoryIds.length === 0
    || (group.categoryId !== null && view.categoryIds.includes(group.categoryId))) ?? [];
  return (
    <div className="space-y-4 px-3 py-3">
      <FinanceFilterSection id="blueprint-kind" onToggle={() => toggle("kind")} open={open.kind} title="Blueprint">
        <div className="space-y-1.5">
          {KIND_OPTIONS.map((option) => (
            <FinanceCheckbox checked={view.kinds.includes(option.value)} key={option.value} onChange={() => onChange({ kinds: toggled(view.kinds, option.value) })}>
              {option.label}
            </FinanceCheckbox>
          ))}
          <FinanceCheckbox checked={view.usableOnly} onChange={() => onChange({ usableOnly: !view.usableOnly })}>
            <span className="block">Usable only</span>
            <span className="text-[0.625rem] text-muted">Originals and copies with runs left</span>
          </FinanceCheckbox>
          <FinanceCheckbox checked={view.activity === "reaction"} onChange={() => onChange({ activity: view.activity === "reaction" ? null : "reaction" })}>
            Reaction formulas only
          </FinanceCheckbox>
        </div>
      </FinanceFilterSection>
      <OptionSection
        id="blueprint-categories"
        onToggle={() => toggle("categories")}
        open={open.categories}
        options={facets?.categories ?? []}
        selected={view.categoryIds.map(String)}
        setSelected={(values) => onChange({
          categoryIds: values.map(Number),
          // Drop group picks that no longer belong to a chosen category.
          groupIds: view.groupIds.filter((id) => values.length === 0 || facets?.groups.some((group) => group.value === String(id) && group.categoryId !== null && values.includes(String(group.categoryId)))),
        })}
        title="Product category"
      />
      <OptionSection
        id="blueprint-groups"
        onToggle={() => toggle("groups")}
        open={open.groups}
        options={groups}
        selected={view.groupIds.map(String)}
        setSelected={(values) => onChange({ groupIds: values.map(Number) })}
        title="Product group"
      />
      <OptionSection
        id="blueprint-characters"
        isPrivate
        onToggle={() => toggle("characters")}
        open={open.characters}
        options={facets?.characters ?? []}
        selected={view.connectionIds}
        setSelected={(values) => onChange({ connectionIds: values })}
        title="Character"
      />
      <OptionSection
        id="blueprint-locations"
        onToggle={() => toggle("locations")}
        open={open.locations}
        options={facets?.locations ?? []}
        selected={view.locationIds.map(String)}
        setSelected={(values) => onChange({ locationIds: values.map(Number) })}
        title="Location"
      />
      <FinanceFilterSection id="blueprint-efficiency" onToggle={() => toggle("efficiency")} open={open.efficiency} title="Research">
        <div className="grid grid-cols-2 gap-2">
          <MinimumSelect label="Min ME" max={10} onChange={(minMe) => onChange({ minMe })} value={view.minMe} />
          <MinimumSelect label="Min TE" max={20} onChange={(minTe) => onChange({ minTe })} step={2} value={view.minTe} />
        </div>
      </FinanceFilterSection>
    </div>
  );
}

/** Nothing ticked means everything; ticking narrows to the ticked options. */
function OptionSection({ id, isPrivate = false, onToggle, open, options, selected, setSelected, title }: {
  id: string;
  isPrivate?: boolean;
  onToggle: () => void;
  open: boolean;
  options: BlueprintFilterOption[];
  selected: string[];
  setSelected: (values: string[]) => void;
  title: string;
}) {
  return (
    <FinanceFilterSection id={id} meta={selected.length > 0 ? selected.length : undefined} onToggle={onToggle} open={open} title={title}>
      {options.length === 0 ? <p className="text-[0.625rem] text-muted">None</p> : (
        <div className="max-h-56 space-y-1.5 overflow-y-auto">
          {options.map((option) => (
            <FinanceCheckbox checked={selected.includes(option.value)} key={option.value} onChange={() => setSelected(toggled(selected, option.value))}>
              <span className="block truncate" data-private={isPrivate ? "" : undefined} title={option.label}>{option.label}</span>
              <span className="font-mono text-[0.625rem] text-muted">{option.count.toLocaleString()}</span>
            </FinanceCheckbox>
          ))}
        </div>
      )}
    </FinanceFilterSection>
  );
}

function MinimumSelect({ label, max, onChange, step = 1, value }: { label: string; max: number; onChange: (value: number | null) => void; step?: number; value: number | null }) {
  const values = Array.from({ length: Math.floor(max / step) }, (_, index) => (index + 1) * step);
  return (
    <label className="grid gap-1 text-[0.625rem] text-muted">
      {label}
      <select
        aria-label={label}
        className="iw-input min-h-7 py-0.5 text-xs"
        onChange={(event) => onChange(event.target.value === "" ? null : Number(event.target.value))}
        value={value === null ? "" : String(value)}
      >
        <option value="">Any</option>
        {values.map((option) => <option key={option} value={option}>{option}+</option>)}
      </select>
    </label>
  );
}

function State({ children }: { children: ReactNode }) {
  return <div className="p-4 text-center text-muted">{children}</div>;
}

function toggled<T>(values: T[], value: T): T[] {
  return values.includes(value) ? values.filter((entry) => entry !== value) : [...values, value];
}
