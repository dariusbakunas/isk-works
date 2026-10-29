import { ChevronDown, ClipboardPaste, Plus, Search } from "lucide-react";
import { useMemo, useState } from "react";
import { Link, useNavigate } from "react-router";

import { editFitTags, type FitSummary } from "../../api/fittings";
import { EveTypeImage } from "../../components/eve-type-image";
import { OperationalTable, OperationalTableRow, type OperationalColumn } from "../../components/operational-table";
import { EmptyState, InlineAlert } from "../../components/primitives";
import { PRIVATE_ATTR } from "../../observability/private";
import { EngineUnavailableHint, PilotPortrait, useEngineAvailable, useFittingsContext } from "./fit-shared";
import { TagChips, TagInput, workspaceTags } from "./fit-tags";
import { fitTagTree, fitTree, pilotsByStatus, type TreeGroup } from "./fittings-format";
import { ImportDialog } from "./import-dialog";
import { NewFitDialog } from "./new-fit-dialog";
import { useFits } from "./use-fittings";

const COLUMNS: OperationalColumn[] = [
  { key: "select", label: "", width: "32px" },
  { key: "fit", label: "Fit", width: "minmax(220px, 2fr)" },
  { key: "flyability", label: "Flyability", width: "minmax(200px, 1fr)", hideBelow: "tablet" },
];

/** Portraits of who can use the whole fit, then a count of hull-only pilots. */
export function FlyabilityChips({ fit }: { fit: FitSummary }) {
  const { canUseAll, hullOnly, anyone } = pilotsByStatus(fit.pilots);
  if (!anyone) {
    return <span className="rounded-[2px] border border-danger/50 px-1.5 py-px text-[0.625rem] text-danger">No one</span>;
  }
  return (
    <span className="flex items-center gap-1.5">
      {canUseAll.length ? (
        <span
          aria-label={`Can use all: ${canUseAll.map((pilot) => pilot.name).join(", ")}`}
          className="flex items-center gap-0.5 rounded-[2px] border border-positive/60 p-0.5"
          role="img"
        >
          {canUseAll.slice(0, 3).map((pilot) => (
            <PilotPortrait connectionId={pilot.connectionId} key={pilot.connectionId} name={pilot.name} />
          ))}
          {canUseAll.length > 3 ? <span className="px-0.5 text-[0.625rem] text-muted">+{canUseAll.length - 3}</span> : null}
        </span>
      ) : null}
      {hullOnly.length ? (
        <span
          className="rounded-[2px] border border-border px-1.5 py-px text-[0.625rem] text-muted"
          title={`Hull only: ${hullOnly.map((pilot) => pilot.name).join(", ")}`}
        >
          +{hullOnly.length} hull only
        </span>
      ) : null}
    </span>
  );
}

type TreeMode = "hulls" | "tags";
const TREE_MODE_KEY = "iskworks:fittings-tree";

function readTreeMode(): TreeMode {
  try {
    return window.localStorage.getItem(TREE_MODE_KEY) === "tags" ? "tags" : "hulls";
  } catch {
    return "hulls";
  }
}

/** Which tree the browser shows, remembered per browser. */
function useTreeMode(): [TreeMode, (mode: TreeMode) => void] {
  const [mode, setMode] = useState(readTreeMode);
  const choose = (next: TreeMode) => {
    setMode(next);
    try {
      window.localStorage.setItem(TREE_MODE_KEY, next);
    } catch {
      // Storage off: it just won't be remembered.
    }
  };
  return [mode, choose];
}

function TreeModeToggle({ mode, onChange }: { mode: TreeMode; onChange: (mode: TreeMode) => void }) {
  const option = (value: TreeMode, label: string) => (
    <button
      aria-pressed={mode === value}
      className={`h-6 flex-1 rounded-[3px] text-[0.6875rem] font-semibold ${
        mode === value ? "bg-panel-strong text-foreground" : "text-muted hover:text-foreground"
      }`}
      onClick={() => onChange(value)}
      type="button"
    >
      {label}
    </button>
  );
  return (
    <div aria-label="Group fits by" className="mb-2 flex gap-0.5 rounded-md border border-border bg-background p-0.5" role="group">
      {option("hulls", "Hulls")}
      {option("tags", "Tags")}
    </div>
  );
}

// Groups start collapsed; a search opens every group so its matches show.
function HullTree({ groups, searching }: { groups: TreeGroup[]; searching: boolean }) {
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const isOpen = (name: string) => searching || expanded.has(name);
  const toggle = (name: string) =>
    setExpanded((current) => {
      const next = new Set(current);
      if (next.has(name)) next.delete(name);
      else next.add(name);
      return next;
    });
  return (
    <nav aria-label="Hulls and fits" className="text-xs">
      {groups.map((group) => (
        <div key={group.name}>
          <button
            aria-expanded={isOpen(group.name)}
            className="flex w-full items-center gap-1 px-1 py-1.5 text-left font-semibold text-muted hover:text-foreground"
            disabled={searching}
            onClick={() => toggle(group.name)}
            type="button"
          >
            <ChevronDown aria-hidden="true" className={`h-3 w-3 transition ${isOpen(group.name) ? "" : "-rotate-90"}`} />
            {group.name}
          </button>
          {!isOpen(group.name)
            ? null
            : group.ships.map((ship) => (
                <div className="pl-3" key={ship.typeId}>
                  <div className="flex items-center gap-1.5 py-1">
                    <EveTypeImage size={24} typeId={ship.typeId} typeName={ship.name} />
                    <span className="flex-1 text-foreground">{ship.name}</span>
                    <span className="font-mono text-muted" data-testid={`ship-count-${ship.typeId}`}>{ship.fits.length}</span>
                  </div>
                  {ship.fits.map((fit) => (
                    <Link
                      {...PRIVATE_ATTR}
                      className="block truncate rounded px-2 py-1 pl-8 text-muted hover:bg-panel-strong hover:text-foreground"
                      key={fit.id}
                      to={fit.id}
                    >
                      {fit.name}
                    </Link>
                  ))}
                </div>
              ))}
        </div>
      ))}
    </nav>
  );
}

const flies = (fit: FitSummary, connectionId: string) =>
  fit.pilots.some(
    (pilot) => pilot.connectionId === connectionId && (pilot.status === "canUseAll" || pilot.status === "canFlyHull"),
  );

export function FitList({ revision, onChanged }: { revision: number; onChanged: () => void }) {
  const navigate = useNavigate();
  const { roster } = useFittingsContext();
  const engineAvailable = useEngineAvailable();
  const { data, error, loading } = useFits(revision);
  const [query, setQuery] = useState("");
  const [flyableBy, setFlyableBy] = useState("");
  const [importing, setImporting] = useState(false);
  const [creating, setCreating] = useState(false);
  const [mode, setMode] = useTreeMode();
  const [selected, setSelected] = useState<Set<string>>(new Set());

  const visible = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return (data ?? []).filter(
      (fit) =>
        (!needle ||
          fit.name.toLowerCase().includes(needle) ||
          (fit.ship.name ?? "").toLowerCase().includes(needle) ||
          fit.tags.some((tag) => tag.toLowerCase().includes(needle))) &&
        (!flyableBy || flies(fit, flyableBy)),
    );
  }, [data, flyableBy, query]);
  const tree = useMemo(() => (mode === "tags" ? fitTagTree(visible) : fitTree(visible)), [mode, visible]);
  const anyTags = (data ?? []).some((fit) => fit.tags.length > 0);
  // Only fits shown count: a filter hides a fit from the bulk actions too.
  const selectedFits = visible.filter((fit) => selected.has(fit.id));
  const toggleSelected = (id: string) =>
    setSelected((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  const hulls = new Set(visible.map((fit) => fit.ship.typeId)).size;

  return (
    <div className="grid gap-3 lg:grid-cols-[260px_minmax(0,1fr)]">
      <aside className="iw-panel p-2">
        <TreeModeToggle mode={mode} onChange={setMode} />
        <label className="flex items-center gap-2 rounded-md border border-border bg-background px-2">
          <Search aria-hidden="true" className="h-3.5 w-3.5 text-muted" />
          <input
            aria-label="Search hulls and fits"
            className="h-8 w-full bg-transparent text-xs outline-none"
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search hulls & fits"
            type="search"
            value={query}
          />
        </label>
        <div className="mt-2">
          {mode === "tags" && !anyTags && data?.length ? (
            <p className="px-1 pb-1.5 text-[0.6875rem] text-muted">Tag fits from a fit's header or by selecting them in the table.</p>
          ) : null}
          {/* Each tree keeps its own open groups. */}
          <HullTree groups={tree} key={mode} searching={query.trim() !== ""} />
        </div>
      </aside>

      <section className="min-w-0 space-y-3">
        <div className="flex flex-wrap items-end justify-between gap-2">
          <div>
            <h1 className="iw-title">Fittings</h1>
            <p className="iw-muted">
              {visible.length} saved {visible.length === 1 ? "fit" : "fits"} · {hulls} {hulls === 1 ? "hull" : "hulls"}
            </p>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <label className="flex items-center gap-1.5 text-xs text-muted">
              Flyable by
              <select
                {...PRIVATE_ATTR}
                className="iw-input"
                onChange={(event) => setFlyableBy(event.target.value)}
                value={flyableBy}
              >
                <option value="">All characters</option>
                {roster.map((character) => (
                  <option key={character.connectionId} value={character.connectionId}>{character.characterName}</option>
                ))}
              </select>
            </label>
            <button
              className="iw-button-secondary gap-1.5"
              disabled={!engineAvailable}
              onClick={() => setCreating(true)}
              type="button"
            >
              <Plus aria-hidden="true" className="h-3.5 w-3.5" />
              New fit
            </button>
            <button
              className="iw-button-primary gap-1.5"
              disabled={!engineAvailable}
              onClick={() => setImporting(true)}
              type="button"
            >
              <ClipboardPaste aria-hidden="true" className="h-3.5 w-3.5" />
              Import from clipboard
            </button>
          </div>
        </div>

        {engineAvailable ? null : <EngineUnavailableHint />}
        {error ? <InlineAlert title="Couldn't load fits">{error.message}</InlineAlert> : null}

        {!loading && data && data.length === 0 ? (
          <EmptyState
            action={
              engineAvailable ? (
                <button className="iw-button-primary" onClick={() => setImporting(true)} type="button">
                  Import from clipboard
                </button>
              ) : undefined
            }
            title="Paste a fit from the game or pyfa"
          >
            Copy a fit in the game's fitting window (or pyfa's EFT export) and paste it here to see who can fly it and
            what it does with each character's skills.
          </EmptyState>
        ) : null}

        {visible.length ? (
          <div className="iw-panel">
            <BulkBar
              allTags={workspaceTags(data ?? [])}
              onChanged={onChanged}
              onClear={() => setSelected(new Set())}
              onSelectAll={(all) => setSelected(all ? new Set(visible.map((fit) => fit.id)) : new Set())}
              selected={selectedFits}
              shown={visible.length}
            />
            <OperationalTable ariaLabel="Fits" columns={COLUMNS} onSelectRow={(id) => navigate(id)} selectedRowKey={null}>
              {visible.map((fit) => (
                <FitRow
                  checked={selected.has(fit.id)}
                  fit={fit}
                  key={fit.id}
                  onOpen={() => navigate(fit.id)}
                  onToggle={() => toggleSelected(fit.id)}
                />
              ))}
            </OperationalTable>
          </div>
        ) : null}
      </section>

      {creating ? (
        <NewFitDialog
          onClose={() => setCreating(false)}
          onCreated={(id) => {
            setCreating(false);
            onChanged();
            navigate(`${id}?edit=1`);
          }}
        />
      ) : null}
      {importing ? (
        <ImportDialog
          onClose={() => setImporting(false)}
          onImported={(id) => {
            setImporting(false);
            onChanged();
            navigate(id);
          }}
        />
      ) : null}
    </div>
  );
}

/** Select all, and with fits selected, tag or untag them all at once. */
function BulkBar({
  selected,
  shown,
  allTags,
  onSelectAll,
  onClear,
  onChanged,
}: {
  selected: FitSummary[];
  shown: number;
  allTags: string[];
  onSelectAll: (all: boolean) => void;
  onClear: () => void;
  onChanged: () => void;
}) {
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const held = workspaceTags(selected);
  const edit = async (change: { add?: string[]; remove?: string[] }) => {
    setError(null);
    setSaving(true);
    try {
      await editFitTags({ fitIds: selected.map((fit) => fit.id), ...change });
      onChanged();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "Couldn't change the tags.");
    } finally {
      setSaving(false);
    }
  };
  return (
    <div className="flex min-h-9 flex-wrap items-center gap-2 border-b border-border px-2 py-1 text-xs">
      <label className="flex items-center gap-1.5 text-muted">
        <input
          aria-label="Select all fits shown"
          checked={selected.length > 0 && selected.length === shown}
          onChange={(event) => onSelectAll(event.target.checked)}
          ref={(input) => {
            if (input) input.indeterminate = selected.length > 0 && selected.length < shown;
          }}
          type="checkbox"
        />
        {selected.length ? null : "Select fits to tag them"}
      </label>
      {selected.length ? (
        <>
          <span className="font-semibold text-foreground">{selected.length} selected</span>
          <TagInput
            autoFocus={false}
            label="Tag to add"
            onSubmit={(tag) => void edit({ add: [tag] })}
            placeholder="Add tag"
            suggestions={allTags}
          />
          <select
            aria-label="Remove tag"
            className="iw-input h-7 min-h-0 w-auto py-0 text-[0.6875rem]"
            disabled={saving || held.length === 0}
            onChange={(event) => {
              if (event.target.value) void edit({ remove: [event.target.value] });
            }}
            value=""
          >
            <option value="">Remove tag…</option>
            {held.map((tag) => (
              <option key={tag} value={tag}>{tag}</option>
            ))}
          </select>
          <button aria-label="Clear selection" className="text-muted hover:text-foreground" onClick={onClear} type="button">
            Clear
          </button>
          {error ? <span className="text-danger" role="alert">{error}</span> : null}
        </>
      ) : null}
    </div>
  );
}

function FitRow({ fit, checked, onOpen, onToggle }: { fit: FitSummary; checked: boolean; onOpen: () => void; onToggle: () => void }) {
  return (
    <OperationalTableRow
      cells={{
        select: (
          <input
            aria-label={`Select ${fit.name}`}
            checked={checked}
            onChange={onToggle}
            // Ticking a row selects it; it doesn't open the fit.
            onClick={(event) => event.stopPropagation()}
            onKeyDown={(event) => event.stopPropagation()}
            type="checkbox"
          />
        ),
        fit: (
          <span className="flex min-w-0 items-center gap-2">
            <EveTypeImage size={32} typeId={fit.ship.typeId} typeName={fit.ship.name ?? "Ship"} />
            <span className="min-w-0">
              <span {...PRIVATE_ATTR} className="block truncate font-semibold text-foreground">{fit.name}</span>
              <span className="block truncate text-[0.6875rem] text-muted">
                {fit.ship.name ?? `Type ${fit.ship.typeId}`}
                {fit.ship.groupName ? ` · ${fit.ship.groupName}` : ""}
              </span>
              <TagChips small tags={fit.tags} />
            </span>
          </span>
        ),
        flyability: <FlyabilityChips fit={fit} />,
      }}
      interactive
      onActivate={onOpen}
      rowKey={fit.id}
    />
  );
}
