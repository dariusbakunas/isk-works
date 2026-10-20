import type { RunwayMaterial } from "../../../../api/inventory-analytics";

// The shopping list is built from the runway payload the page already has:
// every row is there, so no export endpoint is needed.

function shortOnly(materials: RunwayMaterial[]): RunwayMaterial[] {
  return materials.filter((material) => material.short > 0);
}

/** EVE's multibuy paste format: one `Name<TAB>quantity` line per item. */
export function shoppingListMultibuy(materials: RunwayMaterial[]): string {
  return shortOnly(materials)
    .map((material) => `${material.name}\t${material.short}`)
    .join("\n");
}

/** A leading `= + - @` (or tab/CR) makes a spreadsheet evaluate the cell, so
 * it is prefixed with `'`; quotes, commas and newlines are quoted. Same rule
 * as the API's `safe_cell`. */
function csvCell(value: string | number | null): string {
  if (value === null) return "";
  let text = String(value);
  if (typeof value === "string" && /^[=+\-@\t\r]/.test(text)) text = `'${text}`;
  return /[",\n\r]/.test(text) ? `"${text.replaceAll('"', '""')}"` : text;
}

export function shoppingListCsv(materials: RunwayMaterial[]): string {
  const rows = [
    ["Type ID", "Name", "Category", "Short", "Unit cost", "Short value"],
    ...shortOnly(materials).map((material) => [
      material.typeId,
      material.name,
      material.category,
      material.short,
      material.unitCost,
      material.shortValue,
    ]),
  ];
  return rows.map((row) => row.map(csvCell).join(",")).join("\n") + "\n";
}

/** Saves text through a throwaway link. */
export function downloadText(text: string, filename: string, type: string): void {
  const url = URL.createObjectURL(new Blob([text], { type }));
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  URL.revokeObjectURL(url);
}
