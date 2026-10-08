import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, test } from "vitest";

import { useMarketScopeRecents, type MarketScopeRecent } from "../use-market-scope-recents";

const jita: MarketScopeRecent = {
  kind: "hub",
  regionId: 10_000_002,
  locationId: 60_003_760,
  label: "Jita 4-4",
  subLabel: "The Forge",
};
const theForge: MarketScopeRecent = {
  kind: "region",
  regionId: 10_000_002,
  label: "The Forge",
};
const keepstar: MarketScopeRecent = {
  kind: "structure",
  regionId: 10_000_058,
  locationId: 1_049_995_520_085,
  label: "GEZ-IXX Keepstar",
  subLabel: "C-J6MT",
};

beforeEach(() => {
  window.localStorage.clear();
});

describe("useMarketScopeRecents", () => {
  test("starts empty when nothing is stored", () => {
    const { result } = renderHook(() => useMarketScopeRecents());
    expect(result.current.recents).toEqual([]);
  });

  test("pushRecent adds to the front and persists across a fresh mount", () => {
    const { result, unmount } = renderHook(() => useMarketScopeRecents());

    act(() => result.current.pushRecent(jita));
    act(() => result.current.pushRecent(theForge));

    expect(result.current.recents.map((recent) => recent.label)).toEqual(["The Forge", "Jita 4-4"]);

    unmount();
    const { result: remounted } = renderHook(() => useMarketScopeRecents());
    expect(remounted.current.recents.map((recent) => recent.label)).toEqual(["The Forge", "Jita 4-4"]);
  });

  test("re-pushing an existing recent moves it to the front instead of duplicating it", () => {
    const { result } = renderHook(() => useMarketScopeRecents());

    act(() => result.current.pushRecent(jita));
    act(() => result.current.pushRecent(theForge));
    act(() => result.current.pushRecent(keepstar));
    act(() => result.current.pushRecent(jita));

    expect(result.current.recents.map((recent) => recent.label)).toEqual([
      "Jita 4-4",
      "GEZ-IXX Keepstar",
      "The Forge",
    ]);
  });

  test("caps at 5 entries, dropping the oldest", () => {
    const { result } = renderHook(() => useMarketScopeRecents());

    for (let index = 0; index < 7; index += 1) {
      act(() =>
        result.current.pushRecent({
          kind: "region",
          regionId: 10_000_000 + index,
          label: `Region ${index}`,
        }),
      );
    }

    expect(result.current.recents).toHaveLength(5);
    expect(result.current.recents.map((recent) => recent.label)).toEqual([
      "Region 6",
      "Region 5",
      "Region 4",
      "Region 3",
      "Region 2",
    ]);
  });

  test("distinguishes a hub from a region sharing the same regionId (kind is part of identity)", () => {
    const { result } = renderHook(() => useMarketScopeRecents());

    act(() => result.current.pushRecent(jita));
    act(() => result.current.pushRecent(theForge));

    expect(result.current.recents).toHaveLength(2);
  });

  test("survives corrupted localStorage content instead of throwing", () => {
    window.localStorage.setItem("iskworks:market-scope-recents", "not json");
    const { result } = renderHook(() => useMarketScopeRecents());
    expect(result.current.recents).toEqual([]);
  });
});
