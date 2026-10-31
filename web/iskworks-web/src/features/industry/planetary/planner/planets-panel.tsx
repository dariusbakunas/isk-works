import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { MoreVertical, Search, Users } from "lucide-react";

import {
  saveDensities,
  savePlanetSettings,
  type DensityWrite,
  type PlannerPlanet,
  type PlannerPlanets,
  type PlannerSystem,
} from "../../../../api/pi-planner";
import { DropdownMenu } from "../../../../components/dropdown-menu";
import {
  OperationalTable,
  OperationalTableGroup,
  OperationalTableRow,
  type OperationalColumn,
} from "../../../../components/operational-table";
import { InlineAlert } from "../../../../components/primitives";
import { apiMessage } from "../../inventory/shared";
import { PlanetTypeChip } from "../planetary-cells";
import { securityTone } from "../planetary-format";
import { DensityCell } from "./density-cell";
import { CustomsTaxDialog, HeadValueDialog } from "./planet-dialogs";
import { isScanned, planetTypeKey, ratePercent } from "./planner-format";

/** Readings typed into one row within this window go up in one request. */
export const SAVE_DEBOUNCE_MS = 500;
const SAVED_FLASH_MS = 1_200;

export type PlanetFilter = "all" | "unscanned" | "scanned";

const COLUMNS: OperationalColumn[] = [
  { key: "planet", label: "Planet", width: "8.5rem" },
  { key: "resources", label: "Resources · density %", width: "minmax(25rem,1fr)" },
  { key: "tax", label: "Tax", width: "3rem", align: "right", numeric: true },
  { key: "menu", label: "", width: "2.25rem" },
];

const securityClass = { high: "text-positive", low: "text-warning", null: "text-destructive" } as const;

export function SecurityValue({ value }: { value: string | null }) {
  if (value === null) return null;
  const security = Number(value);
  return (
    <span className={`font-mono text-xs tabular-nums ${securityClass[securityTone(security)]}`}>
      {security.toFixed(2)}
    </span>
  );
}

function planetScanned(planet: PlannerPlanet): boolean {
  return planet.resources.some(isScanned);
}

function matches(planet: PlannerPlanet, filter: PlanetFilter, query: string): boolean {
  if (filter === "scanned" && !planetScanned(planet)) return false;
  if (filter === "unscanned" && planetScanned(planet)) return false;
  const needle = query.trim().toLowerCase();
  if (!needle) return true;
  return (
    planet.name.toLowerCase().includes(needle) ||
    planet.typeName.toLowerCase().includes(needle) ||
    planet.resources.some((resource) => resource.name.toLowerCase().includes(needle))
  );
}

function readingKey(planetId: number, typeId: number): string {
  return `${planetId}:${typeId}`;
}

/**
 * The planets in range with a density cell per raw resource. Typing saves:
 * each row's changes are debounced and sent together.
 */
export function PlanetsPanel({
  data,
  defaultCustomsRate,
  filter,
  nowMs,
  onFilterChange,
  onSaved,
}: {
  data: PlannerPlanets;
  defaultCustomsRate: string;
  filter: PlanetFilter;
  nowMs: number;
  onFilterChange: (filter: PlanetFilter) => void;
  /** After any reading or setting is stored, so the ranking can refresh. */
  onSaved: () => void;
}) {
  const [planets, setPlanets] = useState(data.planets);
  useEffect(() => setPlanets(data.planets), [data.planets]);
  const [query, setQuery] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState<Set<string>>(new Set());
  const [headValuePlanet, setHeadValuePlanet] = useState<PlannerPlanet | null>(null);
  const [taxPlanet, setTaxPlanet] = useState<PlannerPlanet | null>(null);

  const pending = useRef(new Map<number, Map<number, DensityWrite>>());
  const timers = useRef(new Map<number, number>());
  useEffect(() => {
    const active = timers.current;
    return () => active.forEach((timer) => window.clearTimeout(timer));
  }, []);

  const applyLocally = useCallback((writes: DensityWrite[]) => {
    const at = new Date().toISOString();
    setPlanets((current) =>
      current.map((planet) => {
        const mine = writes.filter((write) => write.planetId === planet.planetId);
        if (mine.length === 0) return planet;
        return {
          ...planet,
          resources: planet.resources.map((resource) => {
            const write = mine.find((entry) => entry.resourceTypeId === resource.typeId);
            if (!write) return resource;
            const cleared = write.densityPct === null && write.headValue === null;
            return {
              ...resource,
              densityPct: write.densityPct,
              headValue: write.headValue,
              updatedAt: cleared ? null : at,
            };
          }),
        };
      }),
    );
  }, []);

  const send = useCallback(
    async (writes: DensityWrite[]) => {
      try {
        await saveDensities(writes);
        setError(null);
        const keys = writes.map((write) => readingKey(write.planetId, write.resourceTypeId));
        setSaved((current) => new Set([...current, ...keys]));
        window.setTimeout(
          () => setSaved((current) => new Set([...current].filter((key) => !keys.includes(key)))),
          SAVED_FLASH_MS,
        );
        onSaved();
      } catch (requestError) {
        setError(apiMessage(requestError));
      }
    },
    [onSaved],
  );

  const flush = useCallback(
    (planetId: number) => {
      timers.current.delete(planetId);
      const row = pending.current.get(planetId);
      pending.current.delete(planetId);
      if (row && row.size > 0) void send([...row.values()]);
    },
    [send],
  );

  const queue = useCallback(
    (write: DensityWrite) => {
      applyLocally([write]);
      const row = pending.current.get(write.planetId) ?? new Map<number, DensityWrite>();
      row.set(write.resourceTypeId, write);
      pending.current.set(write.planetId, row);
      window.clearTimeout(timers.current.get(write.planetId));
      timers.current.set(
        write.planetId,
        window.setTimeout(() => flush(write.planetId), SAVE_DEBOUNCE_MS),
      );
    },
    [applyLocally, flush],
  );

  const saveNow = useCallback(
    (writes: DensityWrite[]) => {
      applyLocally(writes);
      void send(writes);
    },
    [applyLocally, send],
  );

  const saveSettings = useCallback(
    async (planet: PlannerPlanet, customsRate: string | null, contested: boolean) => {
      setPlanets((current) =>
        current.map((entry) => (entry.planetId === planet.planetId ? { ...entry, customsRate, contested } : entry)),
      );
      try {
        await savePlanetSettings([{ planetId: planet.planetId, customsRate, contested }]);
        setError(null);
        onSaved();
      } catch (requestError) {
        setError(apiMessage(requestError));
      }
    },
    [onSaved],
  );

  const scannedCount = planets.filter(planetScanned).length;
  const visible = useMemo(() => planets.filter((planet) => matches(planet, filter, query)), [filter, planets, query]);
  const bySystem = useMemo(() => {
    const groups = new Map<number, PlannerPlanet[]>();
    for (const planet of visible) {
      groups.set(planet.solarSystemId, [...(groups.get(planet.solarSystemId) ?? []), planet]);
    }
    return groups;
  }, [visible]);

  // Sequential positions across every visible cell, for Enter-to-next.
  let cellIndex = 0;
  const groupRows = (system: PlannerSystem, rows: PlannerPlanet[]) =>
    rows.map((planet) => {
      const typeKey = planetTypeKey(planet.typeName);
      const cells = planet.resources.map((resource) => {
        const index = cellIndex++;
        return (
          <DensityCell
            index={index}
            key={resource.typeId}
            nowMs={nowMs}
            onCommit={(densityPct) =>
              queue({ planetId: planet.planetId, resourceTypeId: resource.typeId, densityPct, headValue: resource.headValue })
            }
            onEditHeadValue={() => setHeadValuePlanet(planet)}
            resource={resource}
            saved={saved.has(readingKey(planet.planetId, resource.typeId))}
          />
        );
      });
      return (
        <OperationalTableRow
          cells={{
            planet: (
              <div className="flex min-w-0 items-center gap-1.5">
                <PlanetTypeChip type={typeKey} />
                <div className="min-w-0">
                  <div className="flex items-center gap-1 truncate text-xs font-semibold text-foreground">
                    {planet.name}
                    {planet.contested ? (
                      <Users
                        aria-label="Contested: other players extract here"
                        className="h-3 w-3 shrink-0 text-warning"
                      />
                    ) : null}
                  </div>
                  <div className="text-[0.65rem] capitalize text-muted">{typeKey}</div>
                </div>
              </div>
            ),
            resources: (
              <div className="grid min-w-0 grid-cols-[repeat(5,minmax(0,1fr))] divide-x divide-border">{cells}</div>
            ),
            tax: (
              <span
                className={`font-mono text-xs ${planet.customsRate === null ? "text-faint" : "text-foreground"}`}
                title={planet.customsRate === null ? "Scenario default" : "This planet's customs office"}
              >
                {planet.customsRate === null ? "—" : `${ratePercent(planet.customsRate)} %`}
              </span>
            ),
            menu: (
              <DropdownMenu
                align="end"
                icon={<MoreVertical aria-hidden="true" className="h-3.5 w-3.5" />}
                items={[
                  { key: "head", label: "Head values…", onSelect: () => setHeadValuePlanet(planet) },
                  { key: "tax", label: "Set customs tax…", onSelect: () => setTaxPlanet(planet) },
                  {
                    key: "contested",
                    label: planet.contested ? "Not contested" : "Contested (other players extract here)",
                    onSelect: () => void saveSettings(planet, planet.customsRate, !planet.contested),
                  },
                  {
                    key: "clear",
                    label: "Clear densities",
                    disabled: !planetScanned(planet),
                    onSelect: () =>
                      saveNow(
                        planet.resources
                          .filter(isScanned)
                          .map((resource) => ({
                            planetId: planet.planetId,
                            resourceTypeId: resource.typeId,
                            densityPct: null,
                            headValue: null,
                          })),
                      ),
                  },
                ]}
                label={`${planet.name} actions`}
                portal
                variant="icon"
              />
            ),
          }}
          interactive={false}
          key={planet.planetId}
          rowKey={String(planet.planetId)}
        />
      );
    });

  return (
    <section className="iw-panel min-w-0" aria-label="Planets">
      <div className="flex flex-wrap items-center gap-2 border-b border-border p-2">
        <label className="relative">
          <Search aria-hidden="true" className="pointer-events-none absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted" />
          <span className="sr-only">Search planets</span>
          <input
            className="iw-input min-h-8 w-48 pl-7 text-xs"
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Planet or resource"
            type="search"
            value={query}
          />
        </label>
        <div aria-label="Planet filter" className="flex" role="tablist">
          {(
            [
              ["all", "All"],
              ["unscanned", "Not scanned"],
              ["scanned", "Scanned"],
            ] as const
          ).map(([value, label]) => (
            <button
              aria-selected={filter === value}
              className={filter === value ? "iw-button-primary" : "iw-button-secondary"}
              key={value}
              onClick={() => onFilterChange(value)}
              role="tab"
              type="button"
            >
              {label}
            </button>
          ))}
        </div>
        <span className="ml-auto text-xs text-muted">
          {scannedCount} of {planets.length} planets scanned
        </span>
      </div>
      {error ? (
        <div className="p-2">
          <InlineAlert title="Not saved">{error}</InlineAlert>
        </div>
      ) : null}
      <OperationalTable ariaLabel="Planets in range" columns={COLUMNS} onSelectRow={() => {}} selectedRowKey={null}>
        {data.systems
          .filter((system) => bySystem.has(system.solarSystemId))
          .map((system) => {
            const rows = bySystem.get(system.solarSystemId) ?? [];
            return (
              <OperationalTableGroup
                groupKey={String(system.solarSystemId)}
                itemCount={rows.length}
                key={system.solarSystemId}
                label={system.name}
                labelCase="normal"
                showCount={false}
                summary={
                  <span className="flex items-center gap-2 text-xs">
                    <SecurityValue value={system.securityStatus} />
                    <span>
                      {system.jumps} {system.jumps === 1 ? "jump" : "jumps"} · {rows.length}{" "}
                      {rows.length === 1 ? "planet" : "planets"}
                    </span>
                    <span className="text-faint">tax {ratePercent(defaultCustomsRate)} % default</span>
                  </span>
                }
              >
                {groupRows(system, rows)}
              </OperationalTableGroup>
            );
          })}
      </OperationalTable>
      {visible.length === 0 ? <p className="p-3 text-xs text-muted">No planets match.</p> : null}
      <p className="border-t border-border px-2 py-1.5 text-[0.65rem] text-muted">
        Digits replace the value · Tab / Enter next resource · after the fifth, the next planet · Esc reverts · Delete
        clears
      </p>
      {headValuePlanet ? (
        <HeadValueDialog
          onCancel={() => setHeadValuePlanet(null)}
          onSave={(headValues) => {
            const planet = headValuePlanet;
            setHeadValuePlanet(null);
            saveNow(
              planet.resources
                .filter((resource) => headValues.get(resource.typeId) !== resource.headValue)
                .map((resource) => ({
                  planetId: planet.planetId,
                  resourceTypeId: resource.typeId,
                  densityPct: resource.densityPct,
                  headValue: headValues.get(resource.typeId) ?? null,
                })),
            );
          }}
          planet={planets.find((planet) => planet.planetId === headValuePlanet.planetId) ?? headValuePlanet}
        />
      ) : null}
      {taxPlanet ? (
        <CustomsTaxDialog
          defaultRate={defaultCustomsRate}
          onCancel={() => setTaxPlanet(null)}
          onSave={(customsRate) => {
            const planet = taxPlanet;
            setTaxPlanet(null);
            void saveSettings(planet, customsRate, planet.contested);
          }}
          planet={taxPlanet}
        />
      ) : null}
    </section>
  );
}
