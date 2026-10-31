import { afterEach, describe, expect, test, vi } from "vitest";

import {
  cleanDensityInput,
  DEFAULT_CHARACTERS,
  DEFAULT_SETTINGS,
  loadCharacters,
  percentRate,
  planetTypeKey,
  ratePercent,
  resourceCode,
  settingsFromQuery,
  settingsToQuery,
  storeCharacters,
  type ScenarioSettings,
} from "../planner-format";

afterEach(() => {
  vi.restoreAllMocks();
  window.localStorage.clear();
});

describe("planner-format", () => {
  test("labels", () => {
    expect(planetTypeKey("Planet (Barren)")).toBe("barren");
    expect(planetTypeKey("Gas")).toBe("gas");
    expect(resourceCode("Base Metals")).toBe("BM");
    expect(resourceCode("Non-CS Crystals")).toBe("NC");
    expect(resourceCode("Autotrophs")).toBe("AU");
    expect(cleanDensityInput("4a2.55x")).toBe("42.55");
    expect(cleanDensityInput("1234")).toBe("123");
    expect(ratePercent("0.1")).toBe("10");
    expect(percentRate("7.5")).toBe("0.075");
    expect(percentRate("101")).toBeNull();
  });

  test("the scenario round-trips through the URL and defaults stay off it", () => {
    expect(settingsToQuery(DEFAULT_SETTINGS, new URLSearchParams()).toString()).toBe("");
    const settings: ScenarioSettings = {
      homeSystemId: 30_000_797,
      radius: 3,
      defaultCustomsRate: "0.05",
      resetDays: 7,
      haulDays: 14,
      upliftAtLogin: false,
      buyIntermediates: true,
      maxColonies: 6,
    };
    const query = settingsToQuery(settings, new URLSearchParams("tab=x"));
    expect(query.get("tab")).toBe("x");
    expect(settingsFromQuery(query)).toEqual(settings);
  });

  test("bad query values fall back to defaults", () => {
    expect(settingsFromQuery(new URLSearchParams("system=abc&radius=9&reset=5&tax=200&max=0"))).toEqual(
      DEFAULT_SETTINGS,
    );
  });

  test("characters are remembered, clamped, and survive blocked storage", () => {
    storeCharacters([{ name: "Valka", interplanetaryConsolidation: 4, commandCenterUpgrades: 5 }]);
    expect(loadCharacters()).toEqual([{ name: "Valka", interplanetaryConsolidation: 4, commandCenterUpgrades: 5 }]);

    window.localStorage.setItem(
      "iskworks.piPlanner.characters",
      JSON.stringify([{ interplanetaryConsolidation: 9, commandCenterUpgrades: -2 }]),
    );
    expect(loadCharacters()).toEqual([{ name: "Character 1", interplanetaryConsolidation: 5, commandCenterUpgrades: 0 }]);

    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    expect(loadCharacters()).toEqual(DEFAULT_CHARACTERS);
    expect(() => storeCharacters(DEFAULT_CHARACTERS)).not.toThrow();
  });
});
