import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, expect, test, vi } from "vitest";

import { queryAssets, type FlatAssetPage, type FlatAssetQuery } from "../../../api/assets";
import { useAssetsQuery } from "../use-assets-query";

vi.mock("../../../api/assets", async (importOriginal) => ({
  ...await importOriginal<typeof import("../../../api/assets")>(),
  queryAssets: vi.fn(),
}));

const query: FlatAssetQuery = { sort: "item", order: "asc", limit: 2 };

function page(names: string[], nextCursor: string | null): FlatAssetPage {
  const ids: Record<string, number> = { First: 1, Second: 2, Third: 3, Fourth: 4, Rifter: 5, Stale: 6 };
  return {
    rows: names.map((typeName, index) => ({
      eveItemId: ids[typeName] ?? index + 100, typeId: 34 + index, typeName, quantity: 1,
      packagedVolume: "0.01", totalPackagedVolume: "0.01", ownerId: "owner",
      ownerName: "Owner", connectionId: "connection", characterId: 1,
      characterName: "Valka", locationId: 60003760, locationName: "Jita",
      locationFlag: "Hangar", containerItemId: null, containerName: null,
      groupId: 18, groupName: "Mineral", assetKind: "material",
      observedAt: "2026-08-06T12:00:00Z", blueprint: null,
      reconciliation: { state: "accounted", observedOwnerTypeQuantity: 1, accountedOwnerTypeQuantity: 1, scope: "ownerType" },
      isContainer: false,
    })),
    total: nextCursor ? 4 : names.length,
    nextCursor,
    summary: { locationCount: 1, characterCount: 1, stackCount: 2, totalQuantity: 2, totalPackagedVolume: "0.0200", latestObservedAt: "2026-08-06T12:00:00Z", unresolvedLocationCount: 0, syncStates: [] },
    facets: { characters: [], locations: [], assetKinds: [], groups: [], blueprintKinds: [], reconciliationStates: [] },
  };
}

beforeEach(() => vi.mocked(queryAssets).mockReset());

test("appends cursor pages and keeps existing rows when load more fails", async () => {
  vi.mocked(queryAssets)
    .mockResolvedValueOnce(page(["First", "Second"], "next"))
    .mockRejectedValueOnce(new Error("temporary failure"))
    .mockResolvedValueOnce(page(["Third", "Fourth"], null));
  const { result } = renderHook(() => useAssetsQuery(query));
  await waitFor(() => expect(result.current.rows).toHaveLength(2));

  act(() => result.current.loadMore());
  await waitFor(() => expect(result.current.error?.message).toBe("temporary failure"));
  expect(result.current.rows).toHaveLength(2);

  act(() => result.current.retry());
  await waitFor(() => expect(result.current.rows).toHaveLength(4));
});

test("ignores an old response after query identity changes", async () => {
  let resolveOld!: (value: FlatAssetPage) => void;
  const old = new Promise<FlatAssetPage>((resolve) => { resolveOld = resolve; });
  vi.mocked(queryAssets).mockReturnValueOnce(old).mockResolvedValueOnce(page(["Rifter"], null));
  const { result, rerender } = renderHook(({ value }) => useAssetsQuery(value), { initialProps: { value: query } });
  rerender({ value: { ...query, search: "Rifter" } });
  await waitFor(() => expect(result.current.rows[0]?.typeName).toBe("Rifter"));
  resolveOld(page(["Stale"], null));
  await act(async () => Promise.resolve());
  expect(result.current.rows[0]?.typeName).toBe("Rifter");
});

test("keeps the first page's summary and facets when later pages omit them", async () => {
  vi.mocked(queryAssets)
    .mockResolvedValueOnce(page(["First", "Second"], "next"))
    .mockResolvedValueOnce({ ...page(["Third", "Fourth"], null), summary: null, facets: null });
  const { result } = renderHook(() => useAssetsQuery(query));
  await waitFor(() => expect(result.current.rows).toHaveLength(2));

  act(() => result.current.loadMore());
  await waitFor(() => expect(result.current.rows).toHaveLength(4));
  expect(result.current.page?.summary.stackCount).toBe(2);
  expect(result.current.page?.facets.assetKinds).toEqual([]);
});
