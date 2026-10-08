import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, test, vi } from "vitest";

import type { CreateBuildPlanPreview, WorksheetItem } from "../../../../../api/industry";
import type { BuildWorksheetEditorModel } from "../../use-build-worksheet-editor";
import { BuildInspector } from "../build-inspector";

vi.mock("../../../../../components/market-scope-selector", () => ({
  MarketScopeSelector: () => <button type="button">scope</button>,
}));

function outputItem(overrides: Partial<WorksheetItem> = {}): WorksheetItem {
  return {
    typeId: 23913,
    typeName: "Thanatos",
    role: "output",
    requiredQuantity: 1,
    availableQuantity: 0,
    coveredQuantity: 0,
    missingQuantity: 0,
    coveragePercentage: "0.00",
    projectedInventoryCost: null,
    pricing: {
      selectionKind: "default",
      effectivePolicy: "lowestSell",
      unitPrice: null,
      manualUnitPrice: null,
      missing: true,
      sourceNote: "",
    },
    lineTotal: null,
    contributions: [],
    isBuildResolved: false,
    installationCost: null,
    ...overrides,
  };
}

function worksheetWith(items: WorksheetItem[]): CreateBuildPlanPreview["worksheet"] {
  return {
    groups: [],
    output: { key: "output", label: "Output", items },
    summary: {
      materialCost: "0.0000",
      installationCost: null,
      totalCost: null,
      expectedRevenue: null,
      estimatedMargin: null,
      pricingComplete: false,
      quantityCoverageComplete: true,
      costCoverageComplete: true,
      warnings: [],
    },
  } as unknown as CreateBuildPlanPreview["worksheet"];
}

function fakeEditor(overrides: Partial<BuildWorksheetEditorModel> = {}): BuildWorksheetEditorModel {
  const facilitySlot = {
    facilities: [],
    facilityId: "",
    setFacilityId: vi.fn(),
    automaticEiv: null,
    eivError: "",
    eivLoading: false,
    estimatedItemValue: "",
    manualEiv: false,
    selectedFacility: undefined,
    setEstimatedItemValue: vi.fn(),
    setManualEiv: vi.fn(),
    reset: vi.fn(),
  };
  return {
    inspectorMode: { kind: "closed" },
    closeInspector: vi.fn(),
    openBuildSettings: vi.fn(),
    selectWorksheetRow: vi.fn(),
    selected: {
      kind: "manufacturing",
      result: {
        blueprintTypeId: 1,
        blueprintName: "BP",
        productTypeId: 100,
        productName: "Widget",
        groupName: "Component",
      },
    },
    selectedName: "BP",
    name: "Widget build",
    runs: "1",
    recipe: null,
    previewUpdating: false,
    blueprintMode: null,
    observedBlueprints: [],
    selectedObservationId: "",
    manufacturing: facilitySlot,
    reaction: facilitySlot,
    rootFacilitySelection: facilitySlot,
    materialScope: { regionId: 1, locationId: 2 },
    setMaterialScope: vi.fn(),
    materialPricingPolicy: "highestBuy",
    setMaterialPricingPolicy: vi.fn(),
    outputScope: { regionId: 1, locationId: 2 },
    setOutputScope: vi.fn(),
    outputPricingPolicy: "lowestSell",
    setOutputPricingPolicy: vi.fn(),
    sourceId: "",
    setSourceId: vi.fn(),
    sources: [],
    source: undefined,
    estimate: null,
    allFacilities: [],
    initialBuild: null,
    componentResolutions: {},
    fulfillmentScopes: {},
    buildLinkedComponent: vi.fn(),
    setPrices: vi.fn(),
    setItemPricingPolicies: vi.fn(),
    setComponentResolutions: vi.fn(),
    setFulfillmentScopes: vi.fn(),
    ...overrides,
  } as unknown as BuildWorksheetEditorModel;
}

// `PlannerInspectorShell` portals into a pre-existing `#app-right-rail`
// (rendered by the app layout, always mounted before any page). Mirror that
// by attaching the rail to `document.body` before the inspector mounts.
function mountRail() {
  const rail = document.createElement("div");
  rail.id = "app-right-rail";
  rail.dataset.testid = "rail";
  document.body.appendChild(rail);
  return rail;
}

afterEach(() => {
  document.getElementById("app-right-rail")?.remove();
});

function renderInspector(editor: BuildWorksheetEditorModel) {
  mountRail();
  return render(
    <MemoryRouter>
      <BuildInspector editor={editor} />
    </MemoryRouter>,
  );
}

describe("BuildInspector state machine", () => {
  test("closed mode renders nothing into the rail", () => {
    renderInspector(fakeEditor({ inspectorMode: { kind: "closed" } }));

    expect(screen.getByTestId("rail")).toBeEmptyDOMElement();
    expect(screen.queryByRole("complementary")).not.toBeInTheDocument();
  });

  test("buildSettings mode mounts the canonical root inspector into #app-right-rail", () => {
    renderInspector(fakeEditor({ inspectorMode: { kind: "buildSettings" } }));

    const inspector = screen.getByRole("complementary", { name: "Build settings" });
    expect(screen.getByTestId("rail")).toContainElement(inspector);
    // The canonical root sections -- BuildSettingsPanel is decomposed into
    // FACILITY (facility + EIV) and PRICING (scopes + policies + source).
    expect(within(inspector).getByRole("region", { name: "Facility" })).toBeInTheDocument();
    expect(within(inspector).getByRole("region", { name: "Cost" })).toBeInTheDocument();
    expect(within(inspector).getByRole("region", { name: "Pricing" })).toBeInTheDocument();
    expect(within(inspector).getByRole("region", { name: "Material acquisition" })).toBeInTheDocument();
    expect(within(inspector).getByRole("region", { name: "Output valuation" })).toBeInTheDocument();
    expect(within(inspector).getByLabelText("Manufacturing Facility")).toBeInTheDocument();
    expect(within(inspector).getByLabelText("Price Override (fallback)")).toBeInTheDocument();
  });

  test("selectedItem mode renders the item inspector, not the settings panel", () => {
    renderInspector(fakeEditor({
      inspectorMode: { kind: "selectedItem", rowKey: "output:23913" },
      estimate: { worksheet: worksheetWith([outputItem()]) } as unknown as BuildWorksheetEditorModel["estimate"],
    }));

    expect(screen.getByRole("complementary", { name: "Thanatos" })).toBeInTheDocument();
    expect(screen.queryByRole("complementary", { name: "Build settings" })).not.toBeInTheDocument();
  });

  test("Build Settings and Selected Item never mount together -- switching replaces", () => {
    const base = fakeEditor({
      inspectorMode: { kind: "selectedItem", rowKey: "output:23913" },
      estimate: { worksheet: worksheetWith([outputItem()]) } as unknown as BuildWorksheetEditorModel["estimate"],
    });
    const { rerender } = renderInspector(base);

    expect(screen.getByRole("complementary", { name: "Thanatos" })).toBeInTheDocument();

    rerender(
      <MemoryRouter>
        <BuildInspector editor={fakeEditor({ ...base, inspectorMode: { kind: "buildSettings" } })} />
      </MemoryRouter>,
    );

    expect(screen.queryByRole("complementary", { name: "Thanatos" })).not.toBeInTheDocument();
    expect(screen.getByRole("complementary", { name: "Build settings" })).toBeInTheDocument();
  });

  test("a stored rowKey missing from a freshly returned worksheet stops rendering the item", () => {
    const editor = fakeEditor({
      inspectorMode: { kind: "selectedItem", rowKey: "output:23913" },
      estimate: { worksheet: worksheetWith([]) } as unknown as BuildWorksheetEditorModel["estimate"],
    });
    renderInspector(editor);

    expect(screen.queryByRole("complementary", { name: "Thanatos" })).not.toBeInTheDocument();
    expect(screen.getByTestId("rail")).toBeEmptyDOMElement();
  });

  test("X closes via closeInspector and settings changes go straight to the live setter", async () => {
    const user = userEvent.setup();
    const closeInspector = vi.fn();
    const setMaterialPricingPolicy = vi.fn();
    renderInspector(fakeEditor({
      inspectorMode: { kind: "buildSettings" },
      closeInspector,
      setMaterialPricingPolicy,
    }));

    await user.selectOptions(screen.getByLabelText("Material pricing"), "acquireQuantityFromSellOrders");
    expect(setMaterialPricingPolicy).toHaveBeenCalledWith("acquireQuantityFromSellOrders");
    expect(screen.queryByRole("button", { name: /Apply|Save/ })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Close inspector" }));
    expect(closeInspector).toHaveBeenCalledTimes(1);
  });

  test("Escape closes via closeInspector", async () => {
    const user = userEvent.setup();
    const closeInspector = vi.fn();
    renderInspector(fakeEditor({ inspectorMode: { kind: "buildSettings" }, closeInspector }));

    await user.keyboard("{Escape}");
    expect(closeInspector).toHaveBeenCalledTimes(1);
  });
});
