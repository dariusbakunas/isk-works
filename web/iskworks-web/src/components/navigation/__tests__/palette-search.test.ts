import { LogOut } from "lucide-react";
import { describe, expect, test } from "vitest";

import { pageEntries, searchPalette, type PaletteEntry } from "../palette-search";

const signOut: PaletteEntry = { id: "action:sign-out", kind: "action", label: "Sign out", group: "Actions", icon: LogOut, keywords: ["logout"], run: () => {} };
const entries = [...pageEntries(false), signOut];
const labels = (query: string) => searchPalette(entries, query).map((match) => match.entry.label);

describe("pageEntries", () => {
  test("lists items and their sub-pages as Parent › Child with the parent's icon and group", () => {
    const blueprints = pageEntries(false).find((entry) => entry.path === "/builds/blueprints");
    const builds = pageEntries(false).find((entry) => entry.path === "/builds");
    expect(blueprints).toMatchObject({ label: "Builds › Blueprints", group: "Industry", icon: builds?.icon });
  });

  test("lists Admin only for admins", () => {
    expect(pageEntries(false).some((entry) => entry.path === "/admin")).toBe(false);
    expect(pageEntries(true).some((entry) => entry.path === "/admin")).toBe(true);
  });
});

describe("searchPalette", () => {
  test("an empty query matches nothing", () => {
    expect(searchPalette(entries, "  ")).toEqual([]);
  });

  test("word starts beat looser matches: mo finds Market Orders first", () => {
    const results = labels("mo");
    expect(results[0]).toBe("Market Orders");
    expect(results).toContain("Price Overrides › Market imports");
    expect(results).not.toContain("Market");
  });

  test("highlights the matched characters of the label", () => {
    const [first] = searchPalette(entries, "mo");
    expect(first.matched).toEqual([0, 7]);
    expect(searchPalette(entries, "Inv")[0].matched).toEqual([0, 1, 2]);
  });

  test("a keyword match is flagged", () => {
    const [first] = searchPalette(entries, "bp");
    expect(first.entry.label).toBe("Builds › Blueprints");
    expect(first.viaKeyword).toBe(true);
    expect(searchPalette(entries, "wallet")[0].entry.label).toBe("Finance");
    expect(searchPalette(entries, "Board")[0].viaKeyword).toBe(false);
  });

  test("a keyword beats a word inside a label: orders opens Market Orders", () => {
    expect(labels("orders")[0]).toBe("Market Orders");
  });

  test("actions match too", () => {
    expect(labels("sign")).toEqual(["Sign out"]);
  });

  test("is case-insensitive and finds nothing for nonsense", () => {
    expect(labels("BOARD")[0]).toBe("Board");
    expect(labels("zzz")).toEqual([]);
  });

  test("a group name lists its pages, below direct matches", () => {
    const results = labels("stock");
    expect(results).toEqual(expect.arrayContaining(["Inventory", "Assets"]));
  });
});
