import { describe, expect, test } from "vitest";

import { heatmapCellStyle, layoutHeatmap } from "../heatmap-layout";

function days(from: string, count: number, net = (index: number) => String(index)) {
  return Array.from({ length: count }, (_, index) => {
    const date = new Date(`${from}T00:00:00Z`);
    date.setUTCDate(date.getUTCDate() + index);
    return { date: date.toISOString().slice(0, 10), net: net(index) };
  });
}

describe("layoutHeatmap", () => {
  test("aligns rows to the real weekday, Monday first", () => {
    // 2026-09-29 is a Tuesday: the first week starts one blank cell down.
    const layout = layoutHeatmap(days("2026-09-29", 3));
    expect(layout.weeks[0].map((cell) => cell?.date ?? null)).toEqual([
      null, "2026-09-29", "2026-09-30", "2026-10-01", null, null, null,
    ]);
    // A Monday start has no leading blanks.
    expect(layoutHeatmap(days("2026-09-28", 1)).weeks[0][0]?.date).toBe("2026-09-28");
  });

  test("91 days ending on a weekday span 13 or 14 whole columns of 7", () => {
    const layout = layoutHeatmap(days("2026-06-30", 91));
    expect(layout.weeks.every((week) => week.length === 7)).toBe(true);
    expect(layout.weeks.length).toBeGreaterThanOrEqual(13);
    expect(layout.weeks.length).toBeLessThanOrEqual(14);
    expect(layout.weeks.flat().filter(Boolean)).toHaveLength(91);
  });

  test("labels a column with its month where the month begins", () => {
    const layout = layoutHeatmap(days("2026-08-31", 40));
    const labels = layout.monthLabels.filter(Boolean);
    expect(labels[0]).toBe("Aug");
    expect(labels).toContain("Sep");
    expect(labels).toContain("Oct");
    expect(new Set(labels).size).toBe(labels.length);
  });

  test("never repeats a month label when the first column and the next both fall early in a month", () => {
    // Jul 1 is a Wednesday, so the first column starts in July and the next Monday (Jul 6) is also early July.
    const labels = layoutHeatmap(days("2026-07-01", 91)).monthLabels.filter(Boolean);
    expect(labels).toEqual(["Jul", "Aug", "Sep"]);
  });

  test("scales colour by the largest magnitude in the window, either sign", () => {
    const layout = layoutHeatmap(days("2026-09-01", 3, (index) => ["-200", "100", "0"][index]));
    expect(layout.maxAbs).toBe(200);
  });
});

describe("heatmapCellStyle", () => {
  test("green for gains, red for losses, stronger with magnitude", () => {
    const small = heatmapCellStyle(10, 100);
    const large = heatmapCellStyle(100, 100);
    expect(small.color).toBe("income");
    expect(heatmapCellStyle(-10, 100).color).toBe("expense");
    expect(large.alpha).toBeGreaterThan(small.alpha);
    expect(large.alpha).toBeLessThanOrEqual(0.97);
    expect(small.alpha).toBeGreaterThanOrEqual(0.15);
  });

  test("a day with no net is neutral, not a faint green", () => {
    expect(heatmapCellStyle(0, 100).color).toBe("none");
    expect(heatmapCellStyle(5, 0).color).toBe("none");
  });
});
