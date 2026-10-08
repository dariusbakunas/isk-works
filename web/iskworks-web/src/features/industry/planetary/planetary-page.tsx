import { useCallback, useEffect, useMemo, useState, type DragEvent, type ReactNode } from "react";
import { useSearchParams } from "react-router";
import { AlertTriangle, ArrowDown, ArrowUp, Clock, Globe2, MoreHorizontal, RefreshCw, Search, Settings, WifiOff } from "lucide-react";

import { syncCharacter } from "../../../api/characters";
import { beginAuthorization } from "../../../api/esi";
import {
  getPlanetary,
  getPlanetaryPreferences,
  savePlanetaryPreferences,
  type Planet,
  type PlanetaryCharacter,
  type PlanetaryOverview,
  type PlanetaryPreferences,
  type PlanetExport,
} from "../../../api/planetary";
import { DropdownMenu } from "../../../components/dropdown-menu";
import { EveCharacterPortrait } from "../../../components/eve-character-portrait";
import { formatIskCompact, formatIskSummary } from "../../../components/money";
import {
  OperationalTable,
  OperationalTableGroup,
  OperationalTableRow,
  type OperationalColumn,
} from "../../../components/operational-table";
import { EmptyState, InlineAlert, Panel } from "../../../components/primitives";
import { formatRelativeAge } from "../../characters/characters-formatters";
import { apiMessage, type LoadState } from "../inventory/shared";
import {
  CcBadge,
  ExcludedCell,
  ExportsCell,
  ExtractionCell,
  ImportsCell,
  IskCell,
  PlanetNameCell,
  ProductionCell,
  StorageCell,
} from "./planetary-cells";
import { countdown, formatHours, isStale, toneText } from "./planetary-format";

const COLUMNS: OperationalColumn[] = [
  { key: "planet", label: "Planet", width: "minmax(170px,1.1fr)", sticky: true },
  { key: "cc", label: "CC", width: "44px" },
  { key: "extraction", label: "Extraction", width: "minmax(120px,0.8fr)" },
  { key: "production", label: "Production", width: "minmax(130px,0.9fr)" },
  { key: "imports", label: "Imports (qty/h · lasts)", width: "minmax(190px,1.2fr)" },
  { key: "exports", label: "Exports (uph)", width: "minmax(160px,1fr)" },
  { key: "excluded", label: "Excl", title: "Uncheck to exclude an export from ISK totals", width: "44px" },
  { key: "isk", label: "ISK/mo", width: "84px", align: "right", numeric: true },
  { key: "storage", label: "Storage", width: "minmax(200px,1.1fr)" },
  { key: "menu", label: "", width: "52px" },
];

type Filter = "all" | "attention";

interface Loaded {
  overview: PlanetaryOverview;
  preferences: PlanetaryPreferences;
}

function useNow(intervalMs = 30_000) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), intervalMs);
    return () => window.clearInterval(id);
  }, [intervalMs]);
  return now;
}

function planetMatches(planet: Planet, query: string) {
  if (!query) return true;
  const haystack = [
    planet.name,
    planet.solarSystemName ?? "",
    ...planet.extractors.map((item) => item.productName),
    ...planet.production.map((item) => item.name),
    ...planet.imports.map((item) => item.name),
    ...planet.exports.map((item) => item.name),
  ]
    .join(" ")
    .toLowerCase();
  return haystack.includes(query);
}

function exportKey(characterId: number, planetId: number, typeId: number) {
  return `${characterId}:${planetId}:${typeId}`;
}

export function PlanetaryPage() {
  const [params, setParams] = useSearchParams();
  const filter: Filter = params.get("filter") === "attention" ? "attention" : "all";
  const query = (params.get("q") ?? "").trim().toLowerCase();
  // On by default: characters without colonies (no PI, or no planets scope)
  // are noise on this page. `?empty=show` lists them, e.g. to reconnect one.
  const hideEmpty = params.get("empty") !== "show";
  const [state, setState] = useState<LoadState<Loaded>>({ status: "loading" });
  const [refreshing, setRefreshing] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const [collapsed, setCollapsed] = useState<Set<number>>(new Set());
  const [dragging, setDragging] = useState<number | null>(null);
  const now = useNow();

  const load = useCallback(async () => {
    try {
      const [overview, preferences] = await Promise.all([getPlanetary(), getPlanetaryPreferences()]);
      setState({ status: "ready", data: { overview, preferences } });
    } catch (error) {
      setState({ status: "error", message: apiMessage(error) });
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  function setParam(key: string, value: string | null) {
    const next = new URLSearchParams(params);
    if (value) next.set(key, value);
    else next.delete(key);
    setParams(next, { replace: true });
  }

  async function savePreferences(next: PlanetaryPreferences, optimistic: (data: Loaded) => Loaded) {
    if (state.status !== "ready") return;
    setActionError(null);
    setState({ status: "ready", data: optimistic({ ...state.data, preferences: next }) });
    try {
      await savePlanetaryPreferences(next);
      await load();
    } catch (error) {
      setActionError(apiMessage(error));
      await load();
    }
  }

  function toggleExport(character: PlanetaryCharacter, planet: Planet, item: PlanetExport) {
    if (state.status !== "ready") return;
    const { preferences } = state.data;
    const key = exportKey(character.eveCharacterId, planet.planetId, item.typeId);
    const others = preferences.excludedExports.filter(
      (entry) => exportKey(entry.characterId, entry.planetId, entry.typeId) !== key,
    );
    const excludedExports = item.excluded
      ? others
      : [...others, { characterId: character.eveCharacterId, planetId: planet.planetId, typeId: item.typeId }];
    void savePreferences({ ...preferences, excludedExports }, (data) => ({
      ...data,
      overview: {
        ...data.overview,
        characters: data.overview.characters.map((entry) =>
          entry.eveCharacterId !== character.eveCharacterId
            ? entry
            : {
                ...entry,
                planets: entry.planets.map((candidate) =>
                  candidate.planetId !== planet.planetId
                    ? candidate
                    : {
                        ...candidate,
                        exports: candidate.exports.map((exported) =>
                          exported.typeId === item.typeId ? { ...exported, excluded: !item.excluded } : exported,
                        ),
                      },
                ),
              },
        ),
      },
    }));
  }

  function setAllExports(character: PlanetaryCharacter, planet: Planet, excluded: boolean) {
    if (state.status !== "ready") return;
    const { preferences } = state.data;
    const others = preferences.excludedExports.filter(
      (entry) => !(entry.characterId === character.eveCharacterId && entry.planetId === planet.planetId),
    );
    const excludedExports = excluded
      ? [
          ...others,
          ...planet.exports.map((item) => ({
            characterId: character.eveCharacterId,
            planetId: planet.planetId,
            typeId: item.typeId,
          })),
        ]
      : others;
    void savePreferences({ ...preferences, excludedExports }, (data) => data);
  }

  /** Moves `characterId` into `targetId`'s slot in the full saved order
   * (hidden characters keep their places relative to each other). */
  function reorder(characterId: number, targetId: number | undefined) {
    if (state.status !== "ready" || targetId === undefined || targetId === characterId) return;
    const order = state.data.overview.characters.map((character) => character.eveCharacterId);
    const from = order.indexOf(characterId);
    const to = order.indexOf(targetId);
    if (from < 0 || to < 0) return;
    order.splice(from, 1);
    order.splice(to, 0, characterId);
    void savePreferences({ ...state.data.preferences, characterOrder: order }, (data) => ({
      ...data,
      overview: {
        ...data.overview,
        characters: order
          .map((id) => data.overview.characters.find((character) => character.eveCharacterId === id))
          .filter((character): character is PlanetaryCharacter => character !== undefined),
      },
    }));
  }

  async function refresh() {
    if (state.status !== "ready") return;
    setRefreshing(true);
    setActionError(null);
    const granted = state.data.overview.characters.filter((character) => character.scopeGranted);
    const results = await Promise.allSettled(granted.map((character) => syncCharacter(character.connectionId)));
    const failed = results.find((result) => result.status === "rejected");
    if (failed && failed.status === "rejected") setActionError(apiMessage(failed.reason));
    await load();
    setRefreshing(false);
  }

  async function grantAccess() {
    try {
      const started = await beginAuthorization();
      if (started.fixtureMode) await load();
      else window.location.assign(started.authorizationUrl);
    } catch (error) {
      setActionError(apiMessage(error));
    }
  }

  const overview = state.status === "ready" ? state.data.overview : null;
  const lastSynced = useMemo(() => {
    const observed = (overview?.characters ?? [])
      .filter((character) => character.scopeGranted)
      .map((character) => character.sync.observedAt)
      .filter((value): value is string => value !== null)
      .sort();
    return observed[0] ?? null;
  }, [overview]);

  const visibleCharacters = useMemo(() => {
    if (!overview) return [];
    return overview.characters
      .map((character) => ({
        character,
        planets: character.planets.filter(
          (planet) => (filter === "all" || planet.attention !== null) && planetMatches(planet, query),
        ),
      }))
      .filter(({ character, planets }) => {
        if (hideEmpty && character.planets.length === 0) return false;
        // Unfiltered, every remaining character shows (so a failed sync stays
        // visible); filtered, only characters with matches.
        return (filter === "all" && !query) || planets.length > 0;
      });
  }, [overview, filter, query, hideEmpty]);

  const emptyCharacterCount = overview?.characters.filter((character) => character.planets.length === 0).length ?? 0;

  const noAccess = overview !== null && overview.characters.every((character) => !character.scopeGranted);

  return (
    <>
      <header className="mb-3 flex flex-wrap items-center justify-between gap-3">
        <div className="flex flex-wrap items-center gap-3">
          <h1 className="iw-title flex items-center gap-2">
            <Globe2 aria-hidden="true" className="h-4 w-4 text-primary" />
            Planetary Interaction
          </h1>
          {overview && !noAccess ? (
            <span className="text-xs text-muted" title={lastSynced ?? undefined}>
              Last synced {formatRelativeAge(lastSynced)}
            </span>
          ) : null}
          {overview && !noAccess ? (
            <button className="iw-button-secondary" disabled={refreshing} onClick={() => void refresh()} type="button">
              <RefreshCw aria-hidden="true" className={`h-3.5 w-3.5 ${refreshing ? "animate-spin" : ""}`} />
              <span className="ml-1.5">{refreshing ? "Refreshing…" : "Refresh"}</span>
            </button>
          ) : null}
        </div>
        {overview && !noAccess ? (
          <div className="flex flex-wrap items-center gap-2">
            <div aria-label="Planet filter" className="flex" role="tablist">
              <button
                aria-selected={filter === "all"}
                className={filter === "all" ? "iw-button-primary" : "iw-button-secondary"}
                onClick={() => setParam("filter", null)}
                role="tab"
                type="button"
              >
                All
              </button>
              <button
                aria-selected={filter === "attention"}
                className={filter === "attention" ? "iw-button-primary" : "iw-button-secondary"}
                onClick={() => setParam("filter", "attention")}
                role="tab"
                type="button"
              >
                <AlertTriangle aria-hidden="true" className="h-3.5 w-3.5" />
                <span className="ml-1.5">Needs attention</span>
              </button>
            </div>
            <label className="flex items-center gap-1.5 text-xs text-muted" title="Characters with no colonies or without the planetary scope">
              <input
                checked={hideEmpty}
                className="h-3.5 w-3.5"
                onChange={(event) => setParam("empty", event.target.checked ? null : "show")}
                type="checkbox"
              />
              Hide characters without colonies
              {hideEmpty && emptyCharacterCount > 0 ? <span>({emptyCharacterCount} hidden)</span> : null}
            </label>
            <label className="relative">
              <Search aria-hidden="true" className="pointer-events-none absolute left-2 top-1/2 h-4 w-4 -translate-y-1/2 text-muted" />
              <span className="sr-only">Search planets</span>
              <input
                className="iw-input w-56 pl-8"
                onChange={(event) => setParam("q", event.target.value || null)}
                placeholder="Planet, system, product…"
                type="search"
                value={params.get("q") ?? ""}
              />
            </label>
          </div>
        ) : null}
      </header>

      {actionError ? <div className="mb-3"><InlineAlert title="Planetary action failed">{actionError}</InlineAlert></div> : null}
      {state.status === "loading" ? <Panel>Loading Planetary Interaction…</Panel> : null}
      {state.status === "error" ? <InlineAlert title="Planetary Interaction unavailable">{state.message}</InlineAlert> : null}

      {overview && noAccess ? (
        <Panel className="py-16">
          <EmptyState
            action={
              <button className="iw-button-primary" onClick={() => void grantAccess()} type="button">
                Grant planetary access
              </button>
            }
            title={overview.characters.length === 0 ? "No characters connected" : "No planetary access"}
          >
            {overview.characters.length === 0
              ? "Connect an EVE character with the Planetary Management scope to start tracking colonies."
              : "None of your characters have granted the Planetary Management ESI scope. Reconnect a character to start tracking its colonies."}
          </EmptyState>
        </Panel>
      ) : null}

      {overview && !noAccess ? (
        <>
          <SummaryStrip
            filter={filter}
            nowMs={now}
            onFilterAttention={() => setParam("filter", "attention")}
            overview={overview}
          />
          {visibleCharacters.length === 0 ? (
            <Panel>
              {overview.summary.planetCount === 0 && hideEmpty ? (
                <EmptyState
                  action={
                    <button className="iw-button-secondary" onClick={() => setParam("empty", "show")} type="button">
                      Show all characters
                    </button>
                  }
                  title="No colonies yet"
                >
                  None of your characters with planetary access have colonies synced.
                </EmptyState>
              ) : (
                <EmptyState title={filter === "attention" && !query ? "Nothing needs attention" : "No matching planets"}>
                  {filter === "attention" && !query
                    ? "Every extractor is running, storage has room and factories are fed."
                    : "No planets match the current search and filter."}
                </EmptyState>
              )}
            </Panel>
          ) : (
            <OperationalTable ariaLabel="Planetary colonies" columns={COLUMNS} onSelectRow={() => undefined} selectedRowKey={null}>
              {visibleCharacters.map(({ character, planets }, index) => {
                const stale = character.scopeGranted && isStale(character.sync.observedAt, now);
                return (
                  <OperationalTableGroup
                    emptyMessage={
                      character.scopeGranted ? (
                        character.sync.lastError ? (
                          <span className="text-destructive">Sync failed: {character.sync.lastError}</span>
                        ) : character.sync.observedAt ? (
                          "No colonies on this character."
                        ) : (
                          "Colonies have not synced yet — use Refresh."
                        )
                      ) : (
                        <span>
                          This character has not granted planetary access.{" "}
                          <button className="text-primary underline" onClick={() => void grantAccess()} type="button">
                            Reconnect with planetary access
                          </button>
                        </span>
                      )
                    }
                    expanded={!collapsed.has(character.eveCharacterId)}
                    groupKey={String(character.eveCharacterId)}
                    headerProps={{
                      draggable: true,
                      onDragStart: (event: DragEvent<HTMLTableRowElement>) => {
                        event.dataTransfer.effectAllowed = "move";
                        setDragging(character.eveCharacterId);
                      },
                      onDragOver: (event: DragEvent<HTMLTableRowElement>) => {
                        if (dragging !== null) event.preventDefault();
                      },
                      onDrop: (event: DragEvent<HTMLTableRowElement>) => {
                        event.preventDefault();
                        if (dragging !== null) reorder(dragging, character.eveCharacterId);
                        setDragging(null);
                      },
                      onDragEnd: () => setDragging(null),
                    }}
                    itemCount={planets.length}
                    key={character.eveCharacterId}
                    label={character.name}
                    labelCase="normal"
                    labelPrivate
                    leading={
                      <EveCharacterPortrait
                        characterId={character.eveCharacterId}
                        characterName={character.name}
                        className="h-6 w-6 rounded-full"
                        size={32}
                      />
                    }
                    onExpandedChange={(expanded) =>
                      setCollapsed((current) => {
                        const next = new Set(current);
                        if (expanded) next.delete(character.eveCharacterId);
                        else next.add(character.eveCharacterId);
                        return next;
                      })
                    }
                    showCount={false}
                    status={character.alertCount > 0 ? "warning" : "neutral"}
                    summary={<GroupSummary character={character} nowMs={now} stale={stale} />}
                    trailing={
                      <DropdownMenu
                        align="end"
                        icon={<Settings aria-hidden="true" className="h-3.5 w-3.5" />}
                        items={[
                          {
                            key: "up",
                            label: "Move up",
                            icon: <ArrowUp aria-hidden="true" className="h-4 w-4" />,
                            disabled: index === 0,
                            onSelect: () =>
                              reorder(character.eveCharacterId, visibleCharacters[index - 1]?.character.eveCharacterId),
                          },
                          {
                            key: "down",
                            label: "Move down",
                            icon: <ArrowDown aria-hidden="true" className="h-4 w-4" />,
                            disabled: index === visibleCharacters.length - 1,
                            onSelect: () =>
                              reorder(character.eveCharacterId, visibleCharacters[index + 1]?.character.eveCharacterId),
                          },
                        ]}
                        label={`${character.name} options`}
                        portal
                        variant="icon"
                      />
                    }
                  >
                    {planets.map((planet) => (
                      <OperationalTableRow
                        accent={planet.attention === "red" ? "blocking" : planet.attention === "amber" ? "warning" : undefined}
                        align="top"
                        cells={{
                          planet: <PlanetNameCell planet={planet} stale={stale} />,
                          cc: <CcBadge level={planet.upgradeLevel} />,
                          extraction: <ExtractionCell nowMs={now} planet={planet} />,
                          production: <ProductionCell planet={planet} />,
                          imports: <ImportsCell planet={planet} />,
                          exports: <ExportsCell planet={planet} />,
                          excluded: <ExcludedCell onToggle={(item) => toggleExport(character, planet, item)} planet={planet} />,
                          isk: <IskCell planet={planet} />,
                          storage: <StorageCell planet={planet} />,
                          menu:
                            planet.exports.length > 0 ? (
                              <DropdownMenu
                                align="end"
                                icon={<MoreHorizontal aria-hidden="true" className="h-3.5 w-3.5" />}
                                items={[
                                  {
                                    key: "exclude-all",
                                    label: "Exclude all exports",
                                    onSelect: () => setAllExports(character, planet, true),
                                  },
                                  {
                                    key: "include-all",
                                    label: "Include all exports",
                                    onSelect: () => setAllExports(character, planet, false),
                                  },
                                ]}
                                label={`${planet.name} options`}
                                portal
                                variant="icon"
                              />
                            ) : null,
                        }}
                        interactive={false}
                        key={planet.planetId}
                        rowKey={`${character.eveCharacterId}:${planet.planetId}`}
                      />
                    ))}
                  </OperationalTableGroup>
                );
              })}
            </OperationalTable>
          )}
          {overview.priceObservedAt === null && overview.summary.planetCount > 0 ? (
            <p className="mt-2 text-xs text-muted">
              No market data for PI commodities in the workspace's default market yet — ISK values will appear once prices refresh.
            </p>
          ) : null}
        </>
      ) : null}
    </>
  );
}

function GroupSummary({ character, nowMs, stale }: { character: PlanetaryCharacter; nowMs: number; stale: boolean }) {
  const next = character.nextExpiryAt ? countdown(character.nextExpiryAt, nowMs) : null;
  return (
    <span className="flex flex-wrap items-center gap-2.5 text-xs">
      {character.piSkillLevel !== null ? (
        <span title="Interplanetary Consolidation">
          <span className="font-bold text-primary">PI</span> Lv {character.piSkillLevel}
        </span>
      ) : null}
      <span>
        {character.planets.length} {character.planets.length === 1 ? "planet" : "planets"}
      </span>
      <span className="font-mono font-semibold tabular-nums text-positive" title={`${formatIskSummary(character.iskPerMonth)} / month`}>
        {formatIskCompact(character.iskPerMonth)} /mo
      </span>
      {next ? (
        <span className={`flex items-center gap-1 font-mono tabular-nums ${toneText[next.tone]}`} title={next.absolute}>
          <Clock aria-hidden="true" className="h-3 w-3" />
          next in {next.label}
        </span>
      ) : null}
      {character.alertCount > 0 ? (
        <span className="rounded-sm border border-warning/40 bg-warning/10 px-1.5 font-bold text-warning">
          {character.alertCount} {character.alertCount === 1 ? "alert" : "alerts"}
        </span>
      ) : null}
      {stale ? (
        <span className="flex items-center gap-1 rounded-sm border border-warning/40 bg-warning/10 px-1.5 text-warning" title={character.sync.observedAt ?? undefined}>
          <WifiOff aria-hidden="true" className="h-3 w-3" />
          Stale {formatRelativeAge(character.sync.observedAt)}
        </span>
      ) : null}
      {!character.scopeGranted ? <span className="text-warning">No planetary access</span> : null}
    </span>
  );
}

function SummaryStrip({
  filter,
  nowMs,
  onFilterAttention,
  overview,
}: {
  filter: Filter;
  nowMs: number;
  onFilterAttention: () => void;
  overview: PlanetaryOverview;
}) {
  const { summary } = overview;
  const next = summary.nextAction;
  const nextHours = next ? (new Date(next.at).getTime() - nowMs) / 3_600_000 : null;
  const alerts = [
    { key: "expired", count: summary.alerts.expired, label: "expired" },
    { key: "storage", count: summary.alerts.storageFull, label: "storage ≥90%" },
    { key: "starved", count: summary.alerts.starved, label: "starved" },
  ].filter((alert) => alert.count > 0);
  return (
    <section aria-label="Planetary summary" className="mb-3 flex flex-wrap gap-2">
      <SummaryTile label="ISK / month" sub="all non-excluded exports" tone="text-positive" title={`${formatIskSummary(summary.iskPerMonth)} / month`}>
        {formatIskCompact(summary.iskPerMonth)}
      </SummaryTile>
      <SummaryTile label="Colonies" sub={`${summary.characterCount} ${summary.characterCount === 1 ? "character" : "characters"}`}>
        {summary.planetCount} {summary.planetCount === 1 ? "planet" : "planets"}
      </SummaryTile>
      <SummaryTile
        label="Next action"
        sub={next && nextHours !== null ? `extractor expires in ${formatHours(nextHours)}` : "no running extractors"}
        tone={nextHours !== null && nextHours < 24 ? "text-warning" : undefined}
      >
        {next ? `${next.characterName} · ${next.planetName}` : "—"}
      </SummaryTile>
      <div className="iw-panel flex min-w-56 flex-col gap-1.5 px-3 py-2">
        <span className="text-[0.65rem] font-bold uppercase tracking-widest text-muted">Alerts</span>
        {alerts.length === 0 ? (
          <span className="text-xs text-positive">No alerts</span>
        ) : (
          <div className="flex flex-wrap gap-1.5">
            {alerts.map((alert) => (
              <button
                aria-pressed={filter === "attention"}
                className="flex items-center gap-1 rounded-sm border border-destructive/50 bg-destructive/10 px-2 py-0.5 text-xs font-semibold text-destructive"
                key={alert.key}
                onClick={onFilterAttention}
                type="button"
              >
                <span className="font-mono tabular-nums">{alert.count}</span>
                <span className="font-normal">{alert.label}</span>
              </button>
            ))}
          </div>
        )}
      </div>
    </section>
  );
}

function SummaryTile({
  children,
  label,
  sub,
  title,
  tone,
}: {
  children: ReactNode;
  label: string;
  sub: string;
  title?: string;
  tone?: string;
}) {
  return (
    <div className="iw-panel flex min-w-40 flex-col gap-1 px-3 py-2" title={title}>
      <span className="text-[0.65rem] font-bold uppercase tracking-widest text-muted">{label}</span>
      <span className={`font-mono text-base font-semibold leading-none tabular-nums ${tone ?? "text-foreground"}`}>{children}</span>
      <span className="text-[0.65rem] text-muted">{sub}</span>
    </div>
  );
}
