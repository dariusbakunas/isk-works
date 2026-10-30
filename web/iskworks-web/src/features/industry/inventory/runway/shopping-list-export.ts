import type { RunwayMaterial } from "../../../../api/inventory-analytics";
import { toCsv, type CsvValue } from "../../../../components/csv";

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

export function shoppingListCsv(materials: RunwayMaterial[]): string {
  const rows: CsvValue[][] = [
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
  return toCsv(rows);
}
