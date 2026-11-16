/**
 * Wall-clock time in an IANA time zone, for custom Calendar entries: the
 * form edits a date and time in the entry's zone, the API stores the UTC
 * instant. Conversions go through `Intl`, so they follow each zone's DST
 * rules.
 */

/** EVE time is UTC. */
export const EVE_TIME_ZONE = "UTC";

export interface WallClock {
  /** `YYYY-MM-DD`. */
  date: string;
  /** `HH:MM`, 24-hour. */
  time: string;
}

export function browserTimeZone(): string {
  return Intl.DateTimeFormat().resolvedOptions().timeZone || EVE_TIME_ZONE;
}

export function isValidTimeZone(zone: string): boolean {
  if (!zone.trim()) return false;
  try {
    new Intl.DateTimeFormat("en-US", { timeZone: zone });
    return true;
  } catch {
    return false;
  }
}

/** UTC and the browser's zone first, then every zone the browser knows. */
export function timeZoneOptions(browserZone = browserTimeZone()): string[] {
  const known = typeof Intl.supportedValuesOf === "function" ? Intl.supportedValuesOf("timeZone") : [];
  return [...new Set([EVE_TIME_ZONE, browserZone, ...known])];
}

const pad = (value: number) => String(value).padStart(2, "0");

function zonedParts(instant: Date, zone: string) {
  const parts = new Intl.DateTimeFormat("en-US", {
    timeZone: zone,
    hourCycle: "h23",
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  }).formatToParts(instant);
  const value = (type: Intl.DateTimeFormatPartTypes) => Number(parts.find((part) => part.type === type)?.value);
  return {
    year: value("year"),
    month: value("month"),
    day: value("day"),
    hour: value("hour"),
    minute: value("minute"),
    second: value("second"),
  };
}

/** How far `zone` is ahead of UTC at `instant`, in milliseconds. */
function zoneOffset(instant: Date, zone: string): number {
  const parts = zonedParts(instant, zone);
  const asUtc = Date.UTC(parts.year, parts.month - 1, parts.day, parts.hour, parts.minute, parts.second);
  return asUtc - Math.floor(instant.getTime() / 1000) * 1000;
}

export function utcToWallClock(instant: string | Date, zone: string): WallClock {
  const parts = zonedParts(instant instanceof Date ? instant : new Date(instant), zone);
  return {
    date: `${parts.year}-${pad(parts.month)}-${pad(parts.day)}`,
    time: `${pad(parts.hour)}:${pad(parts.minute)}`,
  };
}

/**
 * The instant a wall-clock time in `zone` names. A time skipped by a DST
 * jump lands just after the jump; a repeated one takes its first instant.
 * `null` when the date or time is malformed.
 */
export function wallClockToUtc({ date, time }: WallClock, zone: string): Date | null {
  const dateMatch = /^(\d{4})-(\d{2})-(\d{2})$/.exec(date);
  const timeMatch = /^(\d{2}):(\d{2})$/.exec(time);
  if (!dateMatch || !timeMatch) return null;
  const [year, month, day] = dateMatch.slice(1).map(Number);
  const [hour, minute] = timeMatch.slice(1).map(Number);
  if (month < 1 || month > 12 || day < 1 || day > 31 || hour > 23 || minute > 59) return null;
  const asUtc = Date.UTC(year, month - 1, day, hour, minute);
  if (new Date(asUtc).getUTCDate() !== day) return null;
  // A zone changes offset at most once around a given day: try the offset
  // in force a day before and a day after. Of the candidates that really
  // read as this wall-clock time, the earlier is the first occurrence of a
  // repeated hour. When none does, the time was skipped: applying the
  // earlier offset lands just after the jump.
  const DAY = 86_400_000;
  const before = asUtc - zoneOffset(new Date(asUtc - DAY), zone);
  const after = asUtc - zoneOffset(new Date(asUtc + DAY), zone);
  const target = `${date} ${time}`;
  const matching = [before, after].filter((candidate) => {
    const clock = utcToWallClock(new Date(candidate), zone);
    return `${clock.date} ${clock.time}` === target;
  });
  return new Date(matching.length ? Math.min(...matching) : before);
}
