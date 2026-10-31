import { requestJson } from "./workspace";

/**
 * Mirrors `apps/iskworks-api/src/routes/pi_planner.rs`. Densities, rates and
 * money are decimal strings. A static data import without PI Planner data
 * fails with `409 sde_missing_planner_data`.
 */
export type SecurityBand = "high" | "low" | "null" | "wormhole";
export type YieldSource = "headValue" | "scanned" | "default";

export const SDE_MISSING_PLANNER_DATA = "sde_missing_planner_data";
/** The reset and haul intervals the planner offers, in days. */
export const INTERVAL_DAYS = [1, 2, 3, 4, 7, 14] as const;
export const MAX_RADIUS = 5;

export interface PlannerSystem {
  solarSystemId: number;
  name: string;
  securityStatus: string | null;
  regionName: string | null;
  band: SecurityBand;
  jumps: number;
}

export interface PlannerResource {
  typeId: number;
  name: string;
  densityPct: string | null;
  headValue: string | null;
  updatedAt: string | null;
  /** The density assumed while nothing is entered. */
  defaultDensityPct: string;
}

export interface PlannerPlanet {
  planetId: number;
  name: string;
  solarSystemId: number;
  jumps: number;
  typeId: number;
  /** The SDE name, e.g. `Planet (Barren)`. */
  typeName: string;
  customsRate: string | null;
  contested: boolean;
  settingsUpdatedAt: string | null;
  /** Alphabetical, as in the in-game scan list. */
  resources: PlannerResource[];
}

export interface PlannerPlanets {
  systems: PlannerSystem[];
  planets: PlannerPlanet[];
}

export interface DensityWrite {
  planetId: number;
  resourceTypeId: number;
  /** Both empty clears the reading. */
  densityPct: string | null;
  headValue: string | null;
}

export interface PlanetSettingsWrite {
  planetId: number;
  /** 0.1 = 10 %. Empty uses the scenario's default. */
  customsRate: string | null;
  contested: boolean;
}

export interface PlannerCharacterSkills {
  eveCharacterId: number;
  name: string;
  /** `null` until the character's skills have synced. */
  interplanetaryConsolidation: number | null;
  commandCenterUpgrades: number | null;
}

export interface PlannerCharacter {
  interplanetaryConsolidation: number;
  commandCenterUpgrades: number;
}

export interface PlannerScenario {
  homeSystemId: number;
  radius: number;
  characters: PlannerCharacter[];
  /** 0.1 = 10 %. */
  defaultCustomsRate: string;
  resetDays: number;
  haulDays: number;
  upliftAtLogin: boolean;
  buyIntermediates: boolean;
  maxColonies: number | null;
}

export interface RankedInput {
  resourceTypeId: number;
  resourceName: string;
  planetId: number;
  planetName: string;
  source: YieldSource;
  contested: boolean;
}

export interface PlannerEffort {
  loginsPerWeekByCharacter: string[];
  haulRunsPerWeek: string;
  stopsPerRun: number;
  m3PerRun: string;
}

export interface RankedProduct {
  typeId: number;
  name: string;
  tier: number;
  iskPerMonth: string;
  unitsPerHour: string;
  colonies: number;
  chains: number;
  inputs: RankedInput[];
  bought: { typeId: number; name: string }[];
  /** 0–1. */
  confidence: string;
  effort: PlannerEffort;
  /** Hauls must come this often for the skyhooks to hold the inputs,
   * sooner than the scenario's haul interval. */
  haulConstrainedDays: number | null;
}

export type OutOfReachReason =
  | { kind: "missingResource"; typeId: number; name: string }
  | { kind: "noFactoryPlanet" }
  | { kind: "needsColonies"; needed: number; available: number }
  | { kind: "overflowsSkyhook" }
  | { kind: "noRoom" }
  | { kind: "noPrice" };

export interface OutOfReachProduct {
  typeId: number;
  name: string;
  tier: number;
  reason: OutOfReachReason;
}

export interface PlannerRanking {
  priceObservedAt: string | null;
  availableColonies: number;
  ranked: RankedProduct[];
  outOfReach: OutOfReachProduct[];
}

const JSON_HEADERS = { "Content-Type": "application/json" };

export function getPlannerPlanets(systemId: number, radius: number): Promise<PlannerPlanets> {
  return requestJson<PlannerPlanets>(`/api/planetary/planner/planets?systemId=${systemId}&radius=${radius}`);
}

export function getPlannerCharacters(): Promise<PlannerCharacterSkills[]> {
  return requestJson<PlannerCharacterSkills[]>("/api/planetary/planner/characters");
}

export function saveDensities(writes: DensityWrite[]): Promise<void> {
  return requestJson<void>("/api/planetary/planner/densities", {
    method: "PUT",
    headers: JSON_HEADERS,
    body: JSON.stringify(writes),
  });
}

export function savePlanetSettings(writes: PlanetSettingsWrite[]): Promise<void> {
  return requestJson<void>("/api/planetary/planner/planet-settings", {
    method: "PUT",
    headers: JSON_HEADERS,
    body: JSON.stringify(writes),
  });
}

export function rankProducts(scenario: PlannerScenario): Promise<PlannerRanking> {
  return requestJson<PlannerRanking>("/api/planetary/planner/rank", {
    method: "POST",
    headers: JSON_HEADERS,
    body: JSON.stringify(scenario),
  });
}
