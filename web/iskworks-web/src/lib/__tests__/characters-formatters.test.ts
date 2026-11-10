import { describe, expect, test } from "vitest";

import { formatRemainingMs, formatSpCompact } from "../characters-formatters";

describe("formatRemainingMs", () => {
  test.each([
    [17 * 60_000 + 6_000, "17m 6s"],
    [80 * 60_000, "1h 20m"],
    [(7 * 60 + 31) * 60 * 1000, "7h 31m"],
    [(24 + 19) * 60 * 60_000, "1d 19h"],
    [(11 * 24 + 11) * 60 * 60_000, "11d 11h"],
    [(559 * 24 + 22) * 60 * 60_000, "559d 22h"],
    [0, "0s"],
    [-5_000, "0s"],
    [45_000, "45s"],
  ])("%i ms -> %s", (ms, expected) => {
    expect(formatRemainingMs(ms)).toBe(expected);
  });
});

describe("formatSpCompact", () => {
  test.each([
    [94_300_000, "94.3m"],
    [26_454_179, "26.45m"],
    [5_000_000, "5m"],
    [512_000, "512k"],
    [1_234_567_890, "1.23b"],
    [850, "850"],
    [0, "0"],
  ])("%i -> %s", (sp, expected) => {
    expect(formatSpCompact(sp)).toBe(expected);
  });
});
