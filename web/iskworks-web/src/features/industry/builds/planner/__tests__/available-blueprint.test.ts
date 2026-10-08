import { describe, expect, test } from "vitest";

import type { BlueprintObservation, BlueprintSelection } from "../../../../../api/industry";
import { reconcileObservedSelection, selectAvailableBlueprint } from "../available-blueprint";

function observation(
  id: string,
  kind: "original" | "copy",
  licensedRuns: number | null,
): BlueprintObservation {
  return {
    id,
    workspaceId: "workspace-1",
    ownerId: "owner-1",
    ownerName: "Valka",
    eveItemId: Number(id.replace(/\D/g, "")) || 1,
    blueprintTypeId: 691,
    blueprintName: "Rifter Blueprint",
    kind,
    materialEfficiency: 10,
    timeEfficiency: 20,
    licensedRuns,
    locationId: 60003760,
    locationFlag: "Hangar",
    locationName: "C-J6MT - GEZ",
    observedAt: "2026-07-27T16:00:00Z",
    importedAt: "2026-07-27T16:00:00Z",
  };
}

describe("selectAvailableBlueprint", () => {
  test("prefers a sufficient copy over an original", () => {
    const original = observation("original-1", "original", null);
    const copy = observation("copy-1", "copy", 30);

    expect(selectAvailableBlueprint([original, copy], 10)?.id).toBe(copy.id);
  });

  test("falls back to an original when copies are insufficient or unknown", () => {
    const insufficient = observation("copy-1", "copy", 2);
    const unknown = observation("copy-2", "copy", null);
    const original = observation("original-1", "original", null);

    expect(selectAvailableBlueprint([insufficient, unknown, original], 10)?.id).toBe(original.id);
  });

  test("falls back to a short copy, planned as several jobs, when nothing covers the runs in one", () => {
    expect(selectAvailableBlueprint([observation("copy-1", "copy", 2)], 10)?.id).toBe("copy-1");
  });

  test("returns null only when there is no blueprint at all", () => {
    expect(selectAvailableBlueprint([], 10)).toBeNull();
  });

  test("keeps the current selection while it remains sufficient", () => {
    const first = observation("copy-1", "copy", 30);
    const current = observation("copy-2", "copy", 20);

    expect(selectAvailableBlueprint([first, current], 10, current.id)?.id).toBe(current.id);
  });

  test("keeps a current copy with fewer runs than required -- it splits into jobs", () => {
    const sufficient = observation("copy-1", "copy", 30);
    const current = observation("copy-2", "copy", 1);

    expect(selectAvailableBlueprint([sufficient, current], 4, current.id)?.id).toBe(current.id);
  });
});

describe("reconcileObservedSelection", () => {
  const observed = (id: string): BlueprintSelection => ({ mode: "observedAsset", observationId: id });

  test("returns the exact observation when its id is still current", () => {
    const a = observation("copy-1", "copy", 30);
    const b = observation("copy-2", "copy", 30);

    expect(reconcileObservedSelection([a, b], observed("copy-2"), 10)?.id).toBe("copy-2");
  });

  test("recovers a stale id after a re-sync minted fresh observation rows", () => {
    // The saved id ("copy-old") is gone; the same blueprint now has a new id.
    const current = observation("copy-new", "copy", 30);

    expect(reconcileObservedSelection([current], observed("copy-old"), 10)?.id).toBe("copy-new");
  });

  test("falls back to an original when the stale id's copy is no longer available", () => {
    const original = observation("original-new", "original", null);

    expect(reconcileObservedSelection([original], observed("copy-old"), 10)?.id).toBe("original-new");
  });

  test("returns null for a manual selection", () => {
    const manual: BlueprintSelection = {
      mode: "manual",
      kind: "original",
      materialEfficiency: 10,
      timeEfficiency: 20,
      licensedRuns: null,
      notes: "",
    };
    expect(reconcileObservedSelection([observation("original-1", "original", null)], manual, 10)).toBeNull();
  });

  test("recovers a stale id onto a short copy -- it plans as several jobs", () => {
    expect(reconcileObservedSelection([observation("copy-1", "copy", 2)], observed("gone"), 10)?.id).toBe("copy-1");
  });

  test("returns null when no blueprint is observed at all", () => {
    expect(reconcileObservedSelection([], observed("gone"), 10)).toBeNull();
  });

  test("returns null for no selection", () => {
    expect(reconcileObservedSelection([observation("copy-1", "copy", 30)], null, 10)).toBeNull();
  });
});
