import { describe, expect, it } from "vitest";

import type { RunwayMaterial } from "../../../../../api/inventory-analytics";
import { shoppingListCsv, shoppingListMultibuy } from "../shopping-list-export";

function material(overrides: Partial<RunwayMaterial>): RunwayMaterial {
  return {
    typeId: 11399,
    name: "Morphite",
    category: "Minerals",
    needed: 100,
    onHand: 0,
    reserved: 0,
    short: 100,
    unitCost: "16700.0000",
    shortValue: "1670000.0000",
    coveragePercent: 0,
    epics: [],
    ...overrides,
  };
}

const materials = [
  material({}),
  material({ typeId: 34, name: "Tritanium", short: 0, shortValue: "0.0000", coveragePercent: 100 }),
  material({ typeId: 1, name: 'Odd, "quoted" name', category: "Other", short: 5, unitCost: null, shortValue: null }),
];

describe("shoppingListMultibuy", () => {
  it("lists only short materials as EVE multibuy lines", () => {
    expect(shoppingListMultibuy(materials)).toBe('Morphite\t100\nOdd, "quoted" name\t5');
  });

  it("is empty when nothing is short", () => {
    expect(shoppingListMultibuy([materials[1]])).toBe("");
  });
});

describe("shoppingListCsv", () => {
  it("writes a header and one escaped row per short material", () => {
    expect(shoppingListCsv(materials)).toBe(
      [
        "Type ID,Name,Category,Short,Unit cost,Short value",
        "11399,Morphite,Minerals,100,16700.0000,1670000.0000",
        '1,"Odd, ""quoted"" name",Other,5,,',
      ].join("\n") + "\n",
    );
  });

  it("neutralises cells a spreadsheet would run as formulas", () => {
    const csv = shoppingListCsv([material({ name: "=HYPERLINK(1)" })]);
    expect(csv.split("\n")[1]).toContain(",'=HYPERLINK(1),");
  });
});
