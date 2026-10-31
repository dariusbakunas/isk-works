import { useCallback, useEffect, useMemo, useState } from "react";
import { useSearchParams } from "react-router";
import { AlertTriangle, Globe2 } from "lucide-react";

import {
  getPlannerPlanets,
  rankProducts,
  SDE_MISSING_PLANNER_DATA,
  type PlannerScenario,
} from "../../../../api/pi-planner";
import { ApiError } from "../../../../api/workspace";
import { InlineAlert } from "../../../../components/primitives";
import { useLoad } from "../../../../hooks/use-load";
import { apiMessage } from "../../inventory/shared";
import { PlanetsPanel, type PlanetFilter } from "./planets-panel";
import {
  isScanned,
  loadCharacters,
  settingsFromQuery,
  settingsToQuery,
  storeCharacters,
  type ScenarioCharacter,
  type ScenarioSettings,
} from "./planner-format";
import { PlanetarySectionTabs } from "../planetary-section-tabs";
import { RankingPanel } from "./ranking-panel";
import { ScenarioBar } from "./scenario-bar";

/** Re-rank once the scenario has been still for this long. */
const RANK_DEBOUNCE_MS = 300;

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
  const [mobileTab, setMobileTab] = useState<"planets" | "ranking">("planets");
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
  const missingSde = isMissingSde(planets.error) || isMissingSde(ranking.error);
  const homeSystem = planets.data?.systems.find((system) => system.jumps === 0) ?? null;
  const scannedPlanets = planets.data?.planets.filter((planet) => planet.resources.some(isScanned)).length ?? 0;

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
          <div aria-label="Planner view" className="mb-2 grid grid-cols-2 md:hidden" role="tablist">
            {(["planets", "ranking"] as const).map((tab) => (
              <button
                aria-selected={mobileTab === tab}
                className={`h-8 border-b-2 text-xs capitalize ${mobileTab === tab ? "border-primary text-foreground" : "border-transparent text-muted"}`}
                key={tab}
                onClick={() => setMobileTab(tab)}
                role="tab"
                type="button"
              >
                {tab}
              </button>
            ))}
          </div>
          <div className="grid items-start gap-3 xl:grid-cols-[minmax(0,46fr)_minmax(0,54fr)]">
            <div className={mobileTab === "planets" ? "min-w-0" : "hidden min-w-0 md:block"}>
              <PlanetsPanel
                data={planets.data}
                defaultCustomsRate={settings.defaultCustomsRate}
                filter={filter}
                nowMs={nowMs}
                onFilterChange={setFilter}
                onSaved={onSaved}
              />
            </div>
            <div className={mobileTab === "ranking" ? "min-w-0" : "hidden min-w-0 md:block"}>
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
                      setMobileTab("planets");
                    },
                  }}
                  haulDays={settings.haulDays}
                  radius={settings.radius}
                  ranking={ranking.data}
                  refreshing={ranking.loading}
                  scannedPlanets={scannedPlanets}
                />
              )}
            </div>
          </div>
        </>
      )}
    </>
  );
}
