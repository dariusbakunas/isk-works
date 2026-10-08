import { dateKey, type CalendarTimezone } from "./calendar-dates";

export type CalendarTypeFilter = "all" | "industry" | "skill" | "planetary";
export type CalendarView = "month" | "week" | "year";

export interface CalendarUrlState {
  view: CalendarView;
  date: string;
  type: CalendarTypeFilter;
  characters: string[];
  timezone: CalendarTimezone;
}

const DATE_PATTERN = /^(\d{4})-(0[1-9]|1[0-2])-(0[1-9]|[12]\d|3[01])$/;
const UUID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

function parseDate(value: string | null): string | null {
  const match = value?.match(DATE_PATTERN);
  if (!match) return null;
  const candidate = new Date(Date.UTC(Number(match[1]), Number(match[2]) - 1, Number(match[3])));
  return dateKey(candidate, "eve") === value ? value : null;
}

export function parseCalendarUrlState(params: URLSearchParams, now = new Date()): CalendarUrlState {
  const type = params.get("type");
  const timezone: CalendarTimezone = params.get("tz") === "local" ? "local" : "eve";
  const requestedView = params.get("view");
  const view: CalendarView = requestedView === "week" || requestedView === "year" ? requestedView : "month";
  const characters = [...new Set(params.getAll("character").filter((id) => UUID_PATTERN.test(id)))];
  return {
    view,
    date: parseDate(params.get("date")) ?? dateKey(now, timezone),
    type: type === "industry" || type === "skill" || type === "planetary" ? type : "all",
    characters,
    timezone,
  };
}

export function serializeCalendarUrlState(state: CalendarUrlState): URLSearchParams {
  const params = new URLSearchParams();
  params.set("view", state.view);
  params.set("date", state.date);
  params.set("type", state.type);
  for (const connectionId of state.characters) params.append("character", connectionId);
  params.set("tz", state.timezone);
  return params;
}

export function resetCalendarFilters(state: CalendarUrlState): CalendarUrlState {
  return { ...state, type: "all", characters: [] };
}
