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
  ownedBlueprint,
  rememberLoaded,
  showType,
  withActivity,
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

  const copy = { kind: "copy" as const, runs: 26, materialEfficiency: 10, timeEfficiency: 20 };

  test("an entry remembers the owned blueprint it was opened from, across loading", () => {
    const state = newTypeInfoState(954, "Tristan Blueprint", copy);
    expect(currentEntry(state).blueprint).toEqual(copy);
    const loaded = rememberLoaded(state, { typeId: 954, name: "Tristan Blueprint", metaGroupId: 1 });
    expect(currentEntry(loaded)).toEqual({ typeId: 954, name: "Tristan Blueprint", metaGroupId: 1, blueprint: copy });
  });

  test("the same type opened from another owned blueprint swaps the blueprint, not the history", () => {
    const state = withTab(newTypeInfoState(954, "Tristan Blueprint", copy), "Description");
    const original = { kind: "original" as const, runs: null, materialEfficiency: 0, timeEfficiency: 0 };
    const merged = mergeTypeInfo(state, newTypeInfoState(954, "Tristan Blueprint", original));
    expect(merged.history).toHaveLength(1);
    expect(currentEntry(merged).blueprint).toEqual(original);
    expect(merged.tab).toBe("Description");
    // Opened again the same way, or with nothing known: unchanged.
    expect(mergeTypeInfo(merged, newTypeInfoState(954, "Tristan Blueprint", { ...original }))).toBe(merged);
    expect(mergeTypeInfo(merged, newTypeInfoState(954, "Tristan Blueprint"))).toBe(merged);
  });

  test("the selected activity belongs to the item showing", () => {
    const state = withActivity(newTypeInfoState(954, "Tristan Blueprint"), "invention");
    expect(state.activity).toBe("invention");
    const next = showType(state, 11173, "Nemesis Blueprint");
    expect(next.activity).toBeUndefined();
    expect(goBack(withActivity(next, "copying")).activity).toBeUndefined();
  });

  test("an observed blueprint carries over only when it is an original or a copy", () => {
    const observed = { kind: "copy", licensedRuns: 26, materialEfficiency: 10, timeEfficiency: 20, observedAt: "x" };
    expect(ownedBlueprint(observed)).toEqual(copy);
    // An original's runs are unlimited whatever the observation says.
    expect(ownedBlueprint({ ...observed, kind: "original", licensedRuns: -1 })?.runs).toBeNull();
    expect(ownedBlueprint({ ...observed, kind: "unknown" })).toBeUndefined();
    expect(ownedBlueprint(null)).toBeUndefined();
  });
});
