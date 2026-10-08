import { expect, test } from "vitest";
import type { CalendarMilestone } from "../../../api/calendar";
import {
  currentMonth,
  dateKeyForMonth,
  dateKey,
  groupByDate,
  formatWeekLabel,
  isToday,
  monthFromDateKey,
  monthGrid,
  monthRange,
  shiftCalendarDate,
  weekDateKeys,
  weekRange,
  yearMonths,
  yearRange,
} from "../calendar-dates";

test("builds complete Monday-first five and six row month grids", () => {
  const february = monthGrid({ year: 2026, month: 2 });
  expect(february).toHaveLength(35);
  expect(february[0]).toMatchObject({ key: "2026-01-26", inMonth: false });
  expect(february[6]).toMatchObject({ key: "2026-02-01", inMonth: true });

  const march = monthGrid({ year: 2026, month: 3 });
  expect(march).toHaveLength(42);
  expect(march[0].key).toBe("2026-02-23");
  expect(march.at(-1)?.key).toBe("2026-04-05");
});

test("includes leap day in February", () => {
  const grid = monthGrid({ year: 2024, month: 2 });
  expect(grid.filter((day) => day.inMonth)).toHaveLength(29);
  expect(grid.some((day) => day.key === "2024-02-29")).toBe(true);
});

test("groups one absolute instant on different dates in EVE and local time", () => {
  const crossing = { id: "crossing", occursAt: "2026-10-01T00:30:00Z" } as CalendarMilestone;
  expect(dateKey(crossing.occursAt, "eve")).toBe("2026-10-01");
  expect(dateKey(crossing.occursAt, "local")).toBe("2026-09-30");
  expect([...groupByDate([crossing], "local").keys()]).toEqual(["2026-09-30"]);
});

test("computes EVE and local month boundaries as absolute instants", () => {
  expect(monthRange({ year: 2026, month: 10 }, "eve")).toEqual({
    from: new Date("2026-10-01T00:00:00.000Z"),
    to: new Date("2026-11-01T00:00:00.000Z"),
  });
  expect(monthRange({ year: 2026, month: 10 }, "local")).toEqual({
    from: new Date("2026-10-01T04:00:00.000Z"),
    to: new Date("2026-11-01T04:00:00.000Z"),
  });
});

test("current month and Today follow the selected timezone", () => {
  const instant = new Date("2026-10-01T00:30:00Z");
  expect(currentMonth(instant, "eve")).toEqual({ year: 2026, month: 10 });
  expect(currentMonth(instant, "local")).toEqual({ year: 2026, month: 9 });
  expect(isToday("2026-10-01", "eve", instant)).toBe(true);
  expect(isToday("2026-09-30", "local", instant)).toBe(true);
});

test("local month ranges preserve civil midnights across DST transitions", () => {
  const spring = monthRange({ year: 2026, month: 3 }, "local");
  const fall = monthRange({ year: 2026, month: 11 }, "local");
  expect((spring.to.getTime() - spring.from.getTime()) / 3_600_000).toBe(743);
  expect((fall.to.getTime() - fall.from.getTime()) / 3_600_000).toBe(721);
  expect(spring.from.toISOString()).toBe("2026-03-01T05:00:00.000Z");
  expect(spring.to.toISOString()).toBe("2026-04-01T04:00:00.000Z");
  expect(fall.from.toISOString()).toBe("2026-11-01T04:00:00.000Z");
  expect(fall.to.toISOString()).toBe("2026-12-01T05:00:00.000Z");
});

test("derives a Monday-first cross-month week from its date anchor", () => {
  expect(monthFromDateKey("2026-10-02")).toEqual({ year: 2026, month: 10 });
  expect(weekDateKeys("2026-10-02")).toEqual([
    "2026-09-28", "2026-09-29", "2026-09-30", "2026-10-01",
    "2026-10-02", "2026-10-03", "2026-10-04",
  ]);
  expect(weekRange("2026-10-02", "eve")).toEqual({
    from: new Date("2026-09-28T00:00:00Z"),
    to: new Date("2026-10-05T00:00:00Z"),
  });
});

test("shifts week anchors and clamps month anchors to a valid day", () => {
  expect(shiftCalendarDate("2026-03-31", "month", -1)).toBe("2026-02-28");
  expect(shiftCalendarDate("2024-03-31", "month", -1)).toBe("2024-02-29");
  expect(shiftCalendarDate("2026-10-02", "week", 1)).toBe("2026-10-09");
});

test("formats week labels across month and year boundaries", () => {
  expect(formatWeekLabel("2026-10-02")).toBe("Sep 28 – Oct 4, 2026");
  expect(formatWeekLabel("2027-01-01")).toBe("Dec 28, 2026 – Jan 3, 2027");
});

test("local week ranges preserve civil midnights across DST transitions", () => {
  const range = weekRange("2026-11-01", "local");
  expect(range.from.toISOString()).toBe("2026-10-26T04:00:00.000Z");
  expect(range.to.toISOString()).toBe("2026-11-02T05:00:00.000Z");
  expect((range.to.getTime() - range.from.getTime()) / 3_600_000).toBe(169);
});

test("builds all twelve months for the anchor year", () => {
  expect(yearMonths("2028-02-29")).toEqual(Array.from({ length: 12 }, (_, index) => ({ year: 2028, month: index + 1 })));
});

test("builds EVE and Local half-open year ranges", () => {
  expect(yearRange("2028-02-29", "eve")).toEqual({
    from: new Date("2028-01-01T00:00:00.000Z"),
    to: new Date("2029-01-01T00:00:00.000Z"),
  });
  expect(yearRange("2028-02-29", "local")).toEqual({
    from: new Date("2028-01-01T05:00:00.000Z"),
    to: new Date("2029-01-01T05:00:00.000Z"),
  });
});

test("shifts Year anchors and clamps leap day", () => {
  expect(shiftCalendarDate("2028-02-29", "year", -1)).toBe("2027-02-28");
  expect(shiftCalendarDate("2027-02-28", "year", 1)).toBe("2028-02-28");
});

test("selects a month while preserving and clamping the anchor day", () => {
  expect(dateKeyForMonth("2027-01-31", { year: 2027, month: 2 })).toBe("2027-02-28");
  expect(dateKeyForMonth("2028-01-31", { year: 2028, month: 2 })).toBe("2028-02-29");
  expect(dateKeyForMonth("2028-10-15", { year: 2028, month: 2 })).toBe("2028-02-15");
});
