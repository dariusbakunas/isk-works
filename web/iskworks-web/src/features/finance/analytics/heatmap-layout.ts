import type { DayNet } from "../../../api/finance-analytics";
import { numeric } from "./analytics-format";

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

export interface HeatmapCell {
  date: string;
  net: number;
  /** The exact decimal from the server, for tooltips. */
  raw: string;
}

export interface HeatmapLayout {
  /** Columns of seven, Monday to Sunday; `null` pads days outside the window. */
  weeks: Array<Array<HeatmapCell | null>>;
  /** A month name over the column where that month starts. */
  monthLabels: Array<string | null>;
  maxAbs: number;
}

/** Monday = 0 .. Sunday = 6. */
function weekdayIndex(date: string): number {
  return (new Date(`${date}T00:00:00Z`).getUTCDay() + 6) % 7;
}

/** GitHub-style grid: real weekdays down the side, weeks across. */
export function layoutHeatmap(days: DayNet[]): HeatmapLayout {
  const cells: Array<HeatmapCell | null> = [];
  if (days.length > 0) {
    for (let pad = weekdayIndex(days[0].date); pad > 0; pad -= 1) cells.push(null);
  }
  for (const day of days) cells.push({ date: day.date, net: numeric(day.net), raw: day.net });
  while (cells.length % 7 !== 0) cells.push(null);

  const weeks: Array<Array<HeatmapCell | null>> = [];
  for (let index = 0; index < cells.length; index += 7) weeks.push(cells.slice(index, index + 7));

  let lastLabel: string | null = null;
  const monthLabels = weeks.map((week, column) => {
    const first = week.find((cell): cell is HeatmapCell => cell !== null);
    if (!first) return null;
    const dayOfMonth = Number(first.date.slice(8, 10));
    if (column !== 0 && dayOfMonth > 7) return null;
    const label = MONTHS[Number(first.date.slice(5, 7)) - 1];
    if (label === lastLabel) return null;
    lastLabel = label;
    return label;
  });
  const maxAbs = days.reduce((max, day) => Math.max(max, Math.abs(numeric(day.net))), 0);
  return { weeks, monthLabels, maxAbs };
}

export interface CellStyle {
  color: "income" | "expense" | "none";
  alpha: number;
}

/**
 * One magnitude scale for both signs. A day with nothing traded is neutral
 * rather than a faint green, so gaps do not read as small wins.
 */
export function heatmapCellStyle(net: number, maxAbs: number): CellStyle {
  if (net === 0 || maxAbs === 0) return { color: "none", alpha: 0 };
  return { color: net > 0 ? "income" : "expense", alpha: 0.15 + (Math.abs(net) / maxAbs) * 0.82 };
}
