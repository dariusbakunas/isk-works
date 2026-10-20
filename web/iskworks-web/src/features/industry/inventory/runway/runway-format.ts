import type { Tone } from "../../../../components/primitives";

const QUANTITY = new Intl.NumberFormat("en-US", { maximumFractionDigits: 0 });

export function formatQuantity(value: number): string {
  return QUANTITY.format(value);
}

export function formatPercent(value: number | null): string {
  if (value === null) return "—";
  // Floor so "99.6%" never reads as fully covered.
  return `${Math.floor(value)}%`;
}

/** Red under half, amber until fully covered, green when covered. */
export function coverageTone(percent: number | null): Tone {
  if (percent === null) return "muted";
  if (percent >= 100) return "positive";
  return percent >= 50 ? "warning" : "danger";
}

/** Exact sum of the API's 4-decimal ISK strings: money is never added as floats. */
export function sumIsk(values: string[]): string {
  let total = 0n;
  for (const value of values) {
    const negative = value.startsWith("-");
    const [whole, fraction = ""] = value.replace("-", "").split(".");
    const scaled = BigInt(whole || "0") * 10000n + BigInt(`${fraction}0000`.slice(0, 4));
    total += negative ? -scaled : scaled;
  }
  const magnitude = total < 0n ? -total : total;
  return `${total < 0n ? "-" : ""}${magnitude / 10000n}.${(magnitude % 10000n).toString().padStart(4, "0")}`;
}
