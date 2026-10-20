import { describe, expect, it } from "vitest";

import { bucketRange, coverTone, formatCover, historyWeeks, rangeFrom, utcToday } from "../consumption-format";

describe("rangeFrom", () => {
  it("counts the end day in the preset", () => {
    expect(rangeFrom("7d", "2026-10-20")).toBe("2026-10-14");
    expect(rangeFrom("90d", "2026-10-20")).toBe("2026-07-23");
    expect(rangeFrom("all", "2026-10-20")).toBeNull();
  });
});

describe("utcToday", () => {
  it("is the UTC calendar day", () => {
    expect(utcToday(new Date("2026-10-20T23:30:00-05:00"))).toBe("2026-10-21");
  });
});

describe("bucketRange", () => {
  it("labels a day, or a Monday-to-Sunday week", () => {
    expect(bucketRange("2026-10-13", "day")).toBe("Oct 13");
    expect(bucketRange("2026-10-26", "week")).toBe("Oct 26 – Nov 1");
  });
});

describe("historyWeeks", () => {
  it("counts whole weeks", () => {
    expect(historyWeeks("2026-09-09", "2026-10-20")).toBe(5);
    expect(historyWeeks("2026-10-14", "2026-10-20")).toBe(0);
  });
});

describe("cover", () => {
  it("is red under a week, amber under two", () => {
    expect([coverTone(3.3), coverTone(12), coverTone(21.7)]).toEqual(["danger", "warning", "positive"]);
    expect(formatCover(3.33)).toBe("3.3d");
    expect(formatCover(512.4)).toBe("512d");
  });
});
