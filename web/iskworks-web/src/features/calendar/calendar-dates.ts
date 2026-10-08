export type CalendarTimezone = "eve" | "local";

export interface CalendarMonth {
  year: number;
  /** One-based Gregorian month. */
  month: number;
}

export interface CalendarDay {
  key: string;
  dayNumber: number;
  inMonth: boolean;
}

export interface CalendarRange {
  from: Date;
  to: Date;
}

type Occurring = { occursAt: string };

const twoDigits = (value: number) => String(value).padStart(2, "0");

const keyFromParts = (year: number, month: number, day: number) =>
  `${year}-${twoDigits(month)}-${twoDigits(day)}`;

function partsFromKey(key: string): [number, number, number] {
  const [year, month, day] = key.split("-").map(Number);
  return [year, month, day];
}

function utcDateFromKey(key: string): Date {
  const [year, month, day] = partsFromKey(key);
  return new Date(Date.UTC(year, month - 1, day));
}

export function currentMonth(now = new Date(), timezone: CalendarTimezone = "eve"): CalendarMonth {
  return timezone === "eve"
    ? { year: now.getUTCFullYear(), month: now.getUTCMonth() + 1 }
    : { year: now.getFullYear(), month: now.getMonth() + 1 };
}

export function monthGrid(month: CalendarMonth): CalendarDay[] {
  const first = new Date(Date.UTC(month.year, month.month - 1, 1));
  const mondayFirstOffset = (first.getUTCDay() + 6) % 7;
  const daysInMonth = new Date(Date.UTC(month.year, month.month, 0)).getUTCDate();
  const cellCount = Math.ceil((mondayFirstOffset + daysInMonth) / 7) * 7;
  const firstCell = new Date(Date.UTC(month.year, month.month - 1, 1 - mondayFirstOffset));

  return Array.from({ length: cellCount }, (_, index) => {
    const date = new Date(firstCell);
    date.setUTCDate(firstCell.getUTCDate() + index);
    const cellMonth = date.getUTCMonth() + 1;
    return {
      key: keyFromParts(date.getUTCFullYear(), cellMonth, date.getUTCDate()),
      dayNumber: date.getUTCDate(),
      inMonth: date.getUTCFullYear() === month.year && cellMonth === month.month,
    };
  });
}

export function monthRange(month: CalendarMonth, timezone: CalendarTimezone): CalendarRange {
  if (timezone === "eve") {
    return {
      from: new Date(Date.UTC(month.year, month.month - 1, 1)),
      to: new Date(Date.UTC(month.year, month.month, 1)),
    };
  }
  return {
    from: new Date(month.year, month.month - 1, 1),
    to: new Date(month.year, month.month, 1),
  };
}

export function monthFromDateKey(key: string): CalendarMonth {
  const [year, month] = partsFromKey(key);
  return { year, month };
}

export function yearMonths(anchor: string): CalendarMonth[] {
  const [year] = partsFromKey(anchor);
  return Array.from({ length: 12 }, (_, index) => ({ year, month: index + 1 }));
}

export function yearRange(anchor: string, timezone: CalendarTimezone): CalendarRange {
  const [year] = partsFromKey(anchor);
  return timezone === "eve"
    ? { from: new Date(Date.UTC(year, 0, 1)), to: new Date(Date.UTC(year + 1, 0, 1)) }
    : { from: new Date(year, 0, 1), to: new Date(year + 1, 0, 1) };
}

export function dateKeyForMonth(anchor: string, month: CalendarMonth): string {
  const [, , day] = partsFromKey(anchor);
  const lastDay = new Date(Date.UTC(month.year, month.month, 0)).getUTCDate();
  return keyFromParts(month.year, month.month, Math.min(day, lastDay));
}

export function weekDateKeys(anchor: string): string[] {
  const date = utcDateFromKey(anchor);
  const mondayOffset = (date.getUTCDay() + 6) % 7;
  date.setUTCDate(date.getUTCDate() - mondayOffset);
  return Array.from({ length: 7 }, (_, index) => {
    const day = new Date(date);
    day.setUTCDate(date.getUTCDate() + index);
    return keyFromParts(day.getUTCFullYear(), day.getUTCMonth() + 1, day.getUTCDate());
  });
}

export function weekRange(anchor: string, timezone: CalendarTimezone): CalendarRange {
  const dates = weekDateKeys(anchor);
  const [fromYear, fromMonth, fromDay] = partsFromKey(dates[0]);
  const after = utcDateFromKey(dates[6]);
  after.setUTCDate(after.getUTCDate() + 1);
  const toParts: [number, number, number] = [after.getUTCFullYear(), after.getUTCMonth() + 1, after.getUTCDate()];
  const construct = ([year, month, day]: [number, number, number]) => timezone === "eve"
    ? new Date(Date.UTC(year, month - 1, day))
    : new Date(year, month - 1, day);
  return { from: construct([fromYear, fromMonth, fromDay]), to: construct(toParts) };
}

export function shiftCalendarDate(anchor: string, view: "month" | "week" | "year", offset: -1 | 1): string {
  const [year, month, day] = partsFromKey(anchor);
  if (view === "week") {
    const shifted = new Date(Date.UTC(year, month - 1, day + (offset * 7)));
    return keyFromParts(shifted.getUTCFullYear(), shifted.getUTCMonth() + 1, shifted.getUTCDate());
  }
  if (view === "year") {
    return dateKeyForMonth(anchor, { year: year + offset, month });
  }
  const target = new Date(Date.UTC(year, month - 1 + offset, 1));
  const lastDay = new Date(Date.UTC(target.getUTCFullYear(), target.getUTCMonth() + 1, 0)).getUTCDate();
  return keyFromParts(target.getUTCFullYear(), target.getUTCMonth() + 1, Math.min(day, lastDay));
}

export function dateKey(instant: string | Date, timezone: CalendarTimezone): string {
  const date = instant instanceof Date ? instant : new Date(instant);
  return timezone === "eve"
    ? keyFromParts(date.getUTCFullYear(), date.getUTCMonth() + 1, date.getUTCDate())
    : keyFromParts(date.getFullYear(), date.getMonth() + 1, date.getDate());
}

export function groupByDate<T extends Occurring>(items: readonly T[], timezone: CalendarTimezone): Map<string, T[]> {
  const grouped = new Map<string, T[]>();
  for (const item of items) {
    const key = dateKey(item.occursAt, timezone);
    const existing = grouped.get(key);
    if (existing) existing.push(item);
    else grouped.set(key, [item]);
  }
  return grouped;
}

export function isToday(key: string, timezone: CalendarTimezone, now = new Date()): boolean {
  return key === dateKey(now, timezone);
}

export function formatMonthLabel(month: CalendarMonth): string {
  return new Intl.DateTimeFormat("en-US", {
    month: "long",
    year: "numeric",
    timeZone: "UTC",
  }).format(new Date(Date.UTC(month.year, month.month - 1, 1)));
}

export function formatWeekLabel(anchor: string): string {
  const dates = weekDateKeys(anchor);
  const first = utcDateFromKey(dates[0]);
  const last = utcDateFromKey(dates[6]);
  const shortMonth = (date: Date) => new Intl.DateTimeFormat("en-US", { month: "short", timeZone: "UTC" }).format(date);
  if (first.getUTCFullYear() !== last.getUTCFullYear()) {
    return `${shortMonth(first)} ${first.getUTCDate()}, ${first.getUTCFullYear()} – ${shortMonth(last)} ${last.getUTCDate()}, ${last.getUTCFullYear()}`;
  }
  if (first.getUTCMonth() === last.getUTCMonth()) {
    return `${shortMonth(first)} ${first.getUTCDate()}–${last.getUTCDate()}, ${last.getUTCFullYear()}`;
  }
  return `${shortMonth(first)} ${first.getUTCDate()} – ${shortMonth(last)} ${last.getUTCDate()}, ${last.getUTCFullYear()}`;
}

export function formatDateLabel(key: string): string {
  const [year, month, day] = key.split("-").map(Number);
  return new Intl.DateTimeFormat("en-US", {
    weekday: "long",
    month: "long",
    day: "numeric",
    year: "numeric",
    timeZone: "UTC",
  }).format(new Date(Date.UTC(year, month - 1, day)));
}

export function formatTime(instant: string | Date, timezone: CalendarTimezone): string {
  return new Intl.DateTimeFormat("en-US", {
    hour: "numeric",
    minute: "2-digit",
    timeZone: timezone === "eve" ? "UTC" : undefined,
    timeZoneName: timezone === "eve" ? "short" : undefined,
  }).format(instant instanceof Date ? instant : new Date(instant));
}
