import { describe, expect, test } from "vitest";

import { activeNavItemPath, adminNavItem, flatNavItems, isNavPathActive, navGroups, resolveNavEntry } from "../registry";

describe("navigation registry", () => {
  test("groups pages as Pilots, Industry, Stock, Trade", () => {
    expect(navGroups.map((group) => group.label)).toEqual(["Pilots", "Industry", "Stock", "Trade"]);
    expect(navGroups[0].items.map((item) => item.label)).toEqual(["Overview", "Characters", "Skills", "Fittings", "Calendar"]);
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

  test("Overview (the home page) is active only on / itself", () => {
    expect(isNavPathActive("/", "/")).toBe(true);
    expect(isNavPathActive("/jobs", "/")).toBe(false);
    expect(resolveNavEntry("/")?.label).toBe("Overview");
    expect(resolveNavEntry("/jobs")?.label).toBe("Jobs");
  });

  test("resolves a location to the most specific entry", () => {
    expect(resolveNavEntry("/builds/123")).toMatchObject({ label: "Builds", path: "/builds" });
    expect(resolveNavEntry("/jobs/history")).toMatchObject({
      label: "History",
      path: "/jobs/history",
      parent: { label: "Jobs" },
    });
    expect(resolveNavEntry("/builds/blueprints")).toMatchObject({ label: "Blueprints", path: "/builds/blueprints" });
    expect(resolveNavEntry("/builds/blueprints")?.parent).toBeUndefined();
    expect(resolveNavEntry("/inventory")?.label).toBe("Inventory");
    expect(resolveNavEntry("/inventory")?.parent).toBeUndefined();
    expect(resolveNavEntry("/nope")).toBeNull();
  });

  test("Blueprints, Runway, Consumption and Analytics are rail rows of their own", () => {
    const rows = flatNavItems(false).map((item) => item.label);
    expect(rows).toEqual(expect.arrayContaining(["Blueprints", "Runway", "Consumption", "Analytics"]));
    const items = flatNavItems(false);
    expect(activeNavItemPath("/builds/blueprints", items)).toBe("/builds/blueprints");
    expect(activeNavItemPath("/builds/123", items)).toBe("/builds");
    expect(activeNavItemPath("/inventory/consumption", items)).toBe("/inventory/consumption");
    expect(activeNavItemPath("/finance/analytics", items)).toBe("/finance/analytics");
    expect(activeNavItemPath("/finance/transactions", items)).toBe("/finance");
    expect(activeNavItemPath("/nope", items)).toBeNull();
  });
});
