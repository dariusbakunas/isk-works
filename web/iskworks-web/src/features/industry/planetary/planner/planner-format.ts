// Pure rules for the PI Planner page: planet and resource labels, reading
// age, and the scenario's URL / localStorage round trip.

import { INTERVAL_DAYS, MAX_RADIUS, type PlannerCharacter, type PlannerResource, type PlannerScenario } from "../../../../api/pi-planner";

const DAY_MS = 86_400_000;
/** Hotspots move and deplete: older readings are flagged. */
export const STALE_AFTER_DAYS = 30;
export const MAX_SKILL_LEVEL = 5;

/** `Planet (Barren)` -> `barren`, the key of `PLANET_TYPES`. */
export function planetTypeKey(typeName: string): string {
  const match = /\(([^)]+)\)/.exec(typeName);
  return (match ? match[1] : typeName).trim().toLowerCase();
}

/** Two-letter resource code for narrow cells: `Base Metals` -> `BM`. */
export function resourceCode(name: string): string {
  const words = name.split(/[\s-]+/).filter(Boolean);
  const code = words.length > 1 ? words.map((word) => word[0]).join("") : name.slice(0, 2);
  return code.slice(0, 2).toUpperCase();
}

export function isStaleReading(updatedAt: string | null, nowMs: number): boolean {
  if (updatedAt === null) return false;
  return nowMs - new Date(updatedAt).getTime() > STALE_AFTER_DAYS * DAY_MS;
}

export function readingAgeDays(updatedAt: string, nowMs: number): number {
  return Math.max(0, Math.floor((nowMs - new Date(updatedAt).getTime()) / DAY_MS));
}

/** Filled segments of the 10-segment density bar. */
export function densitySegments(pct: number | null): number {
  if (pct === null || !Number.isFinite(pct)) return 0;
  return Math.max(0, Math.min(10, Math.round(pct / 10)));
}

/** A reading counts as scanned with either a density or a head value. */
export function isScanned(resource: PlannerResource): boolean {
  return resource.densityPct !== null || resource.headValue !== null;
}

/** Digits and at most one decimal point, three integer digits. */
export function cleanDensityInput(text: string): string {
  const [whole = "", ...rest] = text.replace(/[^\d.]/g, "").split(".");
  const fraction = rest.join("").slice(0, 2);
  return rest.length > 0 ? `${whole.slice(0, 3)}.${fraction}` : whole.slice(0, 3);
}

export function isValidDensity(text: string): boolean {
  if (text === "") return true;
  const value = Number(text);
  return Number.isFinite(value) && value >= 0 && value <= 100;
}

/** `"0.1"` -> `"10"`. */
export function ratePercent(rate: string): string {
  return String(Math.round(Number(rate) * 10_000) / 100);
}

/** `"10"` -> `"0.1"`, `null` when not a 0–100 number. */
export function percentRate(text: string): string | null {
  if (text.trim() === "") return null;
  const value = Number(text);
  if (!Number.isFinite(value) || value < 0 || value > 100) return null;
  return String(Math.round(value * 100) / 10_000);
}

/** A scenario character: skill levels plus a label for the page. */
export type ScenarioCharacter = PlannerCharacter & { name: string };

export function colonies(character: PlannerCharacter): number {
  return 1 + Math.min(MAX_SKILL_LEVEL, character.interplanetaryConsolidation);
}

export function clampLevel(level: number): number {
  if (!Number.isFinite(level)) return 0;
  return Math.max(0, Math.min(MAX_SKILL_LEVEL, Math.round(level)));
}

// ---- Scenario persistence ---------------------------------------------------

/** The scenario minus its characters, which live in localStorage. */
export type ScenarioSettings = Omit<PlannerScenario, "homeSystemId" | "characters"> & {
  homeSystemId: number | null;
};

export const DEFAULT_SETTINGS: ScenarioSettings = {
  homeSystemId: null,
  radius: 1,
  defaultCustomsRate: "0.1",
  resetDays: 3,
  haulDays: 3,
  upliftAtLogin: true,
  buyIntermediates: false,
  maxColonies: null,
  estimateUnscanned: false,
};

export const DEFAULT_CHARACTERS: ScenarioCharacter[] = [
  { name: "Character 1", interplanetaryConsolidation: 4, commandCenterUpgrades: 4 },
];

function interval(value: string | null, fallback: number): number {
  const days = Number(value);
  return (INTERVAL_DAYS as readonly number[]).includes(days) ? days : fallback;
}

export function settingsFromQuery(params: URLSearchParams): ScenarioSettings {
  const system = Number(params.get("system"));
  const radius = Number(params.get("radius") ?? DEFAULT_SETTINGS.radius);
  const tax = params.get("tax");
  const max = Number(params.get("max"));
  return {
    homeSystemId: Number.isInteger(system) && system > 0 ? system : null,
    radius: Number.isInteger(radius) && radius >= 0 && radius <= MAX_RADIUS ? radius : DEFAULT_SETTINGS.radius,
    defaultCustomsRate: tax !== null && percentRate(tax) !== null ? (percentRate(tax) as string) : DEFAULT_SETTINGS.defaultCustomsRate,
    resetDays: interval(params.get("reset"), DEFAULT_SETTINGS.resetDays),
    haulDays: interval(params.get("haul"), DEFAULT_SETTINGS.haulDays),
    upliftAtLogin: params.get("uplift") !== "0",
    buyIntermediates: params.get("buy") === "1",
    maxColonies: Number.isInteger(max) && max > 0 ? max : null,
    estimateUnscanned: params.get("estimate") === "1",
  };
}

/** Writes only what differs from the defaults, so a fresh page has a bare URL. */
export function settingsToQuery(settings: ScenarioSettings, params: URLSearchParams): URLSearchParams {
  const next = new URLSearchParams(params);
  const put = (key: string, value: string | null) => (value === null ? next.delete(key) : next.set(key, value));
  put("system", settings.homeSystemId === null ? null : String(settings.homeSystemId));
  put("radius", settings.radius === DEFAULT_SETTINGS.radius ? null : String(settings.radius));
  put(
    "tax",
    settings.defaultCustomsRate === DEFAULT_SETTINGS.defaultCustomsRate ? null : ratePercent(settings.defaultCustomsRate),
  );
  put("reset", settings.resetDays === DEFAULT_SETTINGS.resetDays ? null : String(settings.resetDays));
  put("haul", settings.haulDays === DEFAULT_SETTINGS.haulDays ? null : String(settings.haulDays));
  put("uplift", settings.upliftAtLogin ? null : "0");
  put("buy", settings.buyIntermediates ? "1" : null);
  put("max", settings.maxColonies === null ? null : String(settings.maxColonies));
  put("estimate", settings.estimateUnscanned ? "1" : null);
  return next;
}

const CHARACTERS_KEY = "iskworks.piPlanner.characters";

export function loadCharacters(): ScenarioCharacter[] {
  try {
    const raw = window.localStorage.getItem(CHARACTERS_KEY);
    if (!raw) return DEFAULT_CHARACTERS;
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed) || parsed.length === 0) return DEFAULT_CHARACTERS;
    return parsed.map((entry, index) => ({
      name: typeof entry?.name === "string" && entry.name ? entry.name : `Character ${index + 1}`,
      interplanetaryConsolidation: clampLevel(Number(entry?.interplanetaryConsolidation)),
      commandCenterUpgrades: clampLevel(Number(entry?.commandCenterUpgrades)),
    }));
  } catch {
    return DEFAULT_CHARACTERS;
  }
}

export function storeCharacters(characters: ScenarioCharacter[]): void {
  try {
    window.localStorage.setItem(CHARACTERS_KEY, JSON.stringify(characters));
  } catch {
    // Private windows and blocked storage: the list just isn't remembered.
  }
}
