import { requestJson } from "./workspace";

/** Mirrors `apps/iskworks-api/src/routes/planetary.rs`. Rates and money are decimal strings. */
export type PlanetAttention = "amber" | "red";
export type StorageKind = "L" | "S" | "C";
export type PlanetarySyncState = "missing" | "current" | "refreshing" | "failed";

export interface PlanetExtractor {
  pinId: number;
  productTypeId: number;
  productName: string;
  expiresAt: string | null;
  unitsPerHour: string;
}

export interface PlanetProduction {
  schematicId: number;
  name: string;
  factoryCount: number;
  outputTypeId: number;
  outputPerHour: string;
}

export interface PlanetImport {
  typeId: number;
  name: string;
  qtyPerHour: string;
  lastsHours: string;
}

export interface PlanetExport {
  typeId: number;
  name: string;
  unitsPerHour: string;
  iskPerMonth: string | null;
  excluded: boolean;
}

export interface PlanetStorageItem {
  typeId: number;
  name: string;
  quantity: number;
  volumeM3: string;
  value: string | null;
}

export interface PlanetStorage {
  pinId: number;
  kind: StorageKind;
  capacityM3: string;
  usedM3: string;
  fillPercent: string;
  value: string;
  contents: PlanetStorageItem[];
}

export interface Planet {
  planetId: number;
  name: string;
  planetType: string;
  solarSystemId: number;
  solarSystemName: string | null;
  security: string | null;
  upgradeLevel: number;
  lastUpdate: string;
  attention: PlanetAttention | null;
  iskPerMonth: string;
  extractors: PlanetExtractor[];
  production: PlanetProduction[];
  imports: PlanetImport[];
  exports: PlanetExport[];
  storage: PlanetStorage[];
}

export interface PlanetaryCharacter {
  connectionId: string;
  eveCharacterId: number;
  name: string;
  piSkillLevel: number | null;
  scopeGranted: boolean;
  sync: { observedAt: string | null; refreshState: PlanetarySyncState; lastError: string | null };
  iskPerMonth: string;
  nextExpiryAt: string | null;
  alertCount: number;
  planets: Planet[];
}

export interface PlanetaryOverview {
  priceObservedAt: string | null;
  summary: {
    iskPerMonth: string;
    planetCount: number;
    characterCount: number;
    nextAction: { characterName: string; planetName: string; at: string } | null;
    alerts: { expired: number; storageFull: number; starved: number };
  };
  characters: PlanetaryCharacter[];
}

export interface ExcludedExport {
  characterId: number;
  planetId: number;
  typeId: number;
}

export interface PlanetaryPreferences {
  excludedExports: ExcludedExport[];
  characterOrder: number[];
}

export function getPlanetary(): Promise<PlanetaryOverview> {
  return requestJson<PlanetaryOverview>("/api/planetary");
}

export function getPlanetaryPreferences(): Promise<PlanetaryPreferences> {
  return requestJson<PlanetaryPreferences>("/api/planetary/preferences");
}

export function savePlanetaryPreferences(preferences: PlanetaryPreferences): Promise<PlanetaryPreferences> {
  return requestJson<PlanetaryPreferences>("/api/planetary/preferences", {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(preferences),
  });
}
