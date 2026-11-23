import type { InventionGap, InventionOption, InventionQuote } from "../../../api/invention";
import { formatIskCompact } from "../../../components/money";

/** The row key of an option: its decryptor's type id, or `"none"`. */
export function optionKey(option: InventionOption): string {
  return option.decryptor ? String(option.decryptor.typeId) : "none";
}

export function optionName(option: InventionOption): string {
  return option.decryptor?.name ?? "No decryptor";
}

/** A chance fraction as the game shows it: a percentage truncated, not
 * rounded, to one decimal (0.4375 reads "43.7%"). */
export function formatChance(chance: string): string {
  const tenths = Math.floor(Number(chance) * 1000 + 1e-6);
  return `${(tenths / 10).toFixed(1)}%`;
}

/** A signed compact ISK amount: "+71K", "−12K", "0". */
export function formatIskDelta(value: string): string {
  const amount = Number(value);
  if (!Number.isFinite(amount) || amount === 0) return "0";
  const magnitude = formatIskCompact(value.replace(/^-/, ""));
  return `${amount > 0 ? "+" : "−"}${magnitude}`;
}

export function formatPercentDelta(value: string): string {
  const amount = Number(value);
  if (!Number.isFinite(amount)) return "";
  return `${amount > 0 ? "+" : amount < 0 ? "−" : ""}${Math.abs(amount).toFixed(1)}%`;
}

export type CopyTone = "better" | "worse" | "same";

/** How a copy's figure compares with the no-decryptor copy; more is
 * better for ME, TE and runs alike. */
export function copyTone(value: number, baseline: number): CopyTone {
  return value > baseline ? "better" : value < baseline ? "worse" : "same";
}

export type InventionSort = "decryptor" | "chance" | "attempt" | "perCopy" | "perRun" | "unitCost" | "attempts";
export type SortOrder = "asc" | "desc";

function sortValue(option: InventionOption, sort: InventionSort): number | string | null {
  const money = (value: string | null) => (value === null ? null : Number(value));
  switch (sort) {
    case "decryptor": return optionName(option).toLowerCase();
    case "chance": return Number(option.chance);
    case "attempt": return money(option.attemptCost);
    case "perCopy": return money(option.costPerCopy);
    case "perRun": return money(option.costPerRun);
    case "unitCost": return money(option.unitCost);
    case "attempts": return option.attempts?.expected ?? null;
  }
}

/** Options in the asked-for order. A row with nothing to sort on goes last
 * in either direction, and ties keep the server's order. */
export function sortOptions(options: InventionOption[], sort: InventionSort, order: SortOrder): InventionOption[] {
  const direction = order === "asc" ? 1 : -1;
  return options
    .map((option, index) => ({ option, index, value: sortValue(option, sort) }))
    .sort((left, right) => {
      if (left.value === null || right.value === null) {
        if (left.value === right.value) return left.index - right.index;
        return left.value === null ? 1 : -1;
      }
      if (left.value < right.value) return -direction;
      if (left.value > right.value) return direction;
      return left.index - right.index;
    })
    .map((entry) => entry.option);
}

/** What an incomplete option is missing, for its chip and tooltip. */
export function gapLabel(gap: InventionGap, quote: InventionQuote): string {
  switch (gap.kind) {
    case "materialPrice": {
      const material = quote.materials.find((entry) => entry.typeId === gap.typeId);
      return `No price for ${material?.typeName ?? "a datacore"}`;
    }
    case "decryptorPrice": return "No decryptor price";
    case "jobFee": return "Invention job fee unknown";
    case "copyCost": return "Copy cost unknown";
    case "manufacturingCost": return "Manufacturing cost unknown";
  }
}

export interface ShoppingLine {
  name: string;
  quantity: number;
  /** False for copy runs, which are made and not bought. */
  marketItem: boolean;
}

/** What to have on hand for the confident number of attempts. */
export function shoppingLines(quote: InventionQuote, option: InventionOption): ShoppingLine[] {
  const attempts = option.attempts?.confident ?? 0;
  if (attempts === 0) return [];
  const lines: ShoppingLine[] = quote.materials.map((material) => ({
    name: material.typeName,
    quantity: material.quantity * attempts,
    marketItem: true,
  }));
  if (option.decryptor) lines.push({ name: option.decryptor.name, quantity: attempts, marketItem: true });
  lines.push({ name: "T1 copy runs", quantity: attempts, marketItem: false });
  return lines;
}

/** The market items as "name quantity" lines, the shape EVE's multibuy
 * accepts. */
export function shoppingListText(quote: InventionQuote, option: InventionOption): string {
  return shoppingLines(quote, option)
    .filter((line) => line.marketItem)
    .map((line) => `${line.name} ${line.quantity}`)
    .join("\n");
}
