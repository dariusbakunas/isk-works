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

export interface FacilityDefaultsView {
  targets: FacilityDefaultTarget[];
  bonuses: FacilityTargetBonus[];
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
