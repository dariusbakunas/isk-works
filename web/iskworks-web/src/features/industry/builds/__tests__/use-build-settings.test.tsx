import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { Build } from "../../../../api/industry";
import { useBuildSettings } from "../use-build-settings";

const getBuild = vi.fn();
const setBuildBlueprintSelection = vi.fn();
const setBuildFacility = vi.fn();
const setBuildPricing = vi.fn();
const updateBuild = vi.fn();
const previewCreateBuildCandidate = vi.fn();

vi.mock("../../../../api/industry", async (importOriginal) => ({
  ...(await importOriginal<object>()),
  getBuild: (...args: unknown[]) => getBuild(...args),
  setBuildBlueprintSelection: (...args: unknown[]) => setBuildBlueprintSelection(...args),
  setBuildFacility: (...args: unknown[]) => setBuildFacility(...args),
  setBuildPricing: (...args: unknown[]) => setBuildPricing(...args),
  updateBuild: (...args: unknown[]) => updateBuild(...args),
  previewCreateBuildCandidate: (...args: unknown[]) => previewCreateBuildCandidate(...args),
}));

function build(overrides: Partial<Build> = {}): Build {
  return { id: "B", revision: 7, name: "Linked", ...overrides } as Build;
}

beforeEach(() => {
  for (const fn of [
    getBuild,
    setBuildBlueprintSelection,
    setBuildFacility,
    setBuildPricing,
    updateBuild,
    previewCreateBuildCandidate,
  ]) {
    fn.mockReset();
  }
  getBuild.mockResolvedValue(build());
});
afterEach(() => vi.restoreAllMocks());

describe("useBuildSettings", () => {
  it("loads the build by id and never touches the whole-draft editor path", async () => {
    const { result } = renderHook(() => useBuildSettings("B"));
    await waitFor(() => expect(result.current.build?.id).toBe("B"));
    expect(getBuild).toHaveBeenCalledWith("B");
    expect(updateBuild).not.toHaveBeenCalled();
    expect(previewCreateBuildCandidate).not.toHaveBeenCalled();
  });

  it("patches the blueprint selection by build id with the current revision, then adopts the result", async () => {
    setBuildBlueprintSelection.mockResolvedValue(build({ revision: 8 }));
    const onChanged = vi.fn();
    const { result } = renderHook(() => useBuildSettings("B", onChanged));
    await waitFor(() => expect(result.current.build?.revision).toBe(7));

    await act(async () => {
      await result.current.updateBlueprintSelection({
        mode: "manual",
        kind: "original",
        materialEfficiency: 10,
        timeEfficiency: 20,
        licensedRuns: null,
        notes: "",
      });
    });

    expect(setBuildBlueprintSelection).toHaveBeenCalledWith("B", {
      expectedRevision: 7,
      blueprintSelection: {
        mode: "manual",
        kind: "original",
        materialEfficiency: 10,
        timeEfficiency: 20,
        licensedRuns: null,
        notes: "",
      },
    });
    expect(result.current.build?.revision).toBe(8);
    expect(onChanged).toHaveBeenCalledTimes(1);
  });

  it("routes facility and pricing patches to their own narrow endpoints", async () => {
    setBuildFacility.mockResolvedValue(build({ revision: 8 }));
    setBuildPricing.mockResolvedValue(build({ revision: 9 }));
    const { result } = renderHook(() => useBuildSettings("B"));
    await waitFor(() => expect(result.current.build?.revision).toBe(7));

    await act(async () => {
      await result.current.updateFacility({ facilityProfileId: "fac-1" });
    });
    expect(setBuildFacility).toHaveBeenCalledWith("B", {
      expectedRevision: 7,
      facilityProfileId: "fac-1",
      estimatedItemValue: undefined,
    });

    await act(async () => {
      await result.current.updatePricing({
        materialScope: { regionId: 10_000_043 },
        outputScope: { regionId: 10_000_002 },
        materialPricingPolicy: "highestBuy",
        outputPricingPolicy: "lowestSell",
      });
    });
    expect(setBuildPricing).toHaveBeenCalledWith(
      "B",
      expect.objectContaining({ expectedRevision: 8, materialPricingPolicy: "highestBuy" }),
    );
    expect(result.current.build?.revision).toBe(9);
  });

  it("surfaces a failed patch as an error and keeps the last-known build", async () => {
    setBuildPricing.mockRejectedValue(new Error("revision conflict"));
    const { result } = renderHook(() => useBuildSettings("B"));
    await waitFor(() => expect(result.current.build?.revision).toBe(7));

    await act(async () => {
      await result.current.updatePricing({
        materialScope: { regionId: 1 },
        outputScope: { regionId: 1 },
        materialPricingPolicy: "highestBuy",
        outputPricingPolicy: "lowestSell",
      });
    });

    expect(result.current.error).toMatch(/revision conflict/i);
    expect(result.current.build?.revision).toBe(7);
  });
});
