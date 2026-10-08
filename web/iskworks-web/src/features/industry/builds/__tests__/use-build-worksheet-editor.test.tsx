import { act, renderHook, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { beforeEach, describe, expect, test, vi } from "vitest";

import type { Build, CreateBuildPlanPreview } from "../../../../api/industry";
import type { BuildPlan } from "../../../../api/sde";

const sdeApi = vi.hoisted(() => ({
  getSdeStatus: vi.fn(),
  planBuild: vi.fn(),
  planReaction: vi.fn(),
}));
vi.mock("../../../../api/sde", async () => {
  const actual = await vi.importActual<typeof import("../../../../api/sde")>("../../../../api/sde");
  return { ...actual, ...sdeApi };
});

const industryApi = vi.hoisted(() => ({
  listPriceSources: vi.fn(),
  listFacilities: vi.fn(),
  listBlueprintObservations: vi.fn(),
  createLinkedBuild: vi.fn(),
  previewCreateBuildCandidate: vi.fn(),
  updateBuild: vi.fn(),
  getBlueprintAutomaticEiv: vi.fn(),
}));
vi.mock("../../../../api/industry", async () => {
  const actual = await vi.importActual<typeof import("../../../../api/industry")>("../../../../api/industry");
  return { ...actual, ...industryApi };
});

import { useBuildWorksheetEditor } from "../use-build-worksheet-editor";

function initialBuild(): Build {
  return {
    id: "build-1",
    workspaceId: "workspace-1",
    ownerId: "owner-1",
    name: "Rifter build",
    recipe: {
      kind: "manufacturing",
      sourceSdeDatasetId: "dataset-1",
      sourceSdeVersion: "3389399",
      blueprintTypeId: 691,
      blueprintName: "Rifter Blueprint",
      durationSecondsPerRun: 300,
      materials: [{ typeId: 34, typeName: "Tritanium", quantityPerRun: 100, sortOrder: 0 }],
      products: [{ typeId: 587, typeName: "Rifter", quantityPerRun: 1, sortOrder: 0 }],
      fingerprint: "recipe",
    },
    runs: 1,
    notes: "",
    revision: 1,
    createdAt: "2026-08-05T00:00:00Z",
    updatedAt: "2026-08-05T00:00:00Z",
    draftPlanning: null,
    recipeCurrency: "current",
    activeSdeVersion: "3389399",
    productCategoryName: null,
    productGroupName: null,
    selectedBlueprintOrigin: null,
    hasOwnedBlueprint: false,
  } as unknown as Build;
}

function previewFixture(tag: string): CreateBuildPlanPreview {
  return {
    candidateFingerprint: tag,
    canPlan: true,
    validation: { fields: [], blockers: [] },
    warnings: [],
    worksheet: {
      groups: [{
        key: "mineral",
        label: "Mineral",
        items: [{
          typeId: 34,
          typeName: "Tritanium",
          role: "material",
          requiredQuantity: 100,
          availableQuantity: 0,
          coveredQuantity: 0,
          missingQuantity: 100,
          coveragePercentage: "0.00",
          projectedInventoryCost: null,
          pricing: { selectionKind: "default", effectivePolicy: "highestBuy", unitPrice: "5.0000", manualUnitPrice: null, missing: false, sourceNote: "" },
          lineTotal: "500.0000",
          contributions: [],
          isBuildResolved: false,
          installationCost: null,
        }],
      }],
      output: { key: "output", label: "Output", items: [] },
      summary: {
        materialCost: "500.0000",
        installationCost: null,
        totalCost: null,
        expectedRevenue: null,
        estimatedMargin: null,
        pricingComplete: false,
        quantityCoverageComplete: false,
        costCoverageComplete: true,
        warnings: [],
      },
    },
  } as unknown as CreateBuildPlanPreview;
}

function linkedBuild(overrides: Partial<Build> = {}): Build {
  return {
    ...initialBuild(),
    id: "linked-build-1",
    name: "Tritanium build",
    runs: 2,
    ...overrides,
  } as unknown as Build;
}

const buildRecipe = { mode: "manufacturing" as const, blueprintTypeId: 691 };

function recipePlanFixture(): BuildPlan {
  return {
    blueprintTypeId: 691,
    blueprintName: "Rifter Blueprint",
    runs: 1,
    durationSeconds: 300,
    materials: [{ typeId: 34, typeName: "Tritanium", quantityPerRun: 100, totalQuantity: 100 }],
    products: [{ typeId: 587, typeName: "Rifter", quantityPerRun: 1, totalQuantity: 1 }],
  };
}

const wrapper = ({ children }: { children: React.ReactNode }) => <MemoryRouter>{children}</MemoryRouter>;

describe("useBuildWorksheetEditor -- preview isolation", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    sdeApi.getSdeStatus.mockResolvedValue({ active: true });
    sdeApi.planBuild.mockResolvedValue({
      blueprintTypeId: 691,
      blueprintName: "Rifter Blueprint",
      runs: 1,
      durationSeconds: 300,
      materials: [{ typeId: 34, typeName: "Tritanium", quantityPerRun: 100, totalQuantity: 100 }],
      products: [{ typeId: 587, typeName: "Rifter", quantityPerRun: 1, totalQuantity: 1 }],
    });
    industryApi.listPriceSources.mockResolvedValue([]);
    industryApi.listFacilities.mockResolvedValue([]);
    industryApi.listBlueprintObservations.mockResolvedValue([]);
    industryApi.createLinkedBuild.mockResolvedValue(linkedBuild());
    industryApi.getBlueprintAutomaticEiv.mockResolvedValue({ value: null, observedAt: null });
    industryApi.updateBuild.mockImplementation(async (_id, input) => ({ ...initialBuild(), ...input, revision: 2 }));
  });

  test("retains the last-known-good estimate when a later preview fails", async () => {
    const good = previewFixture("good");
    industryApi.previewCreateBuildCandidate
      .mockResolvedValueOnce(good)
      .mockRejectedValue(new Error("market sync failed"));

    const { result } = renderHook(() => useBuildWorksheetEditor(initialBuild()), { wrapper });

    await waitFor(() => expect(result.current.estimate).toBe(good), { timeout: 4000 });

    act(() => {
      result.current.setPrices({ 34: "1000000" });
    });

    await waitFor(
      () => expect(industryApi.previewCreateBuildCandidate).toHaveBeenCalledTimes(2),
      { timeout: 4000 },
    );
    await waitFor(() => expect(result.current.error).toBeTruthy(), { timeout: 4000 });

    // The previously valid worksheet must survive a transient preview failure.
    expect(result.current.estimate).toBe(good);
  });

  test("only canonical unformatted money reaches the preview request", async () => {
    industryApi.previewCreateBuildCandidate.mockResolvedValue(previewFixture("any"));

    const { result } = renderHook(() => useBuildWorksheetEditor(initialBuild()), { wrapper });
    await waitFor(() => expect(result.current.estimate).toBeTruthy(), { timeout: 4000 });

    act(() => {
      result.current.setPrices({ 34: "1000000" });
    });

    await waitFor(
      () => expect(industryApi.previewCreateBuildCandidate).toHaveBeenCalledTimes(2),
      { timeout: 4000 },
    );

    const lastArg = industryApi.previewCreateBuildCandidate.mock.calls.at(-1)?.[0];
    expect(lastArg.pricingSelections).toContainEqual({
      typeId: 34,
      role: "material",
      selection: { kind: "manual", unit_price: "1000000" },
    });
    for (const selection of lastArg.pricingSelections) {
      if (selection.selection.kind === "manual") {
        expect(selection.selection.unit_price).not.toMatch(/[,\s]/);
      }
    }
  });

  test("inspectorMode is a single mutually-exclusive union across the four transitions", async () => {
    industryApi.previewCreateBuildCandidate.mockResolvedValue(previewFixture("union"));
    const { result } = renderHook(() => useBuildWorksheetEditor(initialBuild()), { wrapper });
    await waitFor(() => expect(result.current.estimate).toBeTruthy(), { timeout: 4000 });

    expect(result.current.inspectorMode).toEqual({ kind: "closed" });

    // closed -> Edit build settings -> buildSettings
    act(() => result.current.openBuildSettings());
    expect(result.current.inspectorMode).toEqual({ kind: "buildSettings" });

    // buildSettings -> click a worksheet row -> selectedItem (replaces, never merges)
    act(() => result.current.selectWorksheetRow("material:34"));
    expect(result.current.inspectorMode).toEqual({ kind: "selectedItem", rowKey: "material:34" });

    // selectedItem -> click another row -> selectedItem(new rowKey)
    act(() => result.current.selectWorksheetRow("output:587"));
    expect(result.current.inspectorMode).toEqual({ kind: "selectedItem", rowKey: "output:587" });

    // selectedItem -> Edit build settings -> buildSettings
    act(() => result.current.openBuildSettings());
    expect(result.current.inspectorMode).toEqual({ kind: "buildSettings" });

    // either open mode -> X / Escape -> closed
    act(() => result.current.closeInspector());
    expect(result.current.inspectorMode).toEqual({ kind: "closed" });

    // selectWorksheetRow(null) also closes
    act(() => result.current.selectWorksheetRow("material:34"));
    act(() => result.current.selectWorksheetRow(null));
    expect(result.current.inspectorMode).toEqual({ kind: "closed" });
  });

  test("a canonical facility EIV flows unformatted into the preview request", async () => {
    industryApi.previewCreateBuildCandidate.mockResolvedValue(previewFixture("eiv"));
    const build = {
      ...initialBuild(),
      draftPlanning: {
        input: {
          materialScope: { regionId: 10_000_002, locationId: 60_003_760 },
          outputScope: { regionId: 10_000_002, locationId: 60_003_760 },
          manualPriceListId: null,
          expectedManualPriceListRevision: null,
          materialPricingPolicy: "highestBuy",
          outputPricingPolicy: "lowestSell",
          facilityEivManual: true,
          pricingSelections: [],
          blueprintSelection: null,
          manufacturingFacility: {
            facilityProfileId: "fac-1",
            blueprintMe: 0,
            blueprintTe: 0,
            estimatedItemValue: "500000.0000",
          },
          reactionFacility: null,
          componentResolutions: [],
          fulfillmentScopes: [],
        },
      },
    } as unknown as Build;
    industryApi.listFacilities.mockResolvedValue([
      {
        id: "fac-1",
        name: "Test Sotiyo",
        role: "manufacturing",
        archivedAt: null,
        revision: 1,
        rigs: [],
      },
    ]);

    const { result } = renderHook(() => useBuildWorksheetEditor(build), { wrapper });
    await waitFor(() => expect(result.current.estimate).toBeTruthy(), { timeout: 4000 });

    act(() => {
      result.current.rootFacilitySelection.setEstimatedItemValue("1000000");
    });

    await waitFor(() => {
      const arg = industryApi.previewCreateBuildCandidate.mock.calls.at(-1)?.[0];
      expect(arg?.manufacturingFacility?.estimatedItemValue).toBe("1000000");
    }, { timeout: 4000 });

    for (const call of industryApi.previewCreateBuildCandidate.mock.calls) {
      const eiv = call[0]?.manufacturingFacility?.estimatedItemValue;
      if (eiv) expect(eiv).not.toMatch(/[,\s]/);
    }
  });
});

describe("useBuildWorksheetEditor -- active linked-build lifecycle", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    sdeApi.getSdeStatus.mockResolvedValue({ active: true });
    sdeApi.planBuild.mockResolvedValue({
      blueprintTypeId: 691,
      blueprintName: "Rifter Blueprint",
      runs: 1,
      durationSeconds: 300,
      materials: [{ typeId: 34, typeName: "Tritanium", quantityPerRun: 100, totalQuantity: 100 }],
      products: [{ typeId: 587, typeName: "Rifter", quantityPerRun: 1, totalQuantity: 1 }],
    });
    industryApi.listPriceSources.mockResolvedValue([]);
    industryApi.listFacilities.mockResolvedValue([]);
    industryApi.listBlueprintObservations.mockResolvedValue([]);
    industryApi.getBlueprintAutomaticEiv.mockResolvedValue({ value: null, observedAt: null });
    industryApi.previewCreateBuildCandidate.mockResolvedValue(previewFixture("lifecycle"));
    industryApi.updateBuild.mockImplementation(async (_id, input) => ({ ...initialBuild(), ...input, revision: 2 }));
    industryApi.createLinkedBuild.mockResolvedValue(linkedBuild());
  });

  // Renders the hook and waits until its recipe has loaded, so `queueSave`
  // (which `buildLinkedComponent` runs first) can actually persist.
  async function renderReady() {
    const view = renderHook(() => useBuildWorksheetEditor(initialBuild()), { wrapper });
    await waitFor(() => expect(view.result.current.recipe).toBeTruthy(), { timeout: 4000 });
    return view;
  }

  test("does not validate or ensure linked producers before a saved Build recipe is hydrated", async () => {
    let resolveRecipe!: (value: BuildPlan) => void;
    sdeApi.planBuild.mockReturnValue(new Promise((resolve) => { resolveRecipe = resolve; }));
    const saved = {
      ...initialBuild(),
      draftPlanning: { input: { componentResolutions: [{ typeId: 34, recipe: buildRecipe }] } },
    } as unknown as Build;

    const view = renderHook(() => useBuildWorksheetEditor(saved), { wrapper });
    await waitFor(() => expect(sdeApi.planBuild).toHaveBeenCalled());

    expect(view.result.current.initializingExistingBuild).toBe(true);
    expect(view.result.current.error).toBe("");
    expect(industryApi.updateBuild).not.toHaveBeenCalled();
    expect(industryApi.createLinkedBuild).not.toHaveBeenCalled();

    act(() => resolveRecipe(recipePlanFixture()));
    await waitFor(() => expect(view.result.current.initializingExistingBuild).toBe(false));
  });

  test("exposes a saved recipe request failure as an error state", async () => {
    sdeApi.planBuild.mockRejectedValue(new Error("recipe unavailable"));

    const { result } = renderHook(() => useBuildWorksheetEditor(initialBuild()), { wrapper });

    await waitFor(() => expect(result.current.recipeStatus).toBe("error"));
    expect(result.current.initializingExistingBuild).toBe(false);
    expect(result.current.error).toContain("recipe unavailable");
  });

  test("still validates a new Build with missing required fields", async () => {
    const { result } = renderHook(() => useBuildWorksheetEditor(null), { wrapper });
    let saved: Build | null = null;

    await act(async () => { saved = await result.current.queueSave(); });

    expect(saved).toBeNull();
    expect(result.current.error).toBe("Name, blueprint, and valid runs are required.");
  });

  test("creates the linked build for a Build-resolved direct material with no inspector mounted", async () => {
    const { result } = await renderReady();

    act(() => result.current.setComponentResolutions({ 34: { recipe: buildRecipe } }));

    await waitFor(
      () => expect(industryApi.createLinkedBuild).toHaveBeenCalledWith("build-1", { componentTypeId: 34 }),
      { timeout: 4000 },
    );
    await waitFor(() => expect(result.current.linkedBuildsByTypeId[34]).toBeTruthy(), { timeout: 4000 });
  });

  test("resolves an already Build-resolved component's producer once on open", async () => {
    // A saved build whose component 34 is already Build-resolved: opening
    // it asks the server for that edge's producer (create-or-reuse, which
    // returns the existing one) exactly once.
    const resolvedBuild = {
      ...initialBuild(),
      draftPlanning: { input: { componentResolutions: [{ typeId: 34, recipe: buildRecipe }] } },
    } as unknown as Build;
    industryApi.createLinkedBuild.mockResolvedValue(linkedBuild({ id: "producer-1" }));

    const view = renderHook(() => useBuildWorksheetEditor(resolvedBuild), { wrapper });
    await waitFor(() => expect(view.result.current.recipe).toBeTruthy(), { timeout: 4000 });

    await waitFor(
      () => expect(view.result.current.linkedBuildsByTypeId[34]?.id).toBe("producer-1"),
      { timeout: 4000 },
    );
    await new Promise((resolve) => setTimeout(resolve, 100));
    expect(industryApi.createLinkedBuild).toHaveBeenCalledTimes(1);
    expect(industryApi.createLinkedBuild).toHaveBeenCalledWith("build-1", { componentTypeId: 34 });
  });

  test("does not create a linked build for a Buy-resolved material", async () => {
    const { result } = await renderReady();

    act(() => result.current.setPrices({ 34: "5" }));
    await new Promise((resolve) => setTimeout(resolve, 150));

    expect(industryApi.createLinkedBuild).not.toHaveBeenCalled();
    expect(result.current.linkedBuildsByTypeId[34]).toBeUndefined();
  });

  test("effect reruns and an in-flight creation never launch a duplicate", async () => {
    let resolveCreate!: (build: Build) => void;
    industryApi.createLinkedBuild.mockReturnValue(
      new Promise<Build>((resolve) => {
        resolveCreate = resolve;
      }),
    );
    const { result } = await renderReady();

    act(() => result.current.setComponentResolutions({ 34: { recipe: buildRecipe } }));
    await waitFor(() => expect(industryApi.createLinkedBuild).toHaveBeenCalledTimes(1), { timeout: 4000 });

    // Force several unrelated re-renders while the creation is still pending.
    act(() => result.current.setPrices({ 34: "5" }));
    act(() => result.current.setRuns("2"));
    act(() => result.current.setRuns("1"));
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(industryApi.createLinkedBuild).toHaveBeenCalledTimes(1);

    act(() => resolveCreate(linkedBuild()));
    await waitFor(() => expect(result.current.linkedBuildsByTypeId[34]).toBeTruthy(), { timeout: 4000 });
    // The resolved map entry keeps the ensure pass from re-issuing.
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(industryApi.createLinkedBuild).toHaveBeenCalledTimes(1);
  });

  test("Build -> Buy forgets the link locally without any server-side delete; Buy -> Build re-issues create-or-reuse", async () => {
    industryApi.createLinkedBuild.mockResolvedValue(linkedBuild({ id: "linked-1" }));
    const { result } = await renderReady();

    act(() => result.current.setComponentResolutions({ 34: { recipe: buildRecipe } }));
    await waitFor(() => expect(result.current.linkedBuildsByTypeId[34]?.id).toBe("linked-1"), { timeout: 4000 });
    expect(industryApi.createLinkedBuild).toHaveBeenCalledTimes(1);

    // Build -> Buy: the retained/inactive linked build stays server-side
    // (there is no delete call), it is just dropped from the local map.
    act(() => result.current.setComponentResolutions({}));
    await waitFor(() => expect(result.current.linkedBuildsByTypeId[34]).toBeUndefined(), { timeout: 4000 });
    expect(industryApi.createLinkedBuild).toHaveBeenCalledTimes(1);

    // Buy -> Build again: re-issues create-or-reuse (the server reuses the
    // retained row via the (parent, component) unique index).
    act(() => result.current.setComponentResolutions({ 34: { recipe: buildRecipe } }));
    await waitFor(() => expect(industryApi.createLinkedBuild).toHaveBeenCalledTimes(2), { timeout: 4000 });
    expect(industryApi.createLinkedBuild).toHaveBeenLastCalledWith("build-1", { componentTypeId: 34 });
  });

  test("a failed creation records an error and still allows a retry on the next Build toggle", async () => {
    industryApi.createLinkedBuild
      .mockRejectedValueOnce(new Error("boom"))
      .mockResolvedValue(linkedBuild({ id: "linked-retry" }));
    const { result } = await renderReady();

    act(() => result.current.setComponentResolutions({ 34: { recipe: buildRecipe } }));
    await waitFor(
      () => expect(result.current.linkedBuildErrors[34]).toBe("Could not create the linked build. Try again."),
      { timeout: 4000 },
    );
    expect(result.current.linkedBuildsByTypeId[34]).toBeUndefined();

    // A genuine Buy -> Build transition retries; the failure did not
    // permanently mark the component as satisfied.
    act(() => result.current.setComponentResolutions({}));
    act(() => result.current.setComponentResolutions({ 34: { recipe: buildRecipe } }));
    await waitFor(() => expect(result.current.linkedBuildsByTypeId[34]?.id).toBe("linked-retry"), { timeout: 4000 });
    expect(industryApi.createLinkedBuild).toHaveBeenCalledTimes(2);
  });

  test("changing a linked row's fulfillment scope does not re-issue create-or-reuse", async () => {
    const { result } = await renderReady();

    act(() => result.current.setComponentResolutions({ 34: { recipe: buildRecipe } }));
    await waitFor(() => expect(industryApi.createLinkedBuild).toHaveBeenCalledTimes(1), { timeout: 4000 });

    // The producer is shared and sized from aggregate demand; a scope change
    // is just a draft save, not a new create-or-reuse.
    act(() => result.current.setFulfillmentScopes({ 34: "full" }));
    await new Promise((resolve) => setTimeout(resolve, 100));
    expect(industryApi.createLinkedBuild).toHaveBeenCalledTimes(1);
  });
});
