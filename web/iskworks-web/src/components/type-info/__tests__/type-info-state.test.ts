import { describe, expect, test } from "vitest";

import {
  MAX_HISTORY,
  canGoBack,
  canGoForward,
  currentEntry,
  goBack,
  goForward,
  mergeTypeInfo,
  newTypeInfoState,
  rememberLoaded,
  showType,
  withPilot,
  withTab,
} from "../type-info-state";

const harpy = newTypeInfoState(11381, "Harpy");

describe("type info window state", () => {
  test("a new window shows one type with no history", () => {
    expect(currentEntry(harpy)).toEqual({ typeId: 11381, name: "Harpy" });
    expect(canGoBack(harpy)).toBe(false);
    expect(canGoForward(harpy)).toBe(false);
  });

  test("following a link pushes history; back and forward walk it", () => {
    const script = showType(harpy, 40334, "Missile Range Disruption Script");
    expect(currentEntry(script).typeId).toBe(40334);
    expect(canGoBack(script)).toBe(true);

    const back = goBack(script);
    expect(currentEntry(back).typeId).toBe(11381);
    expect(canGoForward(back)).toBe(true);
    expect(currentEntry(goForward(back)).typeId).toBe(40334);
  });

  test("a new link from the middle of history drops the forward entries", () => {
    const state = goBack(showType(showType(harpy, 1, "One"), 2, "Two"));
    const branched = showType(state, 3, "Three");
    expect(branched.history.map((entry) => entry.typeId)).toEqual([11381, 1, 3]);
    expect(canGoForward(branched)).toBe(false);
  });

  test("showing the type already shown changes nothing", () => {
    expect(showType(harpy, 11381, "Harpy")).toBe(harpy);
  });

  test(`history keeps the last ${MAX_HISTORY} types`, () => {
    let state = harpy;
    for (let typeId = 1; typeId <= MAX_HISTORY + 5; typeId += 1) state = showType(state, typeId);
    expect(state.history).toHaveLength(MAX_HISTORY);
    expect(currentEntry(state).typeId).toBe(MAX_HISTORY + 5);
    expect(state.index).toBe(MAX_HISTORY - 1);
  });

  test("a new type starts on its first tab", () => {
    const onTraits = withTab(harpy, "Traits");
    expect(onTraits.tab).toBe("Traits");
    expect(showType(onTraits, 34, "Tritanium").tab).toBeUndefined();
    expect(goBack(showType(onTraits, 34)).tab).toBeUndefined();
  });

  test("an open from elsewhere is a link followed in this window", () => {
    const merged = mergeTypeInfo(harpy, newTypeInfoState(34, "Tritanium"));
    expect(merged.history.map((entry) => entry.typeId)).toEqual([11381, 34]);
    expect(withPilot(merged, "allZero").pilot).toBe("allZero");
    expect(mergeTypeInfo(withPilot(harpy, "allZero"), newTypeInfoState(34)).pilot).toBe("allZero");
  });

  test("what loaded is remembered for the title and the dock", () => {
    const loaded = rememberLoaded(harpy, { typeId: 11381, name: "Harpy", metaGroupId: 2 });
    expect(currentEntry(loaded)).toEqual({ typeId: 11381, name: "Harpy", metaGroupId: 2 });
    expect(rememberLoaded(loaded, { typeId: 11381, name: "Harpy", metaGroupId: 2 })).toBe(loaded);
    // A late answer for a type no longer shown is ignored.
    expect(rememberLoaded(showType(harpy, 34), { typeId: 11381, name: "X", metaGroupId: null }).history[0].name).toBe("Harpy");
  });
});
