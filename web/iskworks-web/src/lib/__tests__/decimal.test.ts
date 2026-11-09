import { describe, expect, test } from "vitest";

import { effectiveUnitCost, sumDecimalStrings } from "../decimal";

describe("sumDecimalStrings", () => {
  test("adds values exactly across different scales", () => {
    expect(sumDecimalStrings(["0.01", "1.2300", "1000000000000000.9"])).toEqual({
      value: "1000000000000002.1400",
      partial: false,
    });
  });

  test("sums known values and marks missing values partial", () => {
    expect(sumDecimalStrings(["12.50", null, "0.25"])).toEqual({
      value: "12.75",
      partial: true,
    });
  });

  test("returns no value when every input is missing", () => {
    expect(sumDecimalStrings([null, undefined])).toEqual({ value: null, partial: true });
  });
});

describe("effectiveUnitCost", () => {
  test("divides exactly when the quotient terminates", () => {
    expect(effectiveUnitCost("6800.0000", 1_000)).toBe("6.8000");
  });

  test("divides a fraction less than one exactly", () => {
    expect(effectiveUnitCost("5.0000", 2_000)).toBe("0.0025");
  });

  test("rounds the repeating-decimal remainder half up", () => {
    // 10 / 6 = 1.6666... -> 5th digit is 6, rounds the 4th digit up.
    expect(effectiveUnitCost("10.0000", 6)).toBe("1.6667");
    // 10 / 3 = 3.3333... -> 5th digit is 3, stays down.
    expect(effectiveUnitCost("10.0000", 3)).toBe("3.3333");
  });

  test("never divides by zero -- returns null for a zero quantity", () => {
    expect(effectiveUnitCost("892.5000", 0)).toBeNull();
  });

  test("returns null for a negative or non-integer quantity", () => {
    expect(effectiveUnitCost("892.5000", -5)).toBeNull();
    expect(effectiveUnitCost("892.5000", 1.5)).toBeNull();
  });
});
