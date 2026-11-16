import { expect, test } from "vitest";
import { isValidTimeZone, timeZoneOptions, utcToWallClock, wallClockToUtc } from "../calendar-entry-time";

test("converts wall-clock time in a zone to UTC and back", () => {
  const instant = wallClockToUtc({ date: "2026-11-20", time: "18:00" }, "America/New_York");
  expect(instant?.toISOString()).toBe("2026-11-20T23:00:00.000Z");
  expect(utcToWallClock(instant!, "America/New_York")).toEqual({ date: "2026-11-20", time: "18:00" });
  expect(utcToWallClock(instant!, "UTC")).toEqual({ date: "2026-11-20", time: "23:00" });
  expect(wallClockToUtc({ date: "2026-11-21", time: "01:30" }, "Asia/Tokyo")?.toISOString()).toBe("2026-11-20T16:30:00.000Z");
  expect(wallClockToUtc({ date: "2026-11-20", time: "18:00" }, "UTC")?.toISOString()).toBe("2026-11-20T18:00:00.000Z");
});

test("follows each side of a daylight saving change", () => {
  // Vilnius: EEST (UTC+3) until 25 Oct 2026 04:00, then EET (UTC+2).
  expect(wallClockToUtc({ date: "2026-10-24", time: "12:00" }, "Europe/Vilnius")?.toISOString()).toBe("2026-10-24T09:00:00.000Z");
  expect(wallClockToUtc({ date: "2026-10-26", time: "12:00" }, "Europe/Vilnius")?.toISOString()).toBe("2026-10-26T10:00:00.000Z");
  // The repeated 03:30 takes its first (summer time) instant.
  expect(wallClockToUtc({ date: "2026-10-25", time: "03:30" }, "Europe/Vilnius")?.toISOString()).toBe("2026-10-25T00:30:00.000Z");
  // New York skips 02:00-03:00 on 8 Mar 2026; 02:30 lands after the jump.
  const skipped = wallClockToUtc({ date: "2026-03-08", time: "02:30" }, "America/New_York");
  expect(utcToWallClock(skipped!, "America/New_York").time).toBe("03:30");
});

test("refuses malformed dates and times", () => {
  for (const clock of [
    { date: "2026-02-30", time: "10:00" },
    { date: "2026-13-01", time: "10:00" },
    { date: "20261120", time: "10:00" },
    { date: "2026-11-20", time: "24:00" },
    { date: "2026-11-20", time: "" },
  ]) {
    expect(wallClockToUtc(clock, "UTC")).toBeNull();
  }
});

test("lists UTC and the browser zone first and validates zone names", () => {
  const options = timeZoneOptions("Europe/Vilnius");
  expect(options.slice(0, 2)).toEqual(["UTC", "Europe/Vilnius"]);
  expect(options).toContain("America/New_York");
  expect(new Set(options).size).toBe(options.length);
  expect(isValidTimeZone("Asia/Tokyo")).toBe(true);
  expect(isValidTimeZone("Mars/Olympus")).toBe(false);
  expect(isValidTimeZone("")).toBe(false);
});
