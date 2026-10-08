import { expect, test } from "vitest";
import { parseCalendarUrlState, resetCalendarFilters, serializeCalendarUrlState } from "../calendar-url-state";

const NOW = new Date("2026-09-27T12:00:00Z");
const FIRST = "0aa719e8-f94f-4ef1-8f47-cf649d3b2d95";
const SECOND = "874c445a-67f4-4368-9028-b785770a2e52";

test("uses the current EVE date and Month view defaults", () => {
  expect(parseCalendarUrlState(new URLSearchParams(), NOW)).toEqual({
    view: "month", date: "2026-09-27", type: "all", characters: [], timezone: "eve",
  });
});

test("uses the selected timezone when deriving a missing date", () => {
  const instant = new Date("2026-10-01T00:30:00Z");
  expect(parseCalendarUrlState(new URLSearchParams("tz=local"), instant).date).toBe("2026-09-30");
});

test("falls back independently for invalid views dates filters and identifiers", () => {
  for (const date of ["bad", "2026-02-30", "2026-13-01"]) {
    const params = new URLSearchParams(`view=century&date=${date}&type=other&character=nope&tz=mars`);
    expect(parseCalendarUrlState(params, NOW)).toEqual({
      view: "month", date: "2026-09-27", type: "all", characters: [], timezone: "eve",
    });
  }
});

test("round trips canonical view date and repeated character filters", () => {
  const params = new URLSearchParams();
  params.set("view", "week");
  params.set("date", "2026-10-02");
  params.set("type", "skill");
  params.append("character", FIRST);
  params.append("character", SECOND);
  params.append("character", FIRST);
  params.set("tz", "local");
  const state = parseCalendarUrlState(params, NOW);
  expect(state).toEqual({ view: "week", date: "2026-10-02", type: "skill", characters: [FIRST, SECOND], timezone: "local" });
  expect(parseCalendarUrlState(serializeCalendarUrlState(state), NOW)).toEqual(state);
  const serialized = serializeCalendarUrlState(state).toString();
  expect(serialized).toBe(`view=week&date=2026-10-02&type=skill&character=${FIRST}&character=${SECOND}&tz=local`);
  expect(serialized).not.toContain("month=");
});

test("filter reset preserves the view date and timezone", () => {
  expect(resetCalendarFilters({ view: "week", date: "2027-02-14", type: "industry", characters: [FIRST], timezone: "local" }))
    .toEqual({ view: "week", date: "2027-02-14", type: "all", characters: [], timezone: "local" });
});

test("round trips canonical Year state without adding another date parameter", () => {
  const state = parseCalendarUrlState(new URLSearchParams("view=year&date=2028-02-29&type=industry&tz=eve"), NOW);
  expect(state).toEqual({ view: "year", date: "2028-02-29", type: "industry", characters: [], timezone: "eve" });
  const serialized = serializeCalendarUrlState(state).toString();
  expect(serialized).toBe("view=year&date=2028-02-29&type=industry&tz=eve");
  expect(serialized).not.toContain("year=");
});

test("filter reset preserves Year date and timezone", () => {
  expect(resetCalendarFilters({ view: "year", date: "2028-02-29", type: "skill", characters: [FIRST], timezone: "local" }))
    .toEqual({ view: "year", date: "2028-02-29", type: "all", characters: [], timezone: "local" });
});

test("accepts the planetary type filter", () => {
  expect(parseCalendarUrlState(new URLSearchParams("type=planetary"), new Date("2026-10-02T12:00:00Z")).type).toBe("planetary");
});
