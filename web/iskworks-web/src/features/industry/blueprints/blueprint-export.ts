import type { BlueprintLibraryRow } from "../../../api/industry";
import { toCsv, type CsvValue } from "../../../components/csv";
import { hasUnlimitedRuns, kindLabel } from "./blueprint-format";

// Blueprints › Export: a CSV of every row matching the current view, built on
// the client from the library endpoint's own rows (no export endpoint).

export type BlueprintExportColumnKey =
  | "blueprint"
  | "blueprintTypeId"
  | "product"
  | "productTypeId"
  | "productGroup"
  | "productCategory"
  | "kind"
  | "activity"
  | "me"
  | "te"
  | "runs"
  | "character"
  | "location"
  | "locationId"
  | "locationFlag"
  | "itemId"
  | "usedByBuilds"
  | "observed";

export interface BlueprintExportColumn {
  key: BlueprintExportColumnKey;
  label: string;
  value: (row: BlueprintLibraryRow) => CsvValue;
}

/** ME / TE do not apply to reactions, so their cells stay empty. */
function efficiency(row: BlueprintLibraryRow, value: number): CsvValue {
  return row.activity === "reaction" ? null : value;
}

export const BLUEPRINT_EXPORT_COLUMNS: BlueprintExportColumn[] = [
  { key: "blueprint", label: "Blueprint", value: (row) => row.blueprintName },
  { key: "blueprintTypeId", label: "Blueprint type ID", value: (row) => row.blueprintTypeId },
  { key: "product", label: "Product", value: (row) => row.productName },
  { key: "productTypeId", label: "Product type ID", value: (row) => row.productTypeId },
  { key: "productGroup", label: "Product group", value: (row) => row.productGroupName },
  { key: "productCategory", label: "Product category", value: (row) => row.productCategoryName },
  { key: "kind", label: "Kind", value: (row) => (row.activity === "reaction" ? "Reaction formula" : kindLabel(row.kind)) },
  { key: "activity", label: "Activity", value: (row) => (row.activity === "reaction" ? "Reaction" : "Manufacturing") },
  { key: "me", label: "ME", value: (row) => efficiency(row, row.materialEfficiency) },
  { key: "te", label: "TE", value: (row) => efficiency(row, row.timeEfficiency) },
  { key: "runs", label: "Runs", value: (row) => (hasUnlimitedRuns(row) ? "Unlimited" : row.licensedRuns ?? 0) },
  { key: "character", label: "Character", value: (row) => row.characterName },
  { key: "location", label: "Location", value: (row) => row.locationName ?? `Location ${row.locationId}` },
  { key: "locationId", label: "Location ID", value: (row) => row.locationId },
  { key: "locationFlag", label: "Location flag", value: (row) => row.locationFlag },
  { key: "itemId", label: "Item ID", value: (row) => row.eveItemId },
  { key: "usedByBuilds", label: "Used by builds", value: (row) => row.usedByBuilds.map((build) => build.name).join("; ") || null },
  { key: "observed", label: "Observed", value: (row) => row.observedAt },
];

/** The table's own columns. */
export const DEFAULT_EXPORT_COLUMNS: BlueprintExportColumnKey[] = ["blueprint", "product", "kind", "me", "te", "runs", "character", "location"];

/** Columns come out in `BLUEPRINT_EXPORT_COLUMNS` order, whatever order they were picked in. */
export function blueprintLibraryCsv(rows: BlueprintLibraryRow[], keys: readonly BlueprintExportColumnKey[]): string {
  const columns = BLUEPRINT_EXPORT_COLUMNS.filter((column) => keys.includes(column.key));
  return toCsv([
    columns.map((column) => column.label),
    ...rows.map((row) => columns.map((column) => column.value(row))),
  ]);
}

/** Where the Export dialog remembers this browser's last picks. */
export const EXPORT_COLUMNS_STORAGE_KEY = "iskworks.blueprints.exportColumns";
