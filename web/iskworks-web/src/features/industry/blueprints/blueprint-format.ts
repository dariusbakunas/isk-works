import type { BlueprintLibraryKind, BlueprintLibraryRow } from "../../../api/industry";
import type { BlueprintGrouping } from "./blueprint-library-url-state";

export function kindLabel(kind: BlueprintLibraryKind): string {
  return kind === "original" ? "Original" : kind === "copy" ? "Copy" : "Original (never used)";
}

type RunsFields = Pick<BlueprintLibraryRow, "activity" | "kind" | "licensedRuns">;

/**
 * Originals have unlimited runs. "unknown" is also an original: ESI reports
 * a positive quantity (not -1/-2) for a stack of never-used originals. And
 * reaction formulas cannot be researched or copied, so they are always
 * originals whatever kind ESI reported.
 */
export function hasUnlimitedRuns(row: RunsFields): boolean {
  return row.activity === "reaction" || row.kind !== "copy";
}

/** Originals never run out; a copy with no recorded runs has none left. */
export function formatRuns(row: RunsFields): string {
  if (hasUnlimitedRuns(row)) return "∞";
  return row.licensedRuns === null ? "0" : row.licensedRuns.toLocaleString();
}

export function isUsable(row: RunsFields): boolean {
  return hasUnlimitedRuns(row) || (row.licensedRuns ?? 0) > 0;
}

/** ME / TE do not apply to reactions. */
export function formatEfficiency(row: Pick<BlueprintLibraryRow, "activity">, value: number): string {
  return row.activity === "reaction" ? "—" : String(value);
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
