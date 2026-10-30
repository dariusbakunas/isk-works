import { describe, expect, test } from "vitest";

import type { BlueprintLibraryRow } from "../../../../api/industry";
import { BLUEPRINT_EXPORT_COLUMNS, blueprintLibraryCsv, DEFAULT_EXPORT_COLUMNS, EXPORT_COLUMNS_STORAGE_KEY } from "../blueprint-export";
import { readStoredColumns, storeColumns } from "../../shared/export-columns-dialog";

const base: BlueprintLibraryRow = {
  observationId: "obs-1",
  eveItemId: 1001,
  blueprintTypeId: 691,
  blueprintName: "Rifter Blueprint",
  activity: "manufacturing",
  productTypeId: 587,
  productName: "Rifter",
  productGroupId: 25,
  productGroupName: "Frigate",
  productCategoryId: 6,
  productCategoryName: "Ship",
  kind: "original",
  materialEfficiency: 10,
  timeEfficiency: 20,
  licensedRuns: null,
  connectionId: null,
  characterName: "Valka",
  locationId: 60003760,
  locationName: "Jita IV - Moon 4, Caldari Navy",
  locationFlag: "Hangar",
  observedAt: "2026-10-18T12:00:00Z",
  usedByBuilds: [],
};

describe("blueprintLibraryCsv", () => {
  test("writes the table columns by default, quoting commas", () => {
    expect(blueprintLibraryCsv([base], DEFAULT_EXPORT_COLUMNS)).toBe(
      "Blueprint,Product,Kind,ME,TE,Runs,Character,Location\n"
      + "Rifter Blueprint,Rifter,Original,10,20,Unlimited,Valka,\"Jita IV - Moon 4, Caldari Navy\"\n",
    );
  });

  test("keeps the canonical column order whatever order they were picked in", () => {
    expect(blueprintLibraryCsv([base], ["itemId", "blueprint"]).split("\n")[0]).toBe("Blueprint,Item ID");
  });

  test("copies give their runs, exhausted copies 0, reactions blank ME/TE", () => {
    const rows: BlueprintLibraryRow[] = [
      { ...base, kind: "copy", licensedRuns: 7 },
      { ...base, kind: "copy", licensedRuns: null },
      { ...base, activity: "reaction", kind: "unknown", materialEfficiency: 0, timeEfficiency: 0 },
    ];
    expect(blueprintLibraryCsv(rows, ["kind", "me", "te", "runs"]).trim().split("\n")).toEqual([
      "Kind,ME,TE,Runs",
      "Copy,10,20,7",
      "Copy,10,20,0",
      "Reaction formula,,,Unlimited",
    ]);
  });

  test("lists using builds and guards formula-looking names", () => {
    const row = {
      ...base,
      blueprintName: "=HYPERLINK()",
      usedByBuilds: [{ buildId: "b1", name: "Rifter run" }, { buildId: "b2", name: "Fleet" }],
    };
    expect(blueprintLibraryCsv([row], ["blueprint", "usedByBuilds"]).split("\n")[1]).toBe("'=HYPERLINK(),Rifter run; Fleet");
  });
});

describe("export column memory", () => {
  test("falls back to the table columns, and drops unknown keys", () => {
    const read = () => readStoredColumns(EXPORT_COLUMNS_STORAGE_KEY, BLUEPRINT_EXPORT_COLUMNS, DEFAULT_EXPORT_COLUMNS);
    window.localStorage.clear();
    expect(read()).toEqual(DEFAULT_EXPORT_COLUMNS);
    window.localStorage.setItem(EXPORT_COLUMNS_STORAGE_KEY, JSON.stringify(["gone", "te", "blueprint"]));
    expect(read()).toEqual(["blueprint", "te"]);
    storeColumns(EXPORT_COLUMNS_STORAGE_KEY, ["itemId"]);
    expect(read()).toEqual(["itemId"]);
  });
});
