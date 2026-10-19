import { describe, expect, test } from "vitest";

import {
  DEFAULT_VIEW,
  blueprintLibraryQuery,
  blueprintLibraryViewParams,
  hasActiveFilters,
  parseBlueprintLibraryView,
} from "../blueprint-library-url-state";

const CONNECTION = "0b8f8d6e-2f43-4c1a-9a55-5d0f3c6f9a11";

describe("blueprint library URL state", () => {
  test("an empty URL is the default view", () => {
    expect(parseBlueprintLibraryView(new URLSearchParams())).toEqual(DEFAULT_VIEW);
    expect(blueprintLibraryViewParams(DEFAULT_VIEW).toString()).toBe("");
    expect(hasActiveFilters(DEFAULT_VIEW)).toBe(false);
  });

  test("round-trips every filter", () => {
    const view = {
      ...DEFAULT_VIEW,
      search: "rifter",
      kinds: ["copy" as const],
      categoryIds: [6],
      groupIds: [25, 26],
      connectionIds: [CONNECTION],
      locationIds: [60003760],
      minMe: 8,
      minTe: 16,
      needsResearch: true,
      activity: "manufacturing" as const,
      sort: "runs" as const,
      order: "desc" as const,
      groupBy: "location" as const,
    };

    const params = blueprintLibraryViewParams(view);

    expect(parseBlueprintLibraryView(params)).toEqual(view);
    expect(hasActiveFilters(view)).toBe(true);
  });

  test("drops malformed values instead of failing", () => {
    const view = parseBlueprintLibraryView(new URLSearchParams(
      "kinds=copy,bogus,copy&categoryIds=6,ship,-1&connectionIds=nope&minMe=11&minTe=x&sort=price&order=up&groupBy=moon&activity=invention",
    ));

    expect(view).toEqual({ ...DEFAULT_VIEW, kinds: ["copy"], categoryIds: [6] });
  });

  test("the API query leaves out the client-side grouping", () => {
    expect(blueprintLibraryQuery({ ...DEFAULT_VIEW, groupBy: "none" })).not.toHaveProperty("groupBy");
  });
});
