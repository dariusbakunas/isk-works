import type { AnalyticsDelta, AnalyticsGranularity } from "../../../api/finance-analytics";

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

function parts(date: string): { year: number; month: number; day: number } {
  const [year, month, day] = date.split("-").map(Number);
  return { year, month: month - 1, day };
}

export function shortDate(date: string): string {
  const { month, day } = parts(date);
  return `${MONTHS[month]} ${day}`;
}

function addDays(date: string, days: number): string {
  const value = new Date(`${date}T00:00:00Z`);
  value.setUTCDate(value.getUTCDate() + days);
  return value.toISOString().slice(0, 10);
}

function endOfMonth(date: string): string {
  const { year, month } = parts(date);
  return new Date(Date.UTC(year, month + 1, 0)).toISOString().slice(0, 10);
}

/** Axis label for a bucket, by its start date. */
export function bucketLabel(start: string, granularity: AnalyticsGranularity): string {
  if (granularity === "month") return `${MONTHS[parts(start).month]} ${parts(start).year}`;
  return shortDate(start);
}

/** Tooltip range for a bucket, inclusive and clamped to the selected window. */
export function bucketRange(
  start: string,
  granularity: AnalyticsGranularity,
  windowFrom: string,
  windowTo: string,
): string {
  const rawEnd = granularity === "day" ? start : granularity === "week" ? addDays(start, 6) : endOfMonth(start);
  const from = start < windowFrom ? windowFrom : start;
  const to = rawEnd > windowTo ? windowTo : rawEnd;
  return from === to ? shortDate(from) : `${shortDate(from)} – ${shortDate(to)}`;
}

export function formatRangeText(dateFrom: string, dateTo: string): string {
  const from = parts(dateFrom);
  const to = parts(dateTo);
  if (from.year === to.year) return `${shortDate(dateFrom)} – ${shortDate(dateTo)}, ${to.year}`;
  return `${shortDate(dateFrom)}, ${from.year} – ${shortDate(dateTo)}, ${to.year}`;
}

/** Decimal string to a number for drawing only; never for accounting. */
export function numeric(value: string): number {
  const number = Number(value);
  return Number.isFinite(number) ? number : 0;
}

export type DeltaTone = "good" | "bad" | "neutral";

export interface DeltaPresentation {
  text: string;
  arrow: "up" | "down" | "flat";
  tone: DeltaTone;
}

/**
 * The arrow shows which way the number moved; the colour shows whether that is
 * good. Costs (`inverse`) are good when they fall, which the prototype got
 * wrong by colouring every rise green.
 */
export function deltaPresentation(
  delta: AnalyticsDelta | null,
  options: { inverse?: boolean } = {},
): DeltaPresentation | null {
  if (!delta) return null;
  if (delta.percent === null) {
    return { text: delta.isNew ? "new" : "—", arrow: "flat", tone: "neutral" };
  }
  const rounded = Math.round(delta.percent * 10) / 10;
  if (rounded === 0) return { text: "0.0%", arrow: "flat", tone: "neutral" };
  const rising = rounded > 0;
  const good = options.inverse ? !rising : rising;
  return {
    text: `${rising ? "+" : ""}${rounded.toFixed(1)}%`,
    arrow: rising ? "up" : "down",
    tone: good ? "good" : "bad",
  };
}

interface NamedTotal {
  category: string;
  total: string;
}

/** Category names by combined value across both sides, largest first. */
export function categoryOrder(spending: NamedTotal[], income: NamedTotal[]): string[] {
  const totals = new Map<string, number>();
  for (const { category, total } of [...spending, ...income]) {
    totals.set(category, (totals.get(category) ?? 0) + numeric(total));
  }
  return [...totals.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0])).map(([name]) => name);
}

/**
 * One colour per category name for the whole page, so a category looks the
 * same in every chart (the prototype coloured by position and did not).
 * "Other" is the folded tail and stays muted.
 */
export function assignCategoryColors(orderedNames: string[]): Map<string, string> {
  const colors = new Map<string, string>();
  let slot = 0;
  for (const name of orderedNames) {
    if (name === "Other") {
      colors.set(name, "var(--color-faint)");
    } else {
      colors.set(name, `var(--color-cat-${(slot % 8) + 1})`);
      slot += 1;
    }
  }
  return colors;
}

/** Change against the previous period; `null` when comparison is off. */
export function categoryDelta(total: string, previous: string | null): AnalyticsDelta | null {
  if (previous === null) return null;
  const before = numeric(previous);
  const now = numeric(total);
  if (before === 0) return { previous, percent: null, isNew: now !== 0 };
  return { previous, percent: ((now - before) / Math.abs(before)) * 100, isNew: false };
}

export function shareOfTotal(part: string, total: string): number | null {
  const whole = numeric(total);
  return whole === 0 ? null : (numeric(part) / whole) * 100;
}
