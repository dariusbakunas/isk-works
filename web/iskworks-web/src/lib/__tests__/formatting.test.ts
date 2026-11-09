import { describe, expect, it } from "vitest";
import {
  formatDuration,
  formatMoney,
  formatPercent,
  formatRelativeAge,
  signedMoney,
} from "../formatting";

describe("industry formatting", () => {
  it("preserves incomplete and unavailable labels", () => {
    expect(formatMoney(null)).toBe("Incomplete");
    expect(signedMoney(null)).toBe("Incomplete");
    expect(formatPercent(null)).toBe("Unavailable");
    expect(formatDuration(null)).toBe("Duration unavailable");
  });

  it("preserves money, percent, and duration formatting", () => {
    expect(formatMoney("1234567.89")).toBe("1,234,567.89 ISK");
    expect(signedMoney("1250")).toBe("+1,250 ISK");
    expect(formatPercent("11.4")).toBe("11.4%");
    expect(formatDuration(6250)).toBe("1h 44m");
  });

  it("trims the API's fixed-decimal percentage strings", () => {
    expect(formatPercent("1.000000")).toBe("1%");
    expect(formatPercent("30.000000")).toBe("30%");
    expect(formatPercent("2.530000")).toBe("2.5%");
    expect(formatPercent(0)).toBe("0%");
  });

  it("gives a coarse relative age for the Builds library updated line", () => {
    const daysAgo = (days: number) => new Date(Date.now() - days * 86_400_000).toISOString();
    expect(formatRelativeAge(daysAgo(0))).toBe("today");
    expect(formatRelativeAge(new Date(Date.now() + 60_000).toISOString())).toBe("today");
    expect(formatRelativeAge(daysAgo(1))).toBe("1d ago");
    expect(formatRelativeAge(daysAgo(4))).toBe("4d ago");
    expect(formatRelativeAge(daysAgo(15))).toBe("2w ago");
    expect(formatRelativeAge(daysAgo(90))).toBe("3mo ago");
    expect(formatRelativeAge(daysAgo(800))).toBe("2y ago");
  });
});
