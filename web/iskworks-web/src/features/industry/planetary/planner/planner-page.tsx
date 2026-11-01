import { useCallback, useEffect, useMemo, useState } from "react";
import { useSearchParams } from "react-router";
import { AlertTriangle, Globe2, RefreshCw } from "lucide-react";

import {
  getPlannerPlanets,
  rankProducts,
  SDE_MISSING_PLANNER_DATA,
  SOLVE_TIMEOUT,
  solvePlan,
  type PlannerScenario,
} from "../../../../api/pi-planner";
import { ApiError } from "../../../../api/workspace";
import { InlineAlert } from "../../../../components/primitives";
import { useLoad } from "../../../../hooks/use-load";
import { apiMessage } from "../../inventory/shared";
import { PlanetsPanel, type PlanetFilter } from "./planets-panel";
import {
  loadCharacters,
  settingsFromQuery,
  settingsToQuery,
  storeCharacters,
  type ScenarioCharacter,
  type ScenarioSettings,
} from "./planner-format";
import { PlanetarySectionTabs } from "../planetary-section-tabs";
import { PlanPanel } from "./plan-panel";
import { RankingPanel } from "./ranking-panel";
import { ScenarioBar } from "./scenario-bar";

/** Re-rank once the scenario has been still for this long. */
const RANK_DEBOUNCE_MS = 300;

function isSolveTimeout(error: Error | null): boolean {
  return error instanceof ApiError && error.body.code === SOLVE_TIMEOUT;
}

function isMissingSde(error: Error | null): boolean {
  return error instanceof ApiError && error.body.code === SDE_MISSING_PLANNER_DATA;
}

function useDebounced<T>(value: T, ms: number): T {
  const [settled, setSettled] = useState(value);
  useEffect(() => {
    const timer = window.setTimeout(() => setSettled(value), ms);
    return () => window.clearTimeout(timer);
  }, [value, ms]);
  return settled;
}

function EmptyPlanner() {
  const steps = [
    "Pick your home system.",
    "Open each planet's Scan tab in game and type the Resource Density % for each resource.",
    "Compare products by ISK/month and effort.",
  ];
  return (
    <div className="iw-panel mx-auto max-w-2xl p-6">
      <h2 className="text-base font-semibold">Plan planetary production</h2>
      <p className="mt-1 text-sm text-muted">
        Start with a conservative estimate for the security band, then sharpen it with real scans.
      </p>
      <ol className="mt-5 grid gap-3 sm:grid-cols-3">
        {steps.map((step, index) => (
          <li className="border-t border-border pt-3 text-sm" key={step}>
            <b className="mr-2 font-mono text-primary">0{index + 1}</b>
            {step}
          </li>
        ))}
      </ol>
    </div>
  );
}

function MissingSde() {
  return (
    <div className="iw-panel mx-auto max-w-xl p-6 text-center">
      <AlertTriangle aria-hidden="true" className="mx-auto mb-3 h-5 w-5 text-warning" />
      <h2 className="text-base font-semibold">Planet data needs an SDE re-import</h2>
      <p className="mt-1 text-sm text-muted">The static data import predates the PI Planner.</p>
      <code className="mt-3 block rounded-md bg-background p-2 font-mono text-xs text-muted">
        make sde-import SDE_PATH=…
      </code>
    </div>
  );
}

export function PlannerPage() {
  const [params, setParams] = useSearchParams();
  const settings = useMemo(() => settingsFromQuery(params), [params]);
  // "Plan this": the product the Plan tab solves for, or the best mix.
  const target = Number(params.get("plan")) || null;
  const setTarget = useCallback(
    (typeId: number | null) =>
      setParams(
        (current) => {
          const next = new URLSearchParams(current);
          if (typeId === null) next.delete("plan");
          else next.set("plan", String(typeId));
          return next;
        },
        { replace: true },
      ),
    [setParams],
  );
  const setSettings = useCallback(
    (next: ScenarioSettings) => setParams((current) => settingsToQuery(next, current), { replace: true }),
    [setParams],
  );
  const [characters, setCharactersState] = useState<ScenarioCharacter[]>(loadCharacters);
  const setCharacters = useCallback((next: ScenarioCharacter[]) => {
    setCharactersState(next);
    storeCharacters(next);
  }, []);
  const [filter, setFilter] = useState<PlanetFilter>("all");
  // Below md each of the three is its own tab; wider, Planets and Ranking
  // share the screen and the Plan takes all of it.
  const [view, setView] = useState<"planets" | "ranking" | "plan">("planets");
  const [highlight, setHighlight] = useState<{ planetId: number; resourceTypeId: number } | null>(null);
  const [revision, setRevision] = useState(0);
  const [nowMs] = useState(() => Date.now());

  const home = settings.homeSystemId;
  const planets = useLoad(
    home === null ? null : () => getPlannerPlanets(home, settings.radius),
    `${home}:${settings.radius}`,
  );

  const scenario: PlannerScenario | null =
    home === null
      ? null
      : {
          ...settings,
          homeSystemId: home,
          characters: characters.map(({ interplanetaryConsolidation, commandCenterUpgrades }) => ({
            interplanetaryConsolidation,
            commandCenterUpgrades,
          })),
        };
  const rankKey = useDebounced(scenario === null ? "" : `${JSON.stringify(scenario)}#${revision}`, RANK_DEBOUNCE_MS);
  const ranking = useLoad(
    rankKey === "" ? null : () => rankProducts(JSON.parse(rankKey.split("#")[0]) as PlannerScenario),
    rankKey,
  );

  const onSaved = useCallback(() => setRevision((current) => current + 1), []);
  const solveKey = view === "plan" && rankKey !== "" ? `${rankKey}@${target ?? ""}` : "";
  const solution = useLoad(
    solveKey === ""
      ? null
      : () =>
          solvePlan({ ...(JSON.parse(solveKey.split("#")[0]) as PlannerScenario), targetTypeId: target }),
    solveKey,
  );
  const targetName =
    target === null
      ? null
      : ([...(ranking.data?.ranked ?? []), ...(ranking.data?.outOfReach ?? [])].find((product) => product.typeId === target)
          ?.name ?? "this product");
  const missingSde = isMissingSde(planets.error) || isMissingSde(ranking.error) || isMissingSde(solution.error);
  const homeSystem = planets.data?.systems.find((system) => system.jumps === 0) ?? null;
  const [scannedPlanets, setScannedPlanets] = useState(0);

  const addCharacter = () =>
    setCharacters([
      ...characters,
      { name: `Character ${characters.length + 1}`, interplanetaryConsolidation: 4, commandCenterUpgrades: 4 },
    ]);

  return (
    <>
      <header className="mb-3 flex flex-wrap items-center gap-3">
        <h1 className="iw-title flex items-center gap-2">
          <Globe2 aria-hidden="true" className="h-4 w-4 text-primary" />
          Planetary Interaction
        </h1>
        <span
          className="rounded-sm border border-border px-1.5 py-0.5 text-[0.65rem] uppercase text-muted"
          title="Yields are estimated from scan densities. Blank resources use a low default for the security band."
        >
          Estimates
        </span>
      </header>
      <PlanetarySectionTabs />
      <ScenarioBar
        characters={characters}
        counts={planets.data ? { systems: planets.data.systems.length, planets: planets.data.planets.length } : null}
        home={homeSystem}
        onCharactersChange={setCharacters}
        onSettingsChange={setSettings}
        settings={settings}
      />
      {home === null ? (
        <EmptyPlanner />
      ) : missingSde ? (
        <MissingSde />
      ) : planets.error ? (
        <InlineAlert title="Planets could not be loaded">{apiMessage(planets.error)}</InlineAlert>
      ) : planets.data === null ? (
        <p className="text-sm text-muted">Loading planets…</p>
      ) : (
        <>
          <div aria-label="Planner view" className="mb-2 grid grid-cols-3 md:hidden" role="tablist">
            {(
              [
                ["planets", "Planets"],
                ["ranking", "Ranking"],
                ["plan", "Plan"],
              ] as const
            ).map(([tab, label]) => (
              <button
                aria-selected={view === tab}
                className={`h-8 border-b-2 text-xs ${view === tab ? "border-primary text-foreground" : "border-transparent text-muted"}`}
                key={tab}
                onClick={() => setView(tab)}
                role="tab"
                type="button"
              >
                {label}
              </button>
            ))}
          </div>
          <div aria-label="Planner sections" className="mb-2 hidden items-end border-b border-border md:flex" role="tablist">
            {(
              [
                ["ranking", "Scan & rank", view !== "plan"],
                ["plan", "Plan", view === "plan"],
              ] as const
            ).map(([tab, label, selected]) => (
              <button
                aria-selected={selected}
                className={`h-9 border-b-2 px-3 text-xs font-semibold ${
                  selected ? "border-primary text-foreground" : "border-transparent text-muted hover:text-foreground"
                }`}
                key={tab}
                onClick={() => {
                  if (!selected) setView(tab);
                }}
                role="tab"
                type="button"
              >
                {label}
              </button>
            ))}
          </div>
          <div className={view === "plan" ? "hidden" : "grid items-start gap-3 xl:grid-cols-[minmax(0,46fr)_minmax(0,54fr)]"}>
            <div className={view === "planets" ? "min-w-0" : "hidden min-w-0 md:block"}>
              <PlanetsPanel
                data={planets.data}
                defaultCustomsRate={settings.defaultCustomsRate}
                filter={filter}
                highlight={highlight}
                nowMs={nowMs}
                onFilterChange={setFilter}
                onSaved={onSaved}
                onScannedCountChange={setScannedPlanets}
              />
            </div>
            <div className={view === "ranking" ? "min-w-0" : "hidden min-w-0 md:block"}>
              {ranking.error ? (
                <InlineAlert title="Ranking failed">{apiMessage(ranking.error)}</InlineAlert>
              ) : ranking.data === null ? (
                <p className="text-sm text-muted">Ranking products…</p>
              ) : (
                <RankingPanel
                  actions={{
                    tryRadius: (radius) => setSettings({ ...settings, radius }),
                    addCharacter,
                    allowBuying: () => setSettings({ ...settings, buyIntermediates: true }),
                    showNotScanned: () => {
                      setFilter("unscanned");
                      setView("planets");
                    },
                    estimateUnscanned: () => setSettings({ ...settings, estimateUnscanned: true }),
                    planThis: (typeId) => {
                      setTarget(typeId);
                      setView("plan");
                    },
                  }}
                  estimating={settings.estimateUnscanned}
                  haulDays={settings.haulDays}
                  radius={settings.radius}
                  ranking={ranking.data}
                  refreshing={ranking.loading}
                  scannedPlanets={scannedPlanets}
                />
              )}
            </div>
          </div>
          {view === "plan" ? (
            <div className="min-w-0">
              {targetName !== null ? (
                <div className="mb-2 flex flex-wrap items-center gap-2 rounded-md border border-primary/40 bg-primary/10 px-3 py-1.5 text-xs">
                  <span>
                    Planning <b className="text-foreground">{targetName}</b> only: it and its intermediates are the only
                    things sold.
                  </span>
                  <button className="ml-auto text-primary hover:underline" onClick={() => setTarget(null)} type="button">
                    Show the best mix
                  </button>
                </div>
              ) : null}
              {solution.error ? (
                <InlineAlert title={isSolveTimeout(solution.error) ? "Solve timed out" : "Solve failed"}>
                  {apiMessage(solution.error)}
                </InlineAlert>
              ) : solution.data === null ? (
                <p className="flex items-center gap-2 text-sm text-muted" role="status">
                  <RefreshCw aria-hidden="true" className="h-3.5 w-3.5 animate-spin text-primary" />
                  Optimising colonies…
                </p>
              ) : (
                <section aria-label="Plan" className="iw-panel min-w-0">
                  <PlanPanel
                    characterNames={characters.map((character) => character.name)}
                    haulDays={settings.haulDays}
                    onCadence={(resetDays, haulDays) => setSettings({ ...settings, resetDays, haulDays })}
                    onScanFirst={(planetId, resourceTypeId) => {
                      setFilter("all");
                      setHighlight({ planetId, resourceTypeId });
                      setView("planets");
                    }}
                    resetDays={settings.resetDays}
                    solution={solution.data}
                    solving={solution.loading}
                  />
                </section>
              )}
            </div>
          ) : null}
        </>
      )}
    </>
  );
}
