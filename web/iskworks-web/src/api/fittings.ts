import { jsonRequest } from "./json-request";

/** Mirrors `iskworks_core::fitting::FittingEngineInfo`. */
export interface FittingEngineInfo {
  /** The pinned engine data package version, e.g. `13.3650286.0`. */
  dataVersion: string;
  major: number;
  /** The EVE build the engine data was made from. */
  buildNumber: number;
}

/** Mirrors `iskworks_core::fitting::DamageProfilePreset`; only the ratios matter. */
export interface DamageProfilePreset {
  id: string;
  name: string;
  em: number;
  thermal: number;
  kinetic: number;
  explosive: number;
}

/** Mirrors `routes/fittings.rs` `FittingsMeta`. */
export interface FittingsMeta {
  /** Whether fit stats can be calculated (the engine data loaded). */
  available: boolean;
  engine: FittingEngineInfo | null;
  /** The active SDE import's build, versioned apart from the engine data. */
  sde: { buildNumber: string } | null;
  /** Uniform first. */
  damageProfiles: DamageProfilePreset[];
}

const request = jsonRequest({
  unavailable: "ISK Works API is unavailable.",
  failed: "Fittings request failed.",
});

export function getFittingsMeta(signal?: AbortSignal): Promise<FittingsMeta> {
  return request<FittingsMeta>("/api/fittings/meta", { signal });
}
