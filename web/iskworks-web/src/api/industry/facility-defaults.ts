import { json, request } from "./request";
import type { FacilityRole } from "./facilities";

/** How a target row reads in the Defaults view. */
export type FacilityDefaultStatus = "set" | "coverageLost" | "fallback" | "none" | "unknownTarget";

/** A facility shown for a target, with its bonus for that target. */
export interface FacilityDefaultChoice {
  facilityProfileId: string;
  facilityName: string;
  materialBonusPercent: string;
  timeBonusPercent: string;
}

/** One EVE rig target of a role; `filterId: null` is the role's fallback. */
export interface FacilityDefaultTarget {
  role: FacilityRole;
  filterId: number | null;
  name: string;
  default: FacilityDefaultChoice | null;
  /** Only present when it differs from the default. */
  suggestion: FacilityDefaultChoice | null;
  status: FacilityDefaultStatus;
}

/** One active facility's rig-applicability-aware bonus for one target. */
export interface FacilityTargetBonus {
  facilityProfileId: string;
  role: FacilityRole;
  filterId: number | null;
  materialBonusPercent: string;
  timeBonusPercent: string;
  covered: boolean;
}

/** A facility as an invention site: its role and rig bonuses on an
 * invention job's cost index line and time. */
export interface InventionFacilityChoice {
  facilityProfileId: string;
  facilityName: string;
  costBonusPercent: string;
  timeBonusPercent: string;
}

/** The one invention default, and every active facility it could be. */
export interface InventionFacilityDefault {
  default: InventionFacilityChoice | null;
  facilities: InventionFacilityChoice[];
}

export interface FacilityDefaultsView {
  targets: FacilityDefaultTarget[];
  bonuses: FacilityTargetBonus[];
  invention: InventionFacilityDefault;
}

/** The facilities to pre-select for inventing and building a product. */
export interface ProductFacilityDefaults {
  manufacturingFacilityId: string | null;
  inventionFacilityId: string | null;
}

/** `facilityProfileId: null` clears the target. */
export interface FacilityDefaultAssignment {
  role: FacilityRole;
  filterId: number | null;
  facilityProfileId: string | null;
}

export function getFacilityDefaults(): Promise<FacilityDefaultsView> {
  return request("/api/industry/facility-defaults");
}

export function assignFacilityDefaults(
  assignments: FacilityDefaultAssignment[],
): Promise<FacilityDefaultsView> {
  return request("/api/industry/facility-defaults", json("PUT", { assignments }));
}

/** `facilityProfileId: null` clears the invention default. */
export function assignInventionFacilityDefault(facilityProfileId: string | null): Promise<FacilityDefaultsView> {
  return request("/api/industry/facility-defaults/invention", json("PUT", { facilityProfileId }));
}

/** Without a product only the invention default is known. */
export function getProductFacilityDefaults(
  productTypeId: number | null,
  signal?: AbortSignal,
): Promise<ProductFacilityDefaults> {
  const query = productTypeId === null ? "" : `?productTypeId=${productTypeId}`;
  return request(`/api/industry/facility-defaults/for-product${query}`, { signal });
}
