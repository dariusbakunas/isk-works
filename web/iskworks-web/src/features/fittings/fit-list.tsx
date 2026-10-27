import { ChevronDown, ClipboardPaste, Plus, Search } from "lucide-react";
import { useMemo, useState } from "react";
import { Link, useNavigate } from "react-router";

import type { FitSummary } from "../../api/fittings";
import { EveTypeImage } from "../../components/eve-type-image";
import { OperationalTable, OperationalTableRow, type OperationalColumn } from "../../components/operational-table";
import { EmptyState, InlineAlert } from "../../components/primitives";
import { PRIVATE_ATTR } from "../../observability/private";
import { EngineUnavailableHint, PilotPortrait, useEngineAvailable, useFittingsContext } from "./fit-shared";
import { fitTree, pilotsByStatus, type TreeGroup } from "./fittings-format";
import { ImportDialog } from "./import-dialog";
import { NewFitDialog } from "./new-fit-dialog";
import { useFits } from "./use-fittings";

const COLUMNS: OperationalColumn[] = [
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

function HullTree({ groups }: { groups: TreeGroup[] }) {
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const toggle = (name: string) =>
    setCollapsed((current) => {
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
            aria-expanded={!collapsed.has(group.name)}
            className="flex w-full items-center gap-1 px-1 py-1.5 text-left font-semibold text-muted hover:text-foreground"
            onClick={() => toggle(group.name)}
            type="button"
          >
            <ChevronDown aria-hidden="true" className={`h-3 w-3 transition ${collapsed.has(group.name) ? "-rotate-90" : ""}`} />
            {group.name}
          </button>
          {collapsed.has(group.name)
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

  const visible = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return (data ?? []).filter(
      (fit) =>
        (!needle ||
          fit.name.toLowerCase().includes(needle) ||
          (fit.ship.name ?? "").toLowerCase().includes(needle)) &&
        (!flyableBy || flies(fit, flyableBy)),
    );
  }, [data, flyableBy, query]);
  const tree = useMemo(() => fitTree(visible), [visible]);
  const hulls = new Set(visible.map((fit) => fit.ship.typeId)).size;

  return (
    <div className="grid gap-3 lg:grid-cols-[260px_minmax(0,1fr)]">
      <aside className="iw-panel p-2">
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
          <HullTree groups={tree} />
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
            <OperationalTable ariaLabel="Fits" columns={COLUMNS} onSelectRow={(id) => navigate(id)} selectedRowKey={null}>
              {visible.map((fit) => (
                <FitRow fit={fit} key={fit.id} onOpen={() => navigate(fit.id)} />
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

function FitRow({ fit, onOpen }: { fit: FitSummary; onOpen: () => void }) {
  return (
    <OperationalTableRow
      cells={{
        fit: (
          <span className="flex min-w-0 items-center gap-2">
            <EveTypeImage size={32} typeId={fit.ship.typeId} typeName={fit.ship.name ?? "Ship"} />
            <span className="min-w-0">
              <span {...PRIVATE_ATTR} className="block truncate font-semibold text-foreground">{fit.name}</span>
              <span className="block truncate text-[0.6875rem] text-muted">
                {fit.ship.name ?? `Type ${fit.ship.typeId}`}
                {fit.ship.groupName ? ` · ${fit.ship.groupName}` : ""}
              </span>
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
