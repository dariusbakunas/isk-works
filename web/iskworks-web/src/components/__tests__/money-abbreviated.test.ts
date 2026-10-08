import { describe, expect, test } from "vitest";

import { formatIskAbbreviated } from "../money";

describe("formatIskAbbreviated", () => {
  test("abbreviates with K, M and B at the precision the charts use", () => {
    expect(formatIskAbbreviated("934000")).toBe("934K");
    expect(formatIskAbbreviated("498100000")).toBe("498.1M");
    expect(formatIskAbbreviated("1530000000")).toBe("1.53B");
    expect(formatIskAbbreviated("2400000000000")).toBe("2.4T");
  });

  test("never doubles a unit suffix (the prototype produced 1.24BM)", () => {
    for (const value of ["312000000", "1240000000", "5000"]) {
      expect(formatIskAbbreviated(value, { currency: true })).toMatch(/^[\d.]+[KMBT]? ISK$/);
    }
    expect(formatIskAbbreviated("312000000", { currency: true })).toBe("312M ISK");
  });

  test("keeps small values whole and trims trailing zeros", () => {
    expect(formatIskAbbreviated("0")).toBe("0");
    expect(formatIskAbbreviated("999")).toBe("999");
    expect(formatIskAbbreviated("12.5")).toBe("12.5");
    expect(formatIskAbbreviated("1000000")).toBe("1M");
    expect(formatIskAbbreviated("1500000")).toBe("1.5M");
  });

  test("carries the sign and never rounds up into a wrong unit", () => {
    expect(formatIskAbbreviated("-498100000")).toBe("-498.1M");
    expect(formatIskAbbreviated("498100000", { signDisplay: "always" })).toBe("+498.1M");
    expect(formatIskAbbreviated("999999")).toBe("1M");
    expect(formatIskAbbreviated("999999999")).toBe("1B");
  });

  test("passes unparseable input through instead of printing NaN", () => {
    expect(formatIskAbbreviated("n/a")).toBe("n/a");
  });
});
