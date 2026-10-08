import type { FacilityProfile } from "../../../api/industry";

/**
 * `"all"` is the default (no restriction). The other two map 1:1 onto the
 * profile's own `role` domain field -- activity is never inferred from the
 * facility name.
 */
export type FacilityActivityFilter = "all" | "manufacturing" | "reaction";

export const FACILITY_ACTIVITY_FILTERS: { id: FacilityActivityFilter; label: string }[] = [
  { id: "all", label: "All" },
  { id: "manufacturing", label: "Manufacturing" },
  { id: "reaction", label: "Reactions" },
];

export interface FacilityFilterState {
  activity: FacilityActivityFilter;
  /** Free text, matched case-insensitively as a trimmed substring. */
  search: string;
}

export const EMPTY_FACILITY_FILTERS: FacilityFilterState = { activity: "all", search: "" };

export function hasActiveFacilityFilters(filters: FacilityFilterState): boolean {
  return filters.activity !== "all" || filters.search.trim() !== "";
}

function matchesFacilityActivity(
  profile: FacilityProfile,
  activity: FacilityActivityFilter,
): boolean {
  return activity === "all" || profile.role === activity;
}

// Deliberately narrow: the card's own visible identifiers -- profile name,
// solar system, and captured structure type. Never notes or numeric
// assumptions.
function matchesFacilitySearch(profile: FacilityProfile, query: string): boolean {
  const needle = query.trim().toLowerCase();
  if (!needle) return true;
  return [profile.name, profile.solarSystemName, profile.structureTypeName].some(
    (field) => field.toLowerCase().includes(needle),
  );
}

export function facilityMatchesFilters(
  profile: FacilityProfile,
  filters: FacilityFilterState,
): boolean {
  return (
    matchesFacilityActivity(profile, filters.activity) &&
    matchesFacilitySearch(profile, filters.search)
  );
}

/** Label for the filtered-empty state, e.g. "No manufacturing facilities
 * match these filters." */
export function filteredFacilityEmptyMessage(filters: FacilityFilterState): string {
  const scope =
    filters.activity === "manufacturing"
      ? "manufacturing "
      : filters.activity === "reaction"
        ? "reaction "
        : "";
  return `No ${scope}facilities match these filters.`;
}
