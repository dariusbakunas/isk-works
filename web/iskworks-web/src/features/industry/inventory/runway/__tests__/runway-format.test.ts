import { describe, expect, it } from "vitest";

import { coverageTone, formatPercent, sumIsk } from "../runway-format";

describe("sumIsk", () => {
  it("adds 4-decimal ISK strings exactly", () => {
    expect(sumIsk(["0.1000", "0.2000"])).toBe("0.3000");
    expect(sumIsk(["229771230.0000", "1190000000.5", "-0.0001"])).toBe("1419771230.4999");
    expect(sumIsk([])).toBe("0.0000");
  });
});

describe("formatPercent", () => {
  it("floors so an almost-covered row never reads as 100%", () => {
    expect(formatPercent(99.6)).toBe("99%");
    expect(formatPercent(null)).toBe("—");
  });
});

describe("coverageTone", () => {
  it("is red under half, amber until covered, green when covered", () => {
    expect([coverageTone(10), coverageTone(50), coverageTone(100), coverageTone(null)]).toEqual([
      "danger",
      "warning",
      "positive",
      "muted",
    ]);
  });
});
