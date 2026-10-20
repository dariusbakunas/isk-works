import type { ConsumptionGranularity } from "../../../../api/inventory-analytics";
import type { Tone } from "../../../../components/primitives";

/** Mirrors the API: burn rate and days of cover need this much history per material. */
export const BURN_RATE_HISTORY_WEEKS = 4;

export type ConsumptionRange = "7d" | "30d" | "90d" | "all";

export const CONSUMPTION_RANGES: Array<{ id: ConsumptionRange; label: string; days: number | null }> = [
  { id: "7d", label: "7d", days: 7 },
  { id: "30d", label: "30d", days: 30 },
  { id: "90d", label: "90d", days: 90 },
  { id: "all", label: "All", days: null },
];

const DAY_MS = 86_400_000;

/** Today as the API sees it: a UTC calendar day. */
export function utcToday(now: Date = new Date()): string {
  return now.toISOString().slice(0, 10);
}

/** `from` for a preset ending `to` (inclusive); `null` for everything. */
export function rangeFrom(range: ConsumptionRange, to: string): string | null {
  const days = CONSUMPTION_RANGES.find((option) => option.id === range)?.days ?? null;
  if (days === null) return null;
  return new Date(Date.parse(`${to}T00:00:00Z`) - (days - 1) * DAY_MS).toISOString().slice(0, 10);
}

const SHORT_DATE = new Intl.DateTimeFormat("en-US", { month: "short", day: "numeric", timeZone: "UTC" });
const LONG_DATE = new Intl.DateTimeFormat("en-GB", { day: "numeric", month: "short", year: "numeric", timeZone: "UTC" });

export function shortDate(day: string): string {
  return SHORT_DATE.format(new Date(`${day}T00:00:00Z`));
}

export function longDate(day: string): string {
  return LONG_DATE.format(new Date(`${day}T00:00:00Z`));
}

/** "Oct 13" for a day; "Oct 13 – Oct 19" for a week. */
export function bucketRange(start: string, granularity: ConsumptionGranularity): string {
  if (granularity === "day") return shortDate(start);
  const end = new Date(Date.parse(`${start}T00:00:00Z`) + 6 * DAY_MS).toISOString().slice(0, 10);
  return `${shortDate(start)} – ${shortDate(end)}`;
}

/** Whole weeks between the first recorded consumption and `to`. */
export function historyWeeks(earliest: string, to: string): number {
  return Math.floor((Date.parse(`${to}T00:00:00Z`) - Date.parse(`${earliest}T00:00:00Z`)) / (7 * DAY_MS));
}

/** Red under a week, amber under two, green otherwise. */
export function coverTone(days: number): Tone {
  if (days < 7) return "danger";
  return days < 14 ? "warning" : "positive";
}

export function formatCover(days: number): string {
  return days >= 100 ? `${Math.round(days)}d` : `${days.toFixed(1)}d`;
}
