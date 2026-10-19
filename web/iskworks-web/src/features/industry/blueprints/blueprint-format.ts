import type { BlueprintLibraryKind, BlueprintLibraryRow } from "../../../api/industry";
import type { BlueprintGrouping } from "./blueprint-library-url-state";

export function kindLabel(kind: BlueprintLibraryKind): string {
  return kind === "original" ? "Original" : kind === "copy" ? "Copy" : "Unknown";
}

/** Originals never run out; a copy with no recorded runs has none left. */
export function formatRuns(row: Pick<BlueprintLibraryRow, "kind" | "licensedRuns">): string {
  if (row.kind === "original") return "∞";
  return row.licensedRuns === null ? "0" : row.licensedRuns.toLocaleString();
}

export function isUsable(row: Pick<BlueprintLibraryRow, "kind" | "licensedRuns">): boolean {
  return row.kind === "original" || (row.licensedRuns ?? 0) > 0;
}

/**
 * Create Build prefilled with this exact blueprint. Create Build's route
 * prefill only covers manufacturing, so reaction formulas (and blueprints
 * with no known product) get no link.
 */
export function planBuildHref(row: BlueprintLibraryRow): string | null {
  if (row.activity !== "manufacturing" || row.productTypeId === null) return null;
  const params = new URLSearchParams({
    blueprintTypeId: String(row.blueprintTypeId),
    observationId: row.observationId,
  });
  return `/builds/new?${params.toString()}`;
}

export interface BlueprintGroup {
  key: string;
  label: string;
  rows: BlueprintLibraryRow[];
}

/**
 * Buckets rows in the order the server sorted them. Groups are alphabetical
 * with the unclassified bucket ("Unknown") last.
 */
export function groupBlueprints(rows: BlueprintLibraryRow[], grouping: BlueprintGrouping): BlueprintGroup[] {
  if (grouping === "none") return [{ key: "all", label: "All blueprints", rows }];
  const groups = new Map<string, BlueprintGroup>();
  for (const row of rows) {
    const [key, label] = grouping === "category"
      ? [row.productCategoryId === null ? "unknown" : `category-${row.productCategoryId}`, row.productCategoryName ?? "Unknown"]
      : [`location-${row.locationId}`, row.locationName ?? `Location ${row.locationId}`];
    const group = groups.get(key) ?? { key, label, rows: [] };
    group.rows.push(row);
    groups.set(key, group);
  }
  return [...groups.values()].sort((a, b) => {
    if (a.key === "unknown") return 1;
    if (b.key === "unknown") return -1;
    return a.label.localeCompare(b.label);
  });
}
