import { describe, expect, test } from "vitest";

import {
  DEFAULT_VIEW,
  inventionQuoteCommand,
  inventionViewParams,
  parseInventionView,
  resolveChoice,
  type InventionView,
} from "../invention-url-state";

const CHARACTER = "0b8f8d6e-2f43-4c1a-9a55-5d0f3c6f9a11";
const FACILITY = "5d0f3c6f-9a11-4c1a-9a55-0b8f8d6e2f43";

describe("invention url state", () => {
  test("an empty url is the default view and writes no parameters", () => {
    expect(parseInventionView(new URLSearchParams())).toEqual(DEFAULT_VIEW);
    expect(inventionViewParams(DEFAULT_VIEW).toString()).toBe("");
  });

  test("a full view round-trips", () => {
    const view: InventionView = {
      productTypeId: 11377,
      characterId: CHARACTER,
      skillOverrides: { 23121: 5, 11450: 0 },
      inventionFacilityId: FACILITY,
      manufacturingFacilityId: "none",
      targetRuns: 250,
      selected: "34206",
    };

    const params = inventionViewParams(view);

    expect(params.get("skills")).toBe("11450:0,23121:5");
    expect(parseInventionView(params)).toEqual(view);
  });

  test("the no-decryptor row can be the selected one", () => {
    expect(parseInventionView(new URLSearchParams("option=none")).selected).toBe("none");
  });

  test("malformed values fall back to their defaults", () => {
    const view = parseInventionView(new URLSearchParams(
      "product=abc&char=not-a-uuid&skills=1:9,x:2,23121:4&inv=1&runs=0&option=best",
    ));

    expect(view).toEqual({ ...DEFAULT_VIEW, skillOverrides: { 23121: 4 } });
    expect(parseInventionView(new URLSearchParams("runs=99999999")).targetRuns).toBe(100);
  });

  test("a default choice takes the first candidate and none stays nothing", () => {
    expect(resolveChoice(null, [FACILITY, CHARACTER])).toBe(FACILITY);
    expect(resolveChoice(CHARACTER, [FACILITY, CHARACTER])).toBe(CHARACTER);
    expect(resolveChoice("none", [FACILITY])).toBeNull();
    expect(resolveChoice(null, [])).toBeNull();
    // A stored choice that no longer exists falls back to the default.
    expect(resolveChoice(CHARACTER, [FACILITY])).toBe(FACILITY);
  });

  test("no product means no quote request", () => {
    expect(inventionQuoteCommand(DEFAULT_VIEW, {
      characterId: CHARACTER,
      inventionFacilityId: FACILITY,
      manufacturingFacilityId: FACILITY,
    })).toBeNull();
  });

  test("the quote request carries only what is set", () => {
    const view: InventionView = { ...DEFAULT_VIEW, productTypeId: 11377, skillOverrides: { 23121: 5 } };

    expect(inventionQuoteCommand(view, {
      characterId: CHARACTER,
      inventionFacilityId: null,
      manufacturingFacilityId: FACILITY,
    })).toEqual({
      productTypeId: 11377,
      targetRuns: 100,
      characterId: CHARACTER,
      manufacturingFacilityId: FACILITY,
      skillOverrides: { "23121": 5 },
    });
  });
});
