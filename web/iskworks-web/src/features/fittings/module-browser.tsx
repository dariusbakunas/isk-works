import { Plus, Search } from "lucide-react";
import { useEffect, useId, useState } from "react";

import {
  searchFitCatalogue,
  type CatalogueItem,
  type FitItemView,
  type PilotChoice,
  type SlotKind,
} from "../../api/fittings";
import { searchTypes, type TypeSearchResult } from "../../api/sde";
import { formatStat } from "./fittings-format";

/** What a browser row carries when dragged onto the module table. */
export const DRAG_TYPE = "application/x-iskworks-type";

export interface DraggedType {
  typeId: number;
  name: string;
  slot: SlotKind | null;
}

const SEARCH_DELAY_MS = 250;
const SLOT_LETTERS: Partial<Record<SlotKind, string>> = { high: "H", med: "M", low: "L", rig: "R", subsystem: "S", service: "V" };

type Tab = "modules" | "charges" | "drones" | "cargo";
const TABS: Array<{ id: Tab; label: string }> = [
  { id: "modules", label: "Modules" },
  { id: "charges", label: "Charges" },
  { id: "drones", label: "Drones" },
  { id: "cargo", label: "Cargo" },
];

function useDebounced<T>(value: T): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const timer = window.setTimeout(() => setDebounced(value), SEARCH_DELAY_MS);
    return () => window.clearTimeout(timer);
  }, [value]);
  return debounced;
}

function ResultRow({
  entry,
  action,
  onAct,
}: {
  entry: CatalogueItem;
  action: "Add" | "Load";
  onAct: () => void;
}) {
  const dragged: DraggedType = { typeId: entry.typeId, name: entry.name, slot: entry.slot };
  return (
    <li
      className={`flex min-h-9 items-center gap-2 border-t border-border px-2 py-1 text-xs ${entry.canUse === false ? "opacity-60" : ""}`}
      draggable={entry.kind === "module"}
      onDoubleClick={onAct}
      onDragStart={(event) => {
        event.dataTransfer.setData(DRAG_TYPE, JSON.stringify(dragged));
        event.dataTransfer.effectAllowed = "copy";
      }}
    >
      {entry.slot ? (
        <span className="grid h-5 w-5 shrink-0 place-items-center rounded border border-border text-[0.625rem] text-primary">
          {SLOT_LETTERS[entry.slot] ?? "?"}
        </span>
      ) : null}
      <span className="min-w-0 flex-1">
        <span className="block truncate">{entry.name}</span>
        {entry.kind === "module" ? (
          <span className="block font-mono text-[0.625rem] text-muted">
            {formatStat(entry.cpu, 1)} tf · {formatStat(entry.powergrid, 1)} MW
            {entry.calibration ? ` · ${formatStat(entry.calibration)} cal` : ""}
          </span>
        ) : null}
      </span>
      <button
        aria-label={`${action} ${entry.name}`}
        className="rounded p-1 text-muted hover:bg-panel-strong hover:text-primary"
        onClick={onAct}
        type="button"
      >
        <Plus aria-hidden="true" className="h-3.5 w-3.5" />
      </button>
    </li>
  );
}

/** Results under their deepest market group, in the order they came. */
function Grouped({ entries, render }: { entries: CatalogueItem[]; render: (entry: CatalogueItem) => JSX.Element }) {
  const groups: Array<{ name: string; entries: CatalogueItem[] }> = [];
  for (const entry of entries) {
    const name = entry.marketPath.at(-1) ?? "Other";
    const group = groups.at(-1);
    if (group?.name === name) group.entries.push(entry);
    else groups.push({ name, entries: [entry] });
  }
  return (
    <>
      {groups.map((group, position) => (
        <li key={`${group.name}-${position}`}>
          <p className="px-2 pb-0.5 pt-2 text-[0.625rem] uppercase tracking-wide text-muted">{group.name}</p>
          <ul>{group.entries.map(render)}</ul>
        </li>
      ))}
    </>
  );
}

/**
 * The editor's left column: modules the ship fits, charges for the module
 * picked in the table, drones and cargo, each added with a double-click,
 * the + button, or (modules) by dragging onto a slot.
 */
export function ModuleBrowser({
  shipTypeId,
  pilot,
  selected,
  message,
  onAddModule,
  onLoadCharge,
  onAddStack,
}: {
  shipTypeId: number;
  pilot: PilotChoice;
  /** The table row picked, for its charges. */
  selected: FitItemView | null;
  /** What the last action reported (a full rack); shown as a status. */
  message: string | null;
  onAddModule: (entry: CatalogueItem) => void;
  onLoadCharge: (entry: CatalogueItem) => void;
  onAddStack: (type: { typeId: number; name: string }, kind: "drone_bay" | "cargo") => void;
}) {
  const tabsId = useId();
  const [tab, setTab] = useState<Tab>("modules");
  const [query, setQuery] = useState("");
  const [usableOnly, setUsableOnly] = useState(false);
  const [entries, setEntries] = useState<CatalogueItem[]>([]);
  const [cargo, setCargo] = useState<TypeSearchResult[]>([]);
  const [truncated, setTruncated] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const debounced = useDebounced(query);
  const characterPilot = pilot !== "allV" && pilot !== "allZero";
  const chargesFor = tab === "charges" && selected && selected.slotIndex !== null ? selected.typeId : null;

  useEffect(() => {
    setError(null);
    if (tab === "cargo") {
      if (!debounced.trim()) {
        setCargo([]);
        return;
      }
      let live = true;
      searchTypes(debounced)
        .then((results) => live && setCargo(results.filter((result) => result.published)))
        .catch((reason: unknown) => live && setError(reason instanceof Error ? reason.message : "Search failed."));
      return () => {
        live = false;
      };
    }
    if (tab === "charges" && chargesFor === null) {
      setEntries([]);
      return;
    }
    const controller = new AbortController();
    const search =
      tab === "modules"
        ? ({ kind: "modules", shipTypeId } as const)
        : tab === "charges"
          ? ({ kind: "charges", moduleTypeId: chargesFor! } as const)
          : ({ kind: "drones" } as const);
    searchFitCatalogue(search, { q: tab === "charges" ? undefined : debounced, pilot }, controller.signal)
      .then((view) => {
        setEntries(view.entries);
        setTruncated(view.truncated);
      })
      .catch((reason: unknown) => {
        if (!controller.signal.aborted) setError(reason instanceof Error ? reason.message : "Search failed.");
      });
    return () => controller.abort();
  }, [tab, debounced, shipTypeId, chargesFor, pilot]);

  const shown = usableOnly && characterPilot ? entries.filter((entry) => entry.canUse !== false) : entries;

  return (
    <aside aria-label="Module browser" className="iw-panel flex max-h-[calc(100vh-6rem)] flex-col xl:sticky xl:top-3">
      <div aria-label="Browse" className="grid grid-cols-4 border-b border-border" id={tabsId} role="tablist">
        {TABS.map((entry) => (
          <button
            aria-selected={tab === entry.id}
            className={`h-9 border-b-2 text-xs ${tab === entry.id ? "border-primary text-foreground" : "border-transparent text-muted hover:text-foreground"}`}
            key={entry.id}
            onClick={() => {
              setTab(entry.id);
              setQuery("");
            }}
            role="tab"
            type="button"
          >
            {entry.label}
          </button>
        ))}
      </div>
      <div className="space-y-2 p-2">
        {tab === "charges" ? null : (
          <label className="flex items-center gap-2 rounded-md border border-border bg-background px-2">
            <Search aria-hidden="true" className="h-3.5 w-3.5 text-muted" />
            <input
              aria-label={tab === "cargo" ? "Search items" : tab === "drones" ? "Search drones" : "Search modules"}
              className="h-8 w-full bg-transparent text-xs outline-none"
              onChange={(event) => setQuery(event.target.value)}
              placeholder={tab === "cargo" ? "Any item, by name" : "Search"}
              type="search"
              value={query}
            />
          </label>
        )}
        {characterPilot && tab !== "cargo" ? (
          <label className="flex items-center gap-1.5 text-[0.6875rem] text-muted">
            <input checked={usableOnly} onChange={(event) => setUsableOnly(event.target.checked)} type="checkbox" />
            Pilot can use
          </label>
        ) : null}
        {message ? (
          <p className="text-[0.6875rem] text-warning" role="status">{message}</p>
        ) : null}
        {error ? <p className="text-[0.6875rem] text-danger">{error}</p> : null}
      </div>
      <ul className="min-h-0 flex-1 overflow-y-auto">
        {tab === "charges" && chargesFor === null ? (
          <li className="px-3 py-2 text-xs text-muted">Pick a module in the table to see its charges.</li>
        ) : null}
        {tab === "cargo"
          ? cargo.map((result) => (
              <li className="flex min-h-9 items-center gap-2 border-t border-border px-2 py-1 text-xs" key={result.typeId}>
                <span className="min-w-0 flex-1 truncate">{result.typeName}</span>
                <button
                  aria-label={`Add ${result.typeName}`}
                  className="rounded p-1 text-muted hover:bg-panel-strong hover:text-primary"
                  onClick={() => onAddStack({ typeId: result.typeId, name: result.typeName }, "cargo")}
                  type="button"
                >
                  <Plus aria-hidden="true" className="h-3.5 w-3.5" />
                </button>
              </li>
            ))
          : (
            <Grouped
              entries={shown}
              render={(entry) => (
                <ResultRow
                  action={tab === "charges" ? "Load" : "Add"}
                  entry={entry}
                  key={entry.typeId}
                  onAct={() =>
                    tab === "charges"
                      ? onLoadCharge(entry)
                      : tab === "drones"
                        ? onAddStack(entry, "drone_bay")
                        : onAddModule(entry)
                  }
                />
              )}
            />
          )}
      </ul>
      {truncated && tab !== "cargo" ? (
        <p className="border-t border-border px-3 py-1.5 text-[0.625rem] text-muted">First 200 shown; search to narrow.</p>
      ) : null}
    </aside>
  );
}
