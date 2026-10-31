import { useState } from "react";
import { AlertTriangle, Info, RefreshCw } from "lucide-react";

import type { PlanColony, PlannerSolution } from "../../../../api/pi-planner";
import { formatIskCompact, formatIskSummary } from "../../../../components/money";
import {
  OperationalTable,
  OperationalTableGroup,
  OperationalTableRow,
  type OperationalColumn,
} from "../../../../components/operational-table";
import { formatQuantity } from "../planetary-format";
import { FlowDiagram } from "./flow-diagram";

const GRID_RESET = [1, 3, 7];
const GRID_HAUL = [1, 3, 7, 14];

const COLUMNS: OperationalColumn[] = [
  { key: "planet", label: "Planet", width: "minmax(7rem,1fr)" },
  { key: "role", label: "Role", width: "6.5rem" },
  { key: "setup", label: "Setup", width: "minmax(8rem,1.1fr)" },
  { key: "in", label: "In", width: "minmax(8rem,1.1fr)" },
  { key: "out", label: "Out", width: "minmax(8rem,1.1fr)" },
  { key: "tax", label: "Tax/mo", width: "4.5rem", align: "right", numeric: true },
  { key: "info", label: "", width: "2rem" },
];

function Isk({ value }: { value: string }) {
  return <span title={`${formatIskSummary(value)} ISK`}>{formatIskCompact(value)}</span>;
}

function percent(share: string): string {
  return `${Math.round(Number(share) * 100)} %`;
}

function rateLines(rates: { name: string; unitsPerHour: string }[]) {
  return rates.map((rate) => (
    <span className="block truncate leading-5" key={rate.name}>
      {rate.name} {formatQuantity(Number(rate.unitsPerHour))}/h
    </span>
  ));
}

function setup(colony: PlanColony): string {
  const parts = [];
  if (colony.setup.ecus > 0) parts.push(`${colony.setup.ecus} ECU × ${Math.round(colony.setup.heads / colony.setup.ecus)} heads`);
  for (const processor of colony.setup.processors) parts.push(`${processor.count} ${processor.name}`);
  return parts.join(" · ");
}

function Tile({ label, title, value }: { label: string; title?: string; value: string }) {
  return (
    <div className="flex min-h-12 flex-col justify-center border-r border-border px-3 last:border-r-0" title={title}>
      <span className="text-[0.65rem] uppercase text-muted">{label}</span>
      <b className="font-mono text-sm text-foreground">{value}</b>
    </div>
  );
}

/** The optimised plan: what each character sets up, how goods move, and what-ifs. */
export function PlanPanel({
  characterNames,
  haulDays,
  onCadence,
  onScanFirst,
  resetDays,
  solution,
  solving,
}: {
  characterNames: string[];
  haulDays: number;
  onCadence: (resetDays: number, haulDays: number) => void;
  onScanFirst: (planetId: number, resourceTypeId: number) => void;
  resetDays: number;
  solution: PlannerSolution;
  solving: boolean;
}) {
  const [hovered, setHovered] = useState<number | null>(null);
  const { plan, whatIfs } = solution;
  const logins = plan.effort.loginsPerWeekByCharacter.map(Number);
  const averageLogins = logins.length ? logins.reduce((sum, value) => sum + value, 0) / logins.length : 0;
  const hotspots = new Set(plan.sharedHotspots.map((hotspot) => `${hotspot.planetId}:${hotspot.resourceTypeId}`));
  const characters = [...new Set(plan.colonies.map((colony) => colony.character))].sort((a, b) => a - b);
  const best = Math.max(1, ...whatIfs.perCharacter.map(Number));

  if (plan.colonies.length === 0) {
    return (
      <div className="p-6 text-center text-sm text-muted">
        No colony pays with these prices and planets. Try a wider radius or allow buying intermediates.
      </div>
    );
  }

  return (
    <div aria-busy={solving || undefined} className="relative">
      {solving ? (
        <div className="absolute right-3 top-3 z-20 flex items-center gap-2 rounded-md border border-primary/50 bg-panel px-3 py-2 text-xs shadow-xl" role="status">
          <RefreshCw aria-hidden="true" className="h-3.5 w-3.5 animate-spin text-primary" />
          Optimising…
        </div>
      ) : null}
      <div className={solving ? "pointer-events-none opacity-50" : undefined}>
        <div className="grid grid-cols-2 border-b border-border sm:grid-cols-4">
          <Tile label="ISK/mo" value={formatIskCompact(plan.iskPerMonth)} title={`${formatIskSummary(plan.iskPerMonth)} ISK`} />
          <Tile label="Colonies" value={`${plan.colonies.length} of ${solution.availableColonies}`} />
          <Tile label="Customs tax/mo" value={`−${formatIskCompact(plan.customsTaxPerMonth)}`} />
          <Tile label="Sales/mo" value={formatIskCompact(plan.salesPerMonth)} />
        </div>
        <p className="px-3 pt-2 text-[0.65rem] uppercase text-muted">
          Effort · reset every {resetDays}d · haul every {haulDays}d
        </p>
        <div className="grid grid-cols-2 border-b border-border sm:grid-cols-4">
          <Tile
            label="Logins/week"
            title={`Per character: ${logins.map((value) => value.toFixed(1)).join(", ")}. A reset and a haul on the same day count once.`}
            value={`${averageLogins.toFixed(1)} per character`}
          />
          <Tile label="Haul runs/week" value={Number(plan.effort.haulRunsPerWeek).toFixed(1)} />
          <Tile label="Stops per run" title="Customs offices visited each run" value={String(plan.effort.stopsPerRun)} />
          <Tile label="Per run" title="Size your hauler for this" value={`${Math.round(Number(plan.effort.m3PerRun)).toLocaleString("en-US")} m³`} />
        </div>
        <div className="grid items-start gap-3 p-3 2xl:grid-cols-[minmax(0,1fr)_17rem]">
          <div className="min-w-0 space-y-3">
            <OperationalTable ariaLabel="Colonies" columns={COLUMNS} onSelectRow={() => {}} selectedRowKey={null}>
              {characters.map((character) => {
                const mine = plan.colonies.filter((colony) => colony.character === character);
                return (
                  <OperationalTableGroup
                    groupKey={`character-${character}`}
                    itemCount={mine.length}
                    key={character}
                    label={characterNames[character] ?? `Character ${character + 1}`}
                    labelCase="normal"
                    labelPrivate
                    summary={
                      <span className="font-mono text-xs">
                        {mine.length} {mine.length === 1 ? "colony" : "colonies"}
                        {logins[character] !== undefined ? ` · ${logins[character].toFixed(1)} logins/wk` : null}
                      </span>
                    }
                  >
                    {mine.map((colony) => {
                      const shared = colony.extracts.some((extract) => hotspots.has(`${colony.planetId}:${extract.typeId}`));
                      return (
                        <OperationalTableRow
                          cells={{
                            planet: (
                              <span
                                className={`block rounded-sm px-1 ${hovered === colony.index ? "ring-1 ring-primary" : ""}`}
                                onMouseEnter={() => setHovered(colony.index)}
                                onMouseLeave={() => setHovered(null)}
                              >
                                <b className="block truncate text-xs">{colony.planetName}</b>
                                <small className="text-muted">{colony.jumps} j</small>
                              </span>
                            ),
                            role: (
                              <span
                                className={`rounded-sm border px-1.5 py-0.5 text-[0.65rem] ${
                                  colony.role === "factory"
                                    ? "border-reaction/50 bg-reaction/10 text-reaction"
                                    : "border-positive/50 bg-positive/10 text-positive"
                                }`}
                              >
                                {colony.role === "factory" ? "Factory" : "Extract + refine"}
                              </span>
                            ),
                            setup: (
                              <span className="block text-xs text-muted">
                                {setup(colony)}
                                {colony.setup.extraStorage > 0 ? (
                                  <span className="mt-0.5 block w-fit rounded-sm border border-primary/40 bg-primary/10 px-1 text-[0.6rem] text-primary">
                                    +{colony.setup.extraStorage} storage
                                  </span>
                                ) : null}
                              </span>
                            ),
                            in: (
                              <span className="block text-xs text-muted">
                                {rateLines(colony.role === "extraction" ? colony.extracts : colony.imports)}
                              </span>
                            ),
                            out: <span className="block text-xs">{rateLines(colony.exports)}</span>,
                            tax: (
                              <span className="font-mono text-xs text-warning">
                                −<Isk value={colony.taxPerMonth} />
                              </span>
                            ),
                            info: shared ? (
                              <AlertTriangle
                                aria-label="Shared hotspot: another character extracts this resource here"
                                className="h-3.5 w-3.5 text-warning"
                              />
                            ) : (
                              <span
                                title={`Planet ${Number(colony.onPlanetM3).toLocaleString("en-US")} m³ between logins · skyhook ${Number(
                                  colony.skyhookM3,
                                ).toLocaleString("en-US")} / 35,000 m³ · CPU ${percent(colony.cpuUsed)} · power ${percent(colony.powerUsed)}`}
                              >
                                <Info
                                  aria-label={`CPU ${percent(colony.cpuUsed)}, power ${percent(colony.powerUsed)}`}
                                  className="h-3.5 w-3.5 text-muted"
                                />
                              </span>
                            ),
                          }}
                          interactive={false}
                          key={colony.index}
                          rowKey={String(colony.index)}
                        />
                      );
                    })}
                  </OperationalTableGroup>
                );
              })}
            </OperationalTable>
            <div>
              <h3 className="iw-eyebrow mb-1 text-muted">Product flow</h3>
              <FlowDiagram characterNames={characterNames} hovered={hovered} onHover={setHovered} plan={plan} />
            </div>
          </div>
          <aside aria-label="What-ifs" className="space-y-4 text-xs">
            <section>
              <h3 className="iw-eyebrow mb-1 text-muted">Profit vs effort</h3>
              <table className="w-full border-collapse" aria-label="Profit vs effort">
                <thead>
                  <tr>
                    <th className="p-1 text-left text-[0.6rem] font-normal text-muted">Reset / haul</th>
                    {GRID_HAUL.map((haul) => (
                      <th className="p-1 text-[0.6rem] font-normal text-muted" key={haul} scope="col">
                        {haul}d
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {GRID_RESET.map((reset) => (
                    <tr key={reset}>
                      <th className="p-1 text-left text-[0.6rem] font-normal text-muted" scope="row">
                        {reset}d
                      </th>
                      {GRID_HAUL.map((haul) => {
                        const cell = whatIfs.grid.find((entry) => entry.resetDays === reset && entry.haulDays === haul);
                        const current = reset === resetDays && haul === haulDays;
                        return (
                          <td className="p-0.5" key={haul}>
                            <button
                              aria-label={`Reset every ${reset}d, haul every ${haul}d`}
                              aria-pressed={current}
                              className={`w-full rounded-sm border px-1 py-1 text-left ${
                                current ? "border-primary bg-primary/15" : "border-border hover:border-primary/60"
                              }`}
                              onClick={() => onCadence(reset, haul)}
                              title={cell ? `${formatIskSummary(cell.iskPerMonth)} ISK/mo · ${Number(cell.loginsPerWeek).toFixed(1)} logins per week` : undefined}
                              type="button"
                            >
                              <b className="block font-mono text-[0.7rem]">{cell ? formatIskCompact(cell.iskPerMonth) : "—"}</b>
                              <span className="block text-[0.6rem] text-muted">
                                {cell ? `${Number(cell.loginsPerWeek).toFixed(1)} logins` : ""}
                              </span>
                            </button>
                          </td>
                        );
                      })}
                    </tr>
                  ))}
                </tbody>
              </table>
            </section>
            <section>
              <h3 className="iw-eyebrow mb-1 text-muted">Per character</h3>
              <ol className="space-y-1">
                {whatIfs.perCharacter.map((isk, index) => (
                  <li className="grid grid-cols-[4.5rem_1fr_3.5rem] items-center gap-2" key={index}>
                    <span className="text-muted">
                      {index + 1} {index === 0 ? "character" : "chars"}
                    </span>
                    <span className="h-1.5 bg-border">
                      <i className="block h-full bg-primary" style={{ width: `${(Number(isk) / best) * 100}%` }} />
                    </span>
                    <span className="text-right font-mono">
                      <Isk value={isk} />
                    </span>
                  </li>
                ))}
              </ol>
            </section>
            <section>
              <h3 className="iw-eyebrow mb-1 text-muted">Radius</h3>
              <div className="flex flex-wrap gap-1">
                {whatIfs.perRadius.map((entry) => (
                  <span className="rounded-sm border border-border px-1.5 py-0.5 font-mono" key={entry.jumps}>
                    {entry.jumps}j <Isk value={entry.iskPerMonth} />
                  </span>
                ))}
              </div>
            </section>
            {whatIfs.scanFirst.length > 0 ? (
              <section>
                <h3 className="iw-eyebrow mb-1 text-muted">Scan these first</h3>
                <ul className="space-y-1">
                  {whatIfs.scanFirst.map((entry) => (
                    <li key={`${entry.planetId}:${entry.resourceTypeId}`}>
                      <button
                        className="flex w-full items-center gap-2 rounded-sm px-1 py-0.5 text-left hover:bg-primary/10"
                        onClick={() => onScanFirst(entry.planetId, entry.resourceTypeId)}
                        title={`10 points lower costs ${formatIskSummary(entry.iskLostPerMonth)} ISK/mo`}
                        type="button"
                      >
                        <span className="min-w-0 flex-1 truncate">
                          {entry.planetName} · {entry.resourceName}
                          {entry.source === "default" ? <span className="ml-1 text-warning">not scanned</span> : null}
                        </span>
                        <span className="font-mono text-muted">−{formatIskCompact(entry.iskLostPerMonth)}</span>
                      </button>
                    </li>
                  ))}
                </ul>
              </section>
            ) : null}
          </aside>
        </div>
      </div>
    </div>
  );
}
