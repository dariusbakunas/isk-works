import { jsonRequest } from "./json-request";

/** Display order is the order of this list (mirrors the API's `MaterialCategory`). */
export const MATERIAL_CATEGORIES = [
  "Moon Materials",
  "Minerals",
  "Ice Products",
  "Reaction Intermediates",
  "Planetary",
  "Fuel Blocks",
  "Components",
  "Ships & Modules",
  "Other",
] as const;

export type MaterialCategory = (typeof MATERIAL_CATEGORIES)[number];

/** Money is always a decimal string; never do arithmetic on it in floats. */
export interface RunwayKpis {
  shortfallValue: string;
  materialsShort: number;
  materialsTotal: number;
  epicsBlocked: number;
  epicsTotal: number;
  /** Share of outstanding demand value covered by stock; `null` when nothing is priced. */
  demandCoveredPercent: number | null;
}

export interface RunwayEpicReadiness {
  orderId: string;
  displayName: string;
  createdAt: string;
  /** By value; `null` when nothing outstanding is priced. */
  coveredPercent: number | null;
  shortValue: string;
  blocked: boolean;
}

export interface RunwayMaterialEpic {
  orderId: string;
  required: number;
  consumed: number;
  reserved: number;
  /** `required - consumed`: still to be used. */
  outstanding: number;
  short: number;
}

export interface RunwayMaterial {
  typeId: number;
  name: string;
  category: MaterialCategory;
  /** Still to be used by the Epics. */
  needed: number;
  onHand: number;
  /** Held for these Epics. */
  reserved: number;
  short: number;
  unitCost: string | null;
  /** `null` when the shortfall has no price. */
  shortValue: string | null;
  coveragePercent: number;
  /** Largest shortfall first. */
  epics: RunwayMaterialEpic[];
}

export interface InventoryRunway {
  kpis: RunwayKpis;
  /** Oldest first: the order free stock is handed out in. */
  epics: RunwayEpicReadiness[];
  materials: RunwayMaterial[];
  missingPriceCount: number;
}

const request = jsonRequest({
  unavailable: "ISK Works API is unavailable.",
  failed: "Inventory runway request failed.",
});

export function getInventoryRunway(
  category: MaterialCategory | null,
  signal?: AbortSignal,
): Promise<InventoryRunway> {
  const query = category ? `?${new URLSearchParams({ category })}` : "";
  return request(`/api/inventory/runway${query}`, { signal });
}
