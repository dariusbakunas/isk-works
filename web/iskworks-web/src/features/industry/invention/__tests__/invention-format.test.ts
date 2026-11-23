import { describe, expect, test } from "vitest";

import {
  copyTone,
  formatChance,
  formatIskDelta,
  formatPercentDelta,
  gapLabel,
  optionKey,
  shoppingLines,
  shoppingListText,
  sortOptions,
} from "../invention-format";
import { ACCELERANT, PROCESS, quote } from "./invention-fixtures";

describe("invention formatting", () => {
  test("a chance is truncated to one decimal, as the game shows it", () => {
    expect(formatChance("0.4375")).toBe("43.7%");
    expect(formatChance("0.48125")).toBe("48.1%");
    expect(formatChance("0.34")).toBe("34.0%");
    expect(formatChance("1")).toBe("100.0%");
    // 0.464667 is 46.4%, where rounding would say 46.5%.
    expect(formatChance("0.464667")).toBe("46.4%");
  });

  test("deltas carry their sign", () => {
    expect(formatIskDelta("71981.8181")).toBe("+72K");
    expect(formatIskDelta("-12000")).toBe("−12K");
    expect(formatIskDelta("0")).toBe("0");
    expect(formatPercentDelta("10.3")).toBe("+10.3%");
    expect(formatPercentDelta("-1.7")).toBe("−1.7%");
    expect(formatPercentDelta("0")).toBe("0.0%");
  });

  test("a copy figure is better when it is higher", () => {
    expect(copyTone(5, 2)).toBe("better");
    expect(copyTone(0, 2)).toBe("worse");
    expect(copyTone(10, 10)).toBe("same");
  });

  test("options sort by unit cost with unpriced rows last either way", () => {
    const { options } = quote();
    const keys = (sorted: typeof options) => sorted.map(optionKey);

    expect(keys(sortOptions(options, "unitCost", "asc"))).toEqual(["none", String(PROCESS), String(ACCELERANT)]);
    expect(keys(sortOptions(options, "unitCost", "desc"))).toEqual([String(PROCESS), "none", String(ACCELERANT)]);
    expect(keys(sortOptions(options, "chance", "desc"))).toEqual([String(ACCELERANT), String(PROCESS), "none"]);
    expect(keys(sortOptions(options, "decryptor", "asc"))).toEqual([String(ACCELERANT), "none", String(PROCESS)]);
  });

  test("a gap names what is missing", () => {
    const fixture = quote();
    expect(gapLabel({ kind: "decryptorPrice", typeId: ACCELERANT }, fixture)).toBe("No decryptor price");
    expect(gapLabel({ kind: "materialPrice", typeId: 20418 }, fixture)).toBe("No price for Datacore - Electronic Engineering");
    expect(gapLabel({ kind: "jobFee" }, fixture)).toBe("Invention job fee unknown");
  });

  test("the shopping list covers the confident attempts", () => {
    const fixture = quote();
    const process = fixture.options[2];

    expect(shoppingLines(fixture, process)).toEqual([
      { name: "Datacore - Electronic Engineering", quantity: 54, marketItem: true },
      { name: "Datacore - Graviton Physics", quantity: 54, marketItem: true },
      { name: "Process Decryptor", quantity: 27, marketItem: true },
      { name: "T1 copy runs", quantity: 27, marketItem: false },
    ]);
    // Copy runs are made, not bought, so they stay out of the pasted list.
    expect(shoppingListText(fixture, process)).toBe(
      "Datacore - Electronic Engineering 54\nDatacore - Graviton Physics 54\nProcess Decryptor 27",
    );
  });
});
