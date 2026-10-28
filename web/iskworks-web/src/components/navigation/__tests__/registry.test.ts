import { describe, expect, test } from "vitest";

import { adminNavItem, flatNavItems, isNavPathActive, navGroups, resolveNavEntry } from "../registry";

describe("navigation registry", () => {
  test("groups pages as Pilots, Industry, Stock, Trade", () => {
    expect(navGroups.map((group) => group.label)).toEqual(["Pilots", "Industry", "Stock", "Trade"]);
    expect(navGroups[0].items.map((item) => item.label)).toEqual(["Characters", "Skills", "Fittings", "Calendar"]);
  });

  test("every entry has an icon and a unique path", () => {
    const items = flatNavItems(true);
    for (const item of items) expect(item.icon).toBeTruthy();
    const paths = items.flatMap((item) => [item.path, ...(item.children ?? []).map((child) => child.path)]);
    expect(new Set(paths).size).toBe(paths.length);
  });

  test("Admin is listed last, and only for admins", () => {
    expect(flatNavItems(true).at(-1)).toBe(adminNavItem);
    expect(flatNavItems(false)).not.toContain(adminNavItem);
  });

  test("a path is active on itself and its sub-routes, by whole segments", () => {
    expect(isNavPathActive("/builds", "/builds")).toBe(true);
    expect(isNavPathActive("/builds/blueprints", "/builds")).toBe(true);
    expect(isNavPathActive("/market-orders", "/market")).toBe(false);
  });

  test("resolves a location to the most specific entry", () => {
    expect(resolveNavEntry("/builds/123")).toMatchObject({ label: "Builds", path: "/builds" });
    expect(resolveNavEntry("/inventory/runway")).toMatchObject({
      label: "Runway",
      path: "/inventory/runway",
      parent: { label: "Inventory" },
    });
    expect(resolveNavEntry("/inventory")?.label).toBe("Inventory");
    expect(resolveNavEntry("/inventory")?.parent).toBeUndefined();
    expect(resolveNavEntry("/nope")).toBeNull();
  });
});
