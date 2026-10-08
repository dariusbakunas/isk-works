import { describe, expect, test } from "vitest";

import type { Build } from "../../../../api/industry";
import {
  activeFilterChips,
  categoryOptions,
  DEFAULT_CRITERIA,
  filterAndSortBuilds,
  hasActiveFilters,
  type BuildLibraryCriteria,
} from "../build-library-filters";

function build(overrides: Partial<Build> = {}): Build {
  return {
    id: "b",
    workspaceId: "w",
    ownerId: "o",
    name: "Build",
    recipe: {
      kind: "manufacturing",
      sourceSdeDatasetId: "d",
      sourceSdeVersion: "1",
      blueprintTypeId: 1,
      blueprintName: "BP",
      durationSecondsPerRun: 1,
      materials: [],
      products: [{ typeId: 1, typeName: "Widget", quantityPerRun: 1, sortOrder: 0 }],
      fingerprint: "f",
    },
    runs: 1,
    notes: "",
    revision: 1,
    createdAt: "2026-08-01T00:00:00Z",
    updatedAt: "2026-08-01T00:00:00Z",
    draftPlanning: null,
    recipeCurrency: "current",
    activeSdeVersion: "1",
    productCategoryName: "Ship",
    productGroupName: "Frigate",
    selectedBlueprintOrigin: null,
    hasOwnedBlueprint: false,
    ...overrides,
  };
}

const criteria = (patch: Partial<BuildLibraryCriteria> = {}): BuildLibraryCriteria => ({
  ...DEFAULT_CRITERIA,
  ...patch,
});

describe("filterAndSortBuilds", () => {
  const library = [
    build({ id: "ship-old", name: "Machariel", updatedAt: "2026-08-02T00:00:00Z", createdAt: "2026-07-01T00:00:00Z", productCategoryName: "Ship", hasOwnedBlueprint: true }),
    build({ id: "ammo", name: "Scourge Fury batch", updatedAt: "2026-08-09T00:00:00Z", createdAt: "2026-08-08T00:00:00Z", productCategoryName: "Charge", recipe: { ...build().recipe, products: [{ typeId: 9, typeName: "Scourge Fury Heavy Missile", quantityPerRun: 1, sortOrder: 0 }] } }),
    build({ id: "ship-new", name: "Rifter", updatedAt: "2026-08-05T00:00:00Z", createdAt: "2026-08-04T00:00:00Z", productCategoryName: "Ship", hasOwnedBlueprint: true }),
  ];

  test("search matches name and output item name, case-insensitively", () => {
    expect(filterAndSortBuilds(library, criteria({ search: "MACH" })).map((b) => b.id)).toEqual(["ship-old"]);
    expect(filterAndSortBuilds(library, criteria({ search: "scourge fury heavy" })).map((b) => b.id)).toEqual(["ammo"]);
  });

  test("category filter matches productCategoryName exactly", () => {
    expect(filterAndSortBuilds(library, criteria({ category: "Charge" })).map((b) => b.id)).toEqual(["ammo"]);
    expect(filterAndSortBuilds(library, criteria({ category: "Ship" })).map((b) => b.id).sort()).toEqual(["ship-new", "ship-old"]);
  });

  test("blueprint filter keeps only builds whose blueprint is on hand", () => {
    expect(filterAndSortBuilds(library, criteria({ blueprint: "On hand" })).map((b) => b.id).sort()).toEqual(["ship-new", "ship-old"]);
    expect(filterAndSortBuilds(library, criteria({ blueprint: "All" })).length).toBe(3);
  });

  test("sorts by recently updated, name, or created", () => {
    expect(filterAndSortBuilds(library, criteria({ sort: "Recently updated" })).map((b) => b.id)).toEqual(["ammo", "ship-new", "ship-old"]);
    expect(filterAndSortBuilds(library, criteria({ sort: "Name" })).map((b) => b.name)).toEqual(["Machariel", "Rifter", "Scourge Fury batch"]);
    expect(filterAndSortBuilds(library, criteria({ sort: "Created" })).map((b) => b.id)).toEqual(["ammo", "ship-new", "ship-old"]);
  });

  test("combines filters", () => {
    expect(
      filterAndSortBuilds(library, criteria({ category: "Ship", blueprint: "On hand", search: "rifter" })).map((b) => b.id),
    ).toEqual(["ship-new"]);
  });
});

describe("categoryOptions", () => {
  test("prepends All and lists distinct categories alphabetically", () => {
    expect(
      categoryOptions([
        build({ productCategoryName: "Ship" }),
        build({ productCategoryName: "Charge" }),
        build({ productCategoryName: "Ship" }),
        build({ productCategoryName: null }),
      ]),
    ).toEqual(["All", "Charge", "Ship"]);
  });
});

describe("hasActiveFilters / activeFilterChips", () => {
  test("default criteria is inactive", () => {
    expect(hasActiveFilters(DEFAULT_CRITERIA)).toBe(false);
    expect(activeFilterChips(DEFAULT_CRITERIA)).toEqual([]);
  });

  test("reports each non-default filter", () => {
    const active = criteria({ search: " gun ", category: "Module", blueprint: "On hand" });
    expect(hasActiveFilters(active)).toBe(true);
    expect(activeFilterChips(active)).toEqual([
      'Search: "gun"',
      "Category: Module",
      "Blueprint: On hand",
    ]);
  });

  test("a non-default sort alone still counts as active", () => {
    expect(hasActiveFilters(criteria({ sort: "Name" }))).toBe(true);
  });
});
