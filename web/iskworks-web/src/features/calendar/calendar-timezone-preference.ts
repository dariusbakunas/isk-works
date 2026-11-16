import type { CalendarTimezone } from "./calendar-dates";

const TIMEZONE_KEY = "iskworks.calendar.timezone";

/** The timezone last picked in this browser; EVE time when none is stored. */
export function readCalendarTimezone(): CalendarTimezone {
  try {
    return window.localStorage.getItem(TIMEZONE_KEY) === "local" ? "local" : "eve";
  } catch {
    return "eve";
  }
}

export function writeCalendarTimezone(timezone: CalendarTimezone) {
  try {
    window.localStorage.setItem(TIMEZONE_KEY, timezone);
  } catch {
    // Without storage the calendar falls back to EVE time on the next visit.
  }
}
