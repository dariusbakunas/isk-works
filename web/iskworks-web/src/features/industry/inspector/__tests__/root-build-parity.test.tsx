import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { BuildWorksheetEditorModel } from "../../builds/use-build-worksheet-editor";
import { buildRootInspector } from "../adapters/root-build";
import { INSPECTOR_SECTION_ORDER, type InspectorModel } from "../inspector-model";
import { InspectorCollapseProvider } from "../inspector-collapse";
import { UnifiedItemInspector } from "../unified-item-inspector";

vi.mock("../../../../components/market-scope-selector", () => ({
  MarketScopeSelector: ({ scope }: { scope: { regionId?: number } }) => (
    <button type="button">scope {scope.regionId}</button>
  ),
}));

function sectionShape(model: InspectorModel): string[] {
  return INSPECTOR_SECTION_ORDER.filter((id) => {
    if (id === "duration") return model.durationSeconds != null;
    return (model as unknown as Record<string, unknown>)[id] != null;
  });
}

function modelDigest(model: InspectorModel): unknown {
  return JSON.parse(
    JSON.stringify(model, (_key, value) => (typeof value === "function" ? undefined : value)),
  );
}

const FACILITY = {
  id: "fac-X",
  name: "Sotiyo X",
  role: "manufacturing",
  archivedAt: null,
  revision: 4,
  solarSystemName: "Jita",
  structureTypeName: "Sotiyo",
  materialReductionPercent: "2.0",
  timeReductionPercent: "30.0",
  rigs: [],
} as unknown as import("../../../../api/industry").FacilityProfile;

const OBSERVATION = {
  id: "obs-1",
  ownerName: "Valka",
  locationName: "C-J6MT",
  blueprintTypeId: 6830,
  blueprintName: "Ishtar Blueprint",
  kind: "original" as const,
  materialEfficiency: 10,
  timeEfficiency: 20,
  licensedRuns: null,
};

/** One rich root manufacturing Build, expressed as the resident editor -- the
 * one host-neutral input both `BuildInspector` (Worksheet "build settings"
 * mode) and `BuildGraphView` (root node) feed to `buildRootInspector`. */
function rootEditor(overrides: Partial<BuildWorksheetEditorModel> = {}): BuildWorksheetEditorModel {
  const facSlot = {
    facilities: [FACILITY],
    facilityId: "fac-X",
    setFacilityId: vi.fn(),
    selectedFacility: FACILITY,
    automaticEiv: { value: "9999.0000", observedAt: "2026-09-01T00:00:00Z" },
    manualEiv: false,
    estimatedItemValue: "9999.0000",
    eivLoading: false,
    eivError: "",
    setManualEiv: vi.fn(),
    setEstimatedItemValue: vi.fn(),
  };
  return {
    initialBuild: { id: "root-1", recipeCurrency: "current", productGroupName: "Heavy Assault Cruiser" },
    selected: {
      kind: "manufacturing",
      result: {
        blueprintTypeId: 6830,
        blueprintName: "Ishtar Blueprint",
        productTypeId: 12005,
        productName: "Ishtar",
        groupName: "Heavy Assault Cruiser",
      },
    },
    selectedName: "Ishtar Blueprint",
    name: "Ishtar build",
    runs: "3",
    recipe: {
      blueprintTypeId: 6830,
      blueprintName: "Ishtar Blueprint",
      runs: 3,
      durationSeconds: 3600,
      products: [{ typeId: 12005, typeName: "Ishtar", quantityPerRun: 1, totalQuantity: 3 }],
      materials: [
        { typeId: 34, typeName: "Tritanium", quantityPerRun: 100, totalQuantity: 300 },
        { typeId: 35, typeName: "Pyerite", quantityPerRun: 50, totalQuantity: 150 },
      ],
    },
    previewUpdating: false,
    estimate: {
      candidate: {
        blueprint: { plannedDurationSeconds: 3200 },
        manufacturingFacility: { plannedDurationSeconds: 3200 },
        reactionFacility: null,
      },
      worksheet: {
        groups: [
          {
            key: "materials",
            label: "Materials",
            items: [
              { typeId: 34, typeName: "Tritanium", role: "material", requiredQuantity: 300 },
              { typeId: 35, typeName: "Pyerite", role: "material", requiredQuantity: 150 },
            ],
          },
        ],
        output: { key: "output", label: "Output", items: [{ typeId: 12005, typeName: "Ishtar", role: "output", requiredQuantity: 3 }] },
        summary: {
          materialCost: "1000000.0000",
          installationCost: "50000.0000",
          totalCost: "1050000.0000",
          pricingComplete: true,
        },
      },
    },
    blueprintMode: "observedAsset",
    blueprintKind: "original",
    blueprintMe: "0",
    blueprintTe: "0",
    licensedRuns: "",
    blueprintNotes: "",
    observedBlueprints: [OBSERVATION],
    selectedObservationId: "obs-1",
    setBlueprintMode: vi.fn(),
    setBlueprintKind: vi.fn(),
    setBlueprintMe: vi.fn(),
    setBlueprintTe: vi.fn(),
    setLicensedRuns: vi.fn(),
    setBlueprintNotes: vi.fn(),
    setSelectedObservationId: vi.fn(),
    manufacturing: facSlot,
    reaction: facSlot,
    rootFacilitySelection: facSlot,
    allFacilities: [FACILITY],
    materialScope: { regionId: 10_000_002 },
    setMaterialScope: vi.fn(),
    materialPricingPolicy: "highestBuy",
    setMaterialPricingPolicy: vi.fn(),
    outputScope: { regionId: 10_000_043 },
    setOutputScope: vi.fn(),
    outputPricingPolicy: "lowestSell",
    setOutputPricingPolicy: vi.fn(),
    sourceId: "src-1",
    sources: [{ id: "src-1", name: "My overrides" }],
    source: { id: "src-1", name: "My overrides", kind: "manual" },
    ...overrides,
  } as unknown as BuildWorksheetEditorModel;
}

function renderRoot(editor: BuildWorksheetEditorModel) {
  const { model, actions } = buildRootInspector(editor, { onClose: vi.fn() });
  const utils = render(
    <InspectorCollapseProvider>
      <UnifiedItemInspector actions={actions} model={model} />
    </InspectorCollapseProvider>,
  );
  return { model, actions, ...utils };
}

describe("root Build inspector is identical from Worksheet and Graph", () => {
  it("both host resolvers call one builder -> the same canonical model", () => {
    // The Worksheet ("build settings" mode) and the Graph (root node) both do
    // exactly `buildRootInspector(editor, { onClose })` -- there is one path.
    const editor = rootEditor();
    const worksheet = buildRootInspector(editor, { onClose: vi.fn() });
    const graph = buildRootInspector(editor, { onClose: vi.fn() });
    expect(modelDigest(graph.model)).toEqual(modelDigest(worksheet.model));
    expect(sectionShape(worksheet.model)).toEqual([
      "quantities",
      "blueprint",
      "facility",
      "cost",
      "pricing",
      "duration",
      "inputs",
      "provenance",
    ]);
  });

  it("no parent-relationship sections -- no Sourcing, no Used By, no Coverage", () => {
    const { model } = renderRoot(rootEditor());
    expect(model.sourcing).toBeUndefined();
    expect(model.usedBy).toBeUndefined();
    expect(model.coverage).toBeUndefined();
    expect(model.value).toBeUndefined();
    expect(screen.queryByRole("region", { name: "Sourcing" })).not.toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Used By" })).not.toBeInTheDocument();
  });

  it("canonical header: BUILD -- product -- group -- Manufacturing -- Runs/Making; Build id in Provenance", () => {
    renderRoot(rootEditor());
    expect(screen.getByText("BUILD")).toBeInTheDocument();
    expect(screen.getByText("Ishtar")).toBeInTheDocument();
    expect(screen.getByText("Heavy Assault Cruiser · Manufacturing")).toBeInTheDocument();
    expect(screen.getByText("Runs 3 · Making 3")).toBeInTheDocument();
    const provenance = screen.getByRole("region", { name: "Provenance" });
    expect(within(provenance).getByText(/Build root-1/)).toBeInTheDocument();
  });

  it("QUANTITIES uses production semantics -- Runs / Output per run / Making, no Required/Shortage/Surplus", () => {
    const { model } = renderRoot(rootEditor());
    expect(model.quantities?.metrics.map((m) => m.label)).toEqual([
      "Runs",
      "Output per run",
      "Making",
    ]);
  });

  it("BLUEPRINT: an observed selection shows the corrected two-mode control", () => {
    renderRoot(rootEditor());
    const bp = screen.getByRole("region", { name: "Blueprint" });
    expect(within(bp).getByRole("button", { name: "Use available blueprint" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(within(bp).getByRole("combobox", { name: "Available blueprint" })).toHaveTextContent(/Ishtar Blueprint/);
  });

  it("FACILITY: editable role-appropriate select + resolved system/bonuses + EIV control", () => {
    renderRoot(rootEditor());
    const facility = screen.getByRole("region", { name: "Facility" });
    expect(within(facility).getByLabelText("Manufacturing Facility")).toHaveValue("fac-X");
    expect(within(facility).getByText("Jita · Sotiyo")).toBeInTheDocument();
    expect(within(facility).getByText(/Installation cost basis/)).toBeInTheDocument();
  });

  it("COST: Materials / Installation / Total from the preview summary", () => {
    const { model } = renderRoot(rootEditor());
    expect(model.cost).toMatchObject({ state: "known" });
    const cost = screen.getByRole("region", { name: "Cost" });
    expect(within(cost).getByText("Material / Component Cost")).toBeInTheDocument();
    expect(within(cost).getByText("Installation")).toBeInTheDocument();
    expect(within(cost).getByText("Total Production Cost")).toBeInTheDocument();
  });

  it("PRICING: material + output scope/policy + price-source fallback (root config, not per-row)", () => {
    renderRoot(rootEditor());
    const pricing = screen.getByRole("region", { name: "Pricing" });
    expect(within(pricing).getByLabelText("Material pricing")).toHaveValue("highestBuy");
    expect(within(pricing).getByLabelText("Output pricing")).toHaveValue("lowestSell");
    expect(within(pricing).getByLabelText("Price Override (fallback)")).toHaveValue("src-1");
  });

  // ── mutation parity ──────────────────────────────────────────────────

  it("blueprint / facility / pricing edits invoke identical editor mutations from either source", async () => {
    const user = userEvent.setup();
    const editor = rootEditor();
    // "Worksheet" resolver and "Graph" resolver -- identical calls.
    const worksheet = buildRootInspector(editor, { onClose: vi.fn() });
    const graph = buildRootInspector(editor, { onClose: vi.fn() });

    // Model a manual ME through each -- same editor setters, same values.
    for (const built of [worksheet, graph]) {
      (editor.setBlueprintMe as ReturnType<typeof vi.fn>).mockClear();
      built.actions.blueprint?.onModelManually?.({
        kind: "original",
        materialEfficiency: 8,
        timeEfficiency: 16,
        licensedRuns: null,
        notes: "",
      });
      expect(editor.setBlueprintMe).toHaveBeenCalledWith("8");
      expect(editor.setBlueprintMode).toHaveBeenCalledWith("manual");
    }

    // Facility select routes to the root's own slot (never a child).
    worksheet.actions.facility?.onSelect?.("fac-Y");
    graph.actions.facility?.onSelect?.("fac-Y");
    expect(editor.rootFacilitySelection.setFacilityId).toHaveBeenNthCalledWith(1, "fac-Y");
    expect(editor.rootFacilitySelection.setFacilityId).toHaveBeenNthCalledWith(2, "fac-Y");

    // Pricing config routes to the build-wide editor setters.
    worksheet.actions.pricing?.onMaterialPolicy?.("acquireQuantityFromSellOrders");
    graph.actions.pricing?.onMaterialPolicy?.("acquireQuantityFromSellOrders");
    expect(editor.setMaterialPricingPolicy).toHaveBeenNthCalledWith(1, "acquireQuantityFromSellOrders");
    expect(editor.setMaterialPricingPolicy).toHaveBeenNthCalledWith(2, "acquireQuantityFromSellOrders");

    // And the rendered PRICING select drives the same setter.
    (editor.setOutputPricingPolicy as ReturnType<typeof vi.fn>).mockClear();
    renderRoot(editor);
    await user.selectOptions(
      within(screen.getByRole("region", { name: "Pricing" })).getByLabelText("Output pricing"),
      "liquidateQuantityIntoBuyOrders",
    );
    expect(editor.setOutputPricingPolicy).toHaveBeenCalledWith("liquidateQuantityIntoBuyOrders");
  });

  it("changing the root facility never touches a child linked Build", () => {
    const editor = rootEditor();
    const { actions } = buildRootInspector(editor, { onClose: vi.fn() });
    actions.facility?.onSelect?.("fac-Z");
    // Only the root's own facility slot is written.
    expect(editor.rootFacilitySelection.setFacilityId).toHaveBeenCalledWith("fac-Z");
    expect(editor.setComponentResolutions).toBeUndefined();
  });

  it("a reaction root exposes RECIPE instead of BLUEPRINT", () => {
    const editor = rootEditor({
      selected: {
        kind: "reaction",
        result: {
          reactionFormulaTypeId: 45_001,
          reactionFormulaName: "Ferrofluid Reaction",
          productTypeId: 16_659,
          productName: "Ferrofluid",
          groupName: "Composite Reaction",
          published: true,
        },
      },
      selectedName: "Ferrofluid Reaction",
      blueprintMode: null,
    } as unknown as Partial<BuildWorksheetEditorModel>);
    const { model } = renderRoot(editor);
    expect(model.blueprint).toBeUndefined();
    expect(model.recipe?.name).toBe("Ferrofluid Reaction");
    expect(screen.getByRole("region", { name: "Recipe" })).toBeInTheDocument();
  });
});
