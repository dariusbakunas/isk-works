import { describe, expect, test } from "vitest";

import {
  assignCategoryColors,
  bucketLabel,
  bucketRange,
  categoryDelta,
  categoryOrder,
  deltaPresentation,
  formatRangeText,
  numeric,
  shareOfTotal,
} from "../analytics-format";

describe("bucket labels", () => {
  test("label by start date", () => {
    expect(bucketLabel("2026-09-28", "week")).toBe("Sep 28");
    expect(bucketLabel("2026-09-01", "month")).toBe("Sep 2026");
    expect(bucketLabel("2026-09-05", "day")).toBe("Sep 5");
  });

  test("ranges are inclusive and clamped to the selected window", () => {
    expect(bucketRange("2026-09-28", "week", "2026-08-31", "2026-09-29")).toBe("Sep 28 – Sep 29");
    expect(bucketRange("2026-08-31", "week", "2026-09-02", "2026-09-29")).toBe("Sep 2 – Sep 6");
    expect(bucketRange("2026-09-01", "month", "2026-08-01", "2026-09-30")).toBe("Sep 1 – Sep 30");
    expect(bucketRange("2026-09-10", "day", "2026-09-01", "2026-09-30")).toBe("Sep 10");
  });
});

describe("delta presentation", () => {
  const delta = (percent: number | null, isNew = false) => ({ previous: "1", percent, isNew });

  test("arrow follows direction, colour follows good or bad", () => {
    expect(deltaPresentation(delta(12.34))).toMatchObject({ text: "+12.3%", arrow: "up", tone: "good" });
    expect(deltaPresentation(delta(-4.5))).toMatchObject({ text: "-4.5%", arrow: "down", tone: "bad" });
  });

  test("costs invert the colour but not the arrow", () => {
    expect(deltaPresentation(delta(8.1), { inverse: true })).toMatchObject({ arrow: "up", tone: "bad" });
    expect(deltaPresentation(delta(-8.1), { inverse: true })).toMatchObject({ arrow: "down", tone: "good" });
  });

  test("a zero baseline reads as new, and no change is neutral", () => {
    expect(deltaPresentation(delta(null, true))).toMatchObject({ text: "new", tone: "neutral" });
    expect(deltaPresentation(delta(null, false))).toMatchObject({ text: "—", tone: "neutral" });
    expect(deltaPresentation(delta(0))).toMatchObject({ text: "0.0%", arrow: "flat", tone: "neutral" });
    expect(deltaPresentation(null)).toBeNull();
  });
});

describe("range text and numbers", () => {
  test("formats ranges and parses decimals", () => {
    expect(formatRangeText("2026-09-01", "2026-09-07")).toBe("Sep 1 – Sep 7, 2026");
    expect(formatRangeText("2025-12-20", "2026-01-03")).toBe("Dec 20, 2025 – Jan 3, 2026");
    expect(numeric("1234.5000")).toBe(1234.5);
  });
});

describe("category helpers", () => {
  const row = (category: string, total: string) => ({ category, total, previous: null });

  test("orders categories by combined value across both sides", () => {
    const spending = [row("Ships", "100"), row("Modules", "300"), row("Other", "999")];
    const income = [row("Ships", "500"), row("Drones", "50")];
    expect(categoryOrder(spending, income)).toEqual(["Other", "Ships", "Modules", "Drones"]);
  });

  test("a category keeps one colour on both sides, Other is muted", () => {
    const colors = assignCategoryColors(["Ships", "Modules", "Other", "Drones"]);
    expect(colors.get("Ships")).toBe("var(--color-cat-1)");
    expect(colors.get("Modules")).toBe("var(--color-cat-2)");
    expect(colors.get("Drones")).toBe("var(--color-cat-3)");
    expect(colors.get("Other")).toBe("var(--color-faint)");
  });

  test("wraps the palette instead of running out", () => {
    const names = Array.from({ length: 10 }, (_, index) => `C${index}`);
    const colors = assignCategoryColors(names);
    expect(colors.get("C8")).toBe("var(--color-cat-1)");
    expect(colors.get("C9")).toBe("var(--color-cat-2)");
  });

  test("category deltas follow the same rules as KPI deltas", () => {
    expect(categoryDelta("150", "100")).toEqual({ previous: "100", percent: 50, isNew: false });
    expect(categoryDelta("50", "100")).toEqual({ previous: "100", percent: -50, isNew: false });
    expect(categoryDelta("5", "0")).toEqual({ previous: "0", percent: null, isNew: true });
    expect(categoryDelta("0", "0")).toEqual({ previous: "0", percent: null, isNew: false });
    expect(categoryDelta("5", null)).toBeNull();
  });

  test("share of a total, safe on zero", () => {
    expect(shareOfTotal("25", "200")).toBeCloseTo(12.5);
    expect(shareOfTotal("5", "0")).toBeNull();
  });
});
