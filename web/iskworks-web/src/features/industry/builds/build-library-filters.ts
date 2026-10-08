import type { Build } from "../../../api/industry";

export type BlueprintFilter = "All" | "On hand";
export type BuildSort = "Recently updated" | "Name" | "Created";

export interface BuildLibraryCriteria {
  search: string;
  category: string;
  blueprint: BlueprintFilter;
  sort: BuildSort;
}

export const BLUEPRINT_OPTIONS: BlueprintFilter[] = ["All", "On hand"];
export const SORT_OPTIONS: BuildSort[] = ["Recently updated", "Name", "Created"];

export const DEFAULT_CRITERIA: BuildLibraryCriteria = {
  search: "",
  category: "All",
  blueprint: "All",
  sort: "Recently updated",
};

function outputName(build: Build): string {
  return build.recipe.products[0]?.typeName ?? "";
}

function updatedTime(build: Build): number {
  return new Date(build.updatedAt).getTime();
}

function createdTime(build: Build): number {
  return new Date(build.createdAt).getTime();
}

export function filterAndSortBuilds(builds: Build[], criteria: BuildLibraryCriteria): Build[] {
  const query = criteria.search.trim().toLowerCase();
  const matched = builds.filter((build) => {
    if (query) {
      const haystack = `${build.name}\n${outputName(build)}`.toLowerCase();
      if (!haystack.includes(query)) return false;
    }
    if (criteria.category !== "All" && (build.productCategoryName ?? "") !== criteria.category) {
      return false;
    }
    if (criteria.blueprint === "On hand" && !build.hasOwnedBlueprint) return false;
    return true;
  });

  const sorted = [...matched];
  switch (criteria.sort) {
    case "Name":
      sorted.sort((a, b) => a.name.localeCompare(b.name));
      break;
    case "Created":
      sorted.sort((a, b) => createdTime(b) - createdTime(a));
      break;
    case "Recently updated":
    default:
      sorted.sort((a, b) => updatedTime(b) - updatedTime(a));
      break;
  }
  return sorted;
}

/** "All" plus the distinct output categories present in the loaded
 * library, alphabetically -- real SDE `invCategories` names, never a
 * hand-authored taxonomy. */
export function categoryOptions(builds: Build[]): string[] {
  const names = new Set<string>();
  for (const build of builds) {
    if (build.productCategoryName) names.add(build.productCategoryName);
  }
  return ["All", ...[...names].sort((a, b) => a.localeCompare(b))];
}

export function hasActiveFilters(criteria: BuildLibraryCriteria): boolean {
  return (
    criteria.search.trim() !== "" ||
    criteria.category !== "All" ||
    criteria.blueprint !== "All" ||
    criteria.sort !== DEFAULT_CRITERIA.sort
  );
}

/** Human-readable summary of each non-default filter, for the
 * zero-results empty state. */
export function activeFilterChips(criteria: BuildLibraryCriteria): string[] {
  const chips: string[] = [];
  if (criteria.search.trim()) chips.push(`Search: "${criteria.search.trim()}"`);
  if (criteria.category !== "All") chips.push(`Category: ${criteria.category}`);
  if (criteria.blueprint !== "All") chips.push(`Blueprint: ${criteria.blueprint}`);
  return chips;
}
