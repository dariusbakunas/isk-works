import type { MaterialCategory } from "../../../api/inventory-analytics";

// One colour per material category, shared by every Inventory chart and
// legend so a category reads the same everywhere. "Other" stays neutral.
const CATEGORY_COLORS: Record<MaterialCategory, string> = {
  "Moon Materials": "var(--color-cat-3)",
  Minerals: "var(--color-cat-1)",
  "Ice Products": "var(--color-cat-6)",
  "Reaction Intermediates": "var(--color-cat-4)",
  Planetary: "var(--color-cat-5)",
  "Fuel Blocks": "var(--color-cat-2)",
  Components: "var(--color-cat-8)",
  "Ships & Modules": "var(--color-cat-7)",
  Other: "var(--color-muted)",
};

export function categoryColor(category: MaterialCategory): string {
  return CATEGORY_COLORS[category];
}
