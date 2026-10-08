import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router";
import { beforeEach, describe, expect, test, vi } from "vitest";

import type { Build, FacilityProfile, ProductionWorksheet as Worksheet, WorksheetItem } from "../../../../../api/industry";
import { PlannerContextPanel } from "../planner-context-panel";
import { ProductionWorksheet, rowKey } from "../production-worksheet";

const sdeApi = vi.hoisted(() => ({
  recipeForProduct: vi.fn(),
}));

vi.mock("../../../../../api/sde", async () => {
  const actual = await vi.importActual<typeof import("../../../../../api/sde")>("../../../../../api/sde");
  return { ...actual, ...sdeApi };
});

const industryApi = vi.hoisted(() => ({
  listBlueprintObservations: vi.fn(),
  setBuildBlueprintSelection: vi.fn(),
  setBuildFacility: vi.fn(),
}));

vi.mock("../../../../../api/industry", async () => {
  const actual = await vi.importActual<typeof import("../../../../../api/industry")>("../../../../../api/industry");
  return { ...actual, ...industryApi };
});

const worksheet: Worksheet = {
  groups: [{
    key: "mineral",
    label: "Mineral",
    items: [{
      typeId: 34,
      typeName: "Tritanium",
      role: "material",
      requiredQuantity: 1000,
      availableQuantity: 900,
      coveredQuantity: 900,
      missingQuantity: 100,
      coveragePercentage: "90.00",
      projectedInventoryCost: "3000.0000",
      pricing: {
        selectionKind: "default",
        effectivePolicy: "highestBuy",
        unitPrice: "4.0000",
        manualUnitPrice: null,
        missing: false,
        sourceNote: "",
      },
      lineTotal: "4000.0000",
      contributions: [
        { parentTypeId: null, parentTypeName: "Thanatos", quantity: 60 },
        { parentTypeId: 90_001, parentTypeName: "Rifter Hull Section", quantity: 40 },
      ],
      isBuildResolved: true,
      installationCost: "1234.5000",
    }],
  }],
  output: {
    key: "output",
    label: "Output",
    items: [{
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
    }],
  },
  summary: {
    materialCost: "4000.0000",
    installationCost: null,
    totalCost: null,
    expectedRevenue: null,
    estimatedMargin: null,
    pricingComplete: false,
    quantityCoverageComplete: false,
    costCoverageComplete: true,
    warnings: [],
  },
};

function facilityProfile(overrides: Partial<FacilityProfile> = {}): FacilityProfile {
  return {
    id: "facility-1",
    workspaceId: "workspace-1",
    name: "Test Assembly Array",
    kind: "manual",
    role: "manufacturing",
    structureId: null,
    structureTypeId: null,
    structureTypeName: "",
    solarSystemId: null,
    solarSystemName: "",
    securityClass: "unknown",
    materialReductionPercent: "0",
    timeReductionPercent: "0",
    jobCostReductionPercent: "0",
    facilityTaxPercent: "0",
    sccSurchargePercent: "0",
    allianceSurchargePercent: "0",
    fixedSupplementalCost: "0.0000",
    manualSystemCostIndex: "0.05",
    notes: "",
    rigs: [],
    archivedAt: null,
    revision: 1,
    createdAt: "2026-07-27T10:00:00Z",
    updatedAt: "2026-07-27T10:00:00Z",
    ...overrides,
  };
}

const manufacturingFacility = facilityProfile({
  id: "facility-mfg",
  name: "Test Assembly Array",
  role: "manufacturing",
  revision: 3,
});
const reactionFacility = facilityProfile({
  id: "facility-rxn",
  name: "Test Athanor",
  role: "reaction",
  revision: 5,
});

function linkedBuildFixture(overrides: Partial<Build> = {}): Build {
  return {
    id: "linked-build-1",
    workspaceId: "workspace-1",
    ownerId: "owner-1",
    name: "Rifter Hull Section build",
    recipe: {
      kind: "manufacturing",
      sourceSdeDatasetId: "dataset-1",
      sourceSdeVersion: "3389399",
      blueprintTypeId: 57_516,
      blueprintName: "Rifter Hull Section Blueprint",
      durationSecondsPerRun: 300,
      materials: [],
      products: [{ typeId: 34, typeName: "Tritanium", quantityPerRun: 1, sortOrder: 0 }],
      fingerprint: "recipe",
    },
    runs: 2,
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
    ...overrides,
  };
}

describe("ProductionWorksheet", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    industryApi.listBlueprintObservations.mockResolvedValue([]);
    industryApi.setBuildBlueprintSelection.mockImplementation(async (id: string) =>
      linkedBuildFixture({ id, revision: 2 }),
    );
    industryApi.setBuildFacility.mockImplementation(async (id: string) =>
      linkedBuildFixture({ id, revision: 2 }),
    );
  });

  test("shows a linked build's own blueprint read-only -- Stages, not Worksheet, edits descendant configuration", async () => {
    sdeApi.recipeForProduct.mockResolvedValue({ mode: "manufacturing", blueprintTypeId: 57_516 });
    const onBlueprintSelectionChange = vi.fn();
    const onLinkedBuildChanged = vi.fn();
    render(
      <MemoryRouter>
        <PlannerContextPanel
          buildId="build-1"
          buildResolvedTypeIds={new Set([34])}
          linkedBuildsByTypeId={{
            34: linkedBuildFixture({
              id: "linked-build-9",
              revision: 1,
              draftPlanning: {
                input: {
                  materialScope: { regionId: 10_000_002, locationId: 60_003_760 },
                  outputScope: { regionId: 10_000_002, locationId: 60_003_760 },
                  manualPriceListId: null,
                  expectedManualPriceListRevision: null,
                  materialPricingPolicy: "highestBuy",
                  outputPricingPolicy: "lowestSell",
                  pricingSelections: [],
                  blueprintSelection: {
                    mode: "manual", kind: "original", materialEfficiency: 4, timeEfficiency: 8, licensedRuns: null, notes: "",
                  },
                  manufacturingFacility: null,
                  reactionFacility: null,
                  facilityEivManual: false,
                  componentResolutions: [],
                },
                updatedAt: "2026-08-05T00:00:00Z",
              },
            }),
          }}
          onBlueprintSelectionChange={onBlueprintSelectionChange}
          onLinkedBuildChanged={onLinkedBuildChanged}
          onPricingChange={vi.fn()}
          onResolutionChange={vi.fn()}
          selectedItem={worksheet.groups[0].items[0]}
          worksheet={worksheet}
        />
      </MemoryRouter>,
    );

    const blueprint = await screen.findByRole("region", { name: "Blueprint" });
    expect(within(blueprint).getByText("ME 4 · TE 8")).toBeInTheDocument();
    expect(within(blueprint).queryByLabelText("Material Efficiency (0-10)")).toBeNull();

    // Nothing patches on render -- editing this descendant's own
    // configuration now happens in Stages, never here.
    expect(industryApi.setBuildBlueprintSelection).not.toHaveBeenCalled();
    expect(onBlueprintSelectionChange).not.toHaveBeenCalled();
  });

  test("selects a role-aware item row", async () => {
    const onSelectRow = vi.fn();
    render(<ProductionWorksheet onSelectRow={onSelectRow} selectedRowKey={null} worksheet={worksheet} />);

    for (const heading of [
      "Item", "Sourcing", "Required", "Available", "Covered", "Shortage",
      "Coverage", "Pricing", "Unit Cost", "Total Value",
    ]) {
      expect(screen.getByRole("columnheader", { name: heading })).toBeInTheDocument();
    }
    expect(screen.getByRole("button", { name: "Collapse Mineral" })).toHaveTextContent("Mineral");
    expect(screen.getByRole("row", { name: /Tritanium/ })).toHaveTextContent("90.00%");
    // This fixture row is build-resolved -- Pricing reads "Production", not
    // a market policy label (see the Worksheet presentation tests below).
    expect(screen.getByRole("row", { name: /Tritanium/ })).toHaveTextContent("Production");
    await userEvent.click(screen.getByRole("row", { name: /Tritanium/ }));
    expect(onSelectRow).toHaveBeenCalledWith("material:34");
    expect(screen.getAllByRole("row").at(-1)).toHaveTextContent("Thanatos");
  });

  describe("Unit Cost presentation for a Build/Reaction-resolved row", () => {
    // Minimal, self-contained fixtures -- a self-produced row intentionally
    // has
    // `pricing.unitPrice: null` while `pricing.missing: false` and
    // `lineTotal` fully known. `unitPrice`'s mere absence must never be read
    // as "this row's cost is missing" -- only `pricing.missing` may say that.
    function materialItem(overrides: Partial<WorksheetItem> = {}): WorksheetItem {
      return {
        typeId: 90_001,
        typeName: "Rifter Hull Section",
        role: "material",
        requiredQuantity: 2,
        availableQuantity: 0,
        coveredQuantity: 0,
        missingQuantity: 2,
        coveragePercentage: "0.00",
        projectedInventoryCost: null,
        pricing: {
          selectionKind: "default",
          effectivePolicy: "highestBuy",
          unitPrice: null,
          manualUnitPrice: null,
          missing: false,
          sourceNote: "",
        },
        lineTotal: "892.5000",
        contributions: [],
        isBuildResolved: true,
        installationCost: null,
        ...overrides,
      };
    }

    function worksheetWith(item: WorksheetItem): Worksheet {
      return {
        groups: [{ key: "component", label: "Component", items: [item] }],
        output: { key: "output", label: "Output", items: [] },
        summary: {
          materialCost: item.lineTotal ?? "0.0000",
          installationCost: null,
          totalCost: null,
          expectedRevenue: null,
          estimatedMargin: null,
          pricingComplete: !item.pricing.missing,
          quantityCoverageComplete: item.missingQuantity === 0,
          costCoverageComplete: true,
          warnings: [],
        },
      };
    }

    test("complete Build row shows an effective Unit Cost, never 'Missing price'", () => {
      const item = materialItem();
      render(<ProductionWorksheet onSelectRow={vi.fn()} selectedRowKey={null} worksheet={worksheetWith(item)} />);

      const row = screen.getByRole("row", { name: /Rifter Hull Section/ });
      expect(within(row).queryByText(/Missing price/)).not.toBeInTheDocument();
      expect(within(row).queryByText(/Missing cost/)).not.toBeInTheDocument();
      // 892.5000 / 2 = 446.25
      expect(within(row).getByTitle(/446\.25 ISK/)).toBeInTheDocument();
      expect(row).toHaveTextContent("Production");
    });

    test("surplus-producing Build row's Unit Cost excludes retained surplus basis", () => {
      const item = materialItem({
        requiredQuantity: 500,
        lineTotal: "500000.0000",
        planningEvidence: {
          childOpIndex: 1,
          childProducedQuantity: 1_000,
          childConsumedQuantity: 500,
          childUnitProductionCost: "1000.0000",
          childConsumedCost: "500000.0000",
          childSurplusQuantity: 500,
          childSurplusRetainedBasis: "500000.0000",
        },
      });
      render(<ProductionWorksheet onSelectRow={vi.fn()} selectedRowKey={null} worksheet={worksheetWith(item)} />);

      const row = screen.getByRole("row", { name: /Rifter Hull Section/ });
      // 500,000 / 500 = 1,000 -- coincides with childUnitProductionCost here
      // only because there's no partial inventory in this fixture (see the
      // partial-inventory test below for a case where they differ).
      expect(within(row).getByTitle(/1,000 ISK/)).toBeInTheDocument();
      // Total Value must never include the 500,000 retained surplus basis.
      expect(row).toHaveTextContent("500K");
      expect(row).not.toHaveTextContent("1M");
    });

    test("partial inventory + Build: Unit Cost is the row's blended effective cost, not the child's own production unit cost", () => {
      // required 1,426; 143 from inventory @ 5.0000 = 715.0000; 1,283 built
      // via the child @ 6.5000/unit = 8,339.5000; lineTotal = 715 + 8339.5 =
      // 9,054.5000. Deliberately a different basis than the child's own
      // production cost, so the row's effective unit cost (9054.5/1426 =
      // 6.35) cannot accidentally coincide with childUnitProductionCost
      // (6.50) -- this test exists specifically to catch that substitution.
      const item = materialItem({
        requiredQuantity: 1_426,
        lineTotal: "9054.5000",
        reusedQuantity: 143,
        reusedLineTotal: "715.0000",
        planningEvidence: {
          childOpIndex: 1,
          childProducedQuantity: 1_283,
          childConsumedQuantity: 1_283,
          childUnitProductionCost: "6.5000",
          childConsumedCost: "8339.5000",
          childSurplusQuantity: 0,
          childSurplusRetainedBasis: "0.0000",
        },
      });

      render(<ProductionWorksheet onSelectRow={vi.fn()} selectedRowKey={null} worksheet={worksheetWith(item)} />);

      const row = screen.getByRole("row", { name: /Rifter Hull Section/ });
      // 9054.5000 / 1426 = 6.35 exactly -- the cell's own primary figure --
      // must NOT be 6.50 (childUnitProductionCost, a different, smaller-scope
      // number that's only ever secondary evidence in the tooltip).
      expect(within(row).getByText("6.35")).toBeInTheDocument();
      expect(within(row).getByTitle(/6\.35 ISK\/unit/)).toBeInTheDocument();
    });

    test("fully inventory-covered Build row shows the historical basis as Unit Cost, never 'Missing price'", () => {
      const item = materialItem({
        requiredQuantity: 500,
        missingQuantity: 0,
        availableQuantity: 500,
        coveredQuantity: 500,
        coveragePercentage: "100.00",
        lineTotal: "2500.0000",
        reusedQuantity: 500,
        reusedLineTotal: "2500.0000",
      });
      render(<ProductionWorksheet onSelectRow={vi.fn()} selectedRowKey={null} worksheet={worksheetWith(item)} />);

      const row = screen.getByRole("row", { name: /Rifter Hull Section/ });
      expect(within(row).queryByText(/Missing price/)).not.toBeInTheDocument();
      // 2500 / 500 = 5
      expect(within(row).getByText("5")).toBeInTheDocument();
      expect(within(row).getByTitle(/5 ISK\/unit/)).toBeInTheDocument();
      expect(row).toHaveTextContent("Production");
    });

    test("genuinely incomplete Build row shows 'Missing cost', not a fabricated Unit Cost", () => {
      const item = materialItem({
        pricing: {
          selectionKind: "default",
          effectivePolicy: "highestBuy",
          unitPrice: null,
          manualUnitPrice: null,
          missing: true,
          sourceNote: "",
        },
        lineTotal: null,
      });
      render(<ProductionWorksheet onSelectRow={vi.fn()} selectedRowKey={null} worksheet={worksheetWith(item)} />);

      const row = screen.getByRole("row", { name: /Rifter Hull Section/ });
      expect(within(row).getByText("Missing cost")).toBeInTheDocument();
      expect(within(row).queryByText(/^Missing price$/)).not.toBeInTheDocument();
    });

    test("healthy Buy row is unaffected -- shows its blended unit price as before", () => {
      const item = materialItem({
        isBuildResolved: false,
        requiredQuantity: 1_000,
        pricing: {
          selectionKind: "default",
          effectivePolicy: "highestBuy",
          unitPrice: "6.8000",
          manualUnitPrice: null,
          missing: false,
          sourceNote: "",
        },
        lineTotal: "6800.0000",
      });
      render(<ProductionWorksheet onSelectRow={vi.fn()} selectedRowKey={null} worksheet={worksheetWith(item)} />);

      const row = screen.getByRole("row", { name: /Rifter Hull Section/ });
      expect(within(row).getByTitle(/^6\.8 ISK$/)).toBeInTheDocument();
      // Pricing label is untouched for a Buy row.
      expect(row).toHaveTextContent("Default");
      expect(row).not.toHaveTextContent("Production");
    });

    test("genuinely missing Buy price is unaffected -- still shows 'Missing price'", () => {
      const item = materialItem({
        isBuildResolved: false,
        pricing: {
          selectionKind: "default",
          effectivePolicy: "highestBuy",
          unitPrice: null,
          manualUnitPrice: null,
          missing: true,
          sourceNote: "",
        },
        lineTotal: null,
      });
      render(<ProductionWorksheet onSelectRow={vi.fn()} selectedRowKey={null} worksheet={worksheetWith(item)} />);

      const row = screen.getByRole("row", { name: /Rifter Hull Section/ });
      expect(within(row).getByText("Missing price")).toBeInTheDocument();
    });

    test("zero required quantity never divides by zero -- shows the neutral placeholder", () => {
      const item = materialItem({ requiredQuantity: 0, missingQuantity: 0 });
      render(<ProductionWorksheet onSelectRow={vi.fn()} selectedRowKey={null} worksheet={worksheetWith(item)} />);

      const row = screen.getByRole("row", { name: /Rifter Hull Section/ });
      expect(within(row).queryByText(/Missing/)).not.toBeInTheDocument();
      // Column order is Item, Sourcing, Required, Available, Covered,
      // Shortage, Coverage, Pricing, Unit Cost, Total Value -- Unit Cost is
      // the 9th cell. Scoped to that one cell since Shortage also renders
      // "—" for a zero-missing-quantity row.
      const unitCostCell = within(row).getAllByRole("cell")[8];
      expect(within(unitCostCell).getByText("—")).toBeInTheDocument();
    });

    test("output row is unaffected -- still shows the sale-side unit price and expected revenue", () => {
      const output: WorksheetItem = {
        typeId: 41_249,
        typeName: "Fusion Reactor Unit",
        role: "output",
        requiredQuantity: 34,
        availableQuantity: 0,
        coveredQuantity: 0,
        missingQuantity: 0,
        coveragePercentage: "0.00",
        projectedInventoryCost: null,
        pricing: {
          selectionKind: "default",
          effectivePolicy: "lowestSell",
          unitPrice: "105800.0000",
          manualUnitPrice: null,
          missing: false,
          sourceNote: "",
        },
        lineTotal: "3597200.0000",
        contributions: [],
        isBuildResolved: false,
        installationCost: null,
      };
      const sheet: Worksheet = {
        groups: [],
        output: { key: "output", label: "Output", items: [output] },
        summary: {
          materialCost: "0.0000",
          installationCost: null,
          totalCost: null,
          expectedRevenue: "3597200.0000",
          estimatedMargin: null,
          pricingComplete: true,
          quantityCoverageComplete: true,
          costCoverageComplete: true,
          warnings: [],
        },
      };
      render(<ProductionWorksheet onSelectRow={vi.fn()} selectedRowKey={null} worksheet={sheet} />);

      const row = screen.getByRole("row", { name: /Fusion Reactor Unit/ });
      expect(within(row).getByTitle(/expected sale unit price/)).toHaveTextContent("105.8K");
      expect(row).toHaveTextContent("3.6M");
    });
  });

  test("keeps large quantity columns aligned and exposes full values", () => {
    const largeWorksheet = structuredClone(worksheet);
    largeWorksheet.groups[0].items[0].requiredQuantity = 18_203_393;
    largeWorksheet.groups[0].items[0].availableQuantity = 8_892_337;
    largeWorksheet.groups[0].items[0].coveredQuantity = 8_892_337;
    largeWorksheet.groups[0].items[0].missingQuantity = 9_311_056;

    render(<ProductionWorksheet onSelectRow={vi.fn()} selectedRowKey={null} worksheet={largeWorksheet} />);

    const row = screen.getByRole("row", { name: /Tritanium/ });
    const fullRequired = within(row).getByTitle("18,203,393");
    expect(fullRequired).toHaveTextContent("18.2M");
    expect(fullRequired).toHaveTextContent("18,203,393");
    expect(fullRequired.closest("td")).toHaveClass("text-right", "tabular-nums");
    expect(
      screen.getByRole("table", { name: "Production worksheet" }).style.getPropertyValue("--operational-columns"),
    ).toContain("112px 112px 112px 112px");
  });

  test("shows the fulfillment strategy label and full sentence for a Missing-scoped row", () => {
    const missingScopedWorksheet = structuredClone(worksheet);
    missingScopedWorksheet.groups[0].items[0].reusedQuantity = 900;
    missingScopedWorksheet.groups[0].items[0].reusedLineTotal = "3600.0000";

    render(<ProductionWorksheet onSelectRow={vi.fn()} selectedRowKey={null} worksheet={missingScopedWorksheet} />);

    const row = screen.getByRole("row", { name: /Tritanium/ });
    const cell = within(row).getByText("Build · Missing (100)");
    expect(cell).toHaveAttribute("title", "Use 900 from inventory and build 100.");
  });

  test("shows 'Use Inventory' as the strategy label when a Missing-scoped row is fully covered", () => {
    const fullyCoveredWorksheet = structuredClone(worksheet);
    fullyCoveredWorksheet.groups[0].items[0].reusedQuantity = 1000;
    fullyCoveredWorksheet.groups[0].items[0].reusedLineTotal = "4000.0000";

    render(<ProductionWorksheet onSelectRow={vi.fn()} selectedRowKey={null} worksheet={fullyCoveredWorksheet} />);

    const row = screen.getByRole("row", { name: /Tritanium/ });
    expect(within(row).getByText("Use Inventory")).toBeInTheDocument();
  });

  test("shows summary until an item is selected and applies manual output pricing", async () => {
    const onPricingChange = vi.fn();
    const { rerender } = render(
      <PlannerContextPanel onPricingChange={onPricingChange} selectedItem={null} worksheet={worksheet} />,
    );
    expect(screen.getByRole("heading", { name: "Plan Summary" })).toBeInTheDocument();

    const output = worksheet.output.items[0];
    rerender(<PlannerContextPanel onPricingChange={onPricingChange} selectedItem={output} worksheet={worksheet} />);
    expect(screen.getByRole("complementary", { name: "Thanatos" })).toBeInTheDocument();
    const coverage = screen.getByRole("region", { name: "Coverage" });
    expect(within(coverage).getByText("Required")).toBeInTheDocument();
    expect(within(coverage).getByText("Available")).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Provenance" }).querySelector("details")).not.toHaveAttribute("open");
    await userEvent.click(screen.getByRole("radio", { name: "Manual price" }));
    await userEvent.type(screen.getByLabelText("Unit price"), "3420000000");

    await waitFor(() => expect(onPricingChange).toHaveBeenLastCalledWith({
        typeId: 23913,
        role: "output",
        selection: { kind: "manual", unit_price: "3420000000" },
      }));
    expect(rowKey(output)).toBe("output:23913");
  });

  test("shows which parents consume a build-resolved material and how much", () => {
    const material = worksheet.groups[0].items[0];
    render(
      <PlannerContextPanel onPricingChange={vi.fn()} selectedItem={material} worksheet={worksheet} />,
    );

    const usedBy = screen.getByRole("region", { name: "Used By" });
    expect(within(usedBy).getByText("Thanatos")).toBeInTheDocument();
    expect(within(usedBy).getByText("60")).toBeInTheDocument();
    expect(within(usedBy).getByText("Rifter Hull Section")).toBeInTheDocument();
    expect(within(usedBy).getByText("40")).toBeInTheDocument();
    // Root contribution (no parentTypeId) falls back to the worksheet's
    // output type for its icon; a component contribution uses its own.
    expect(within(usedBy).getByAltText("Thanatos")).toBeInTheDocument();
    expect(within(usedBy).getByAltText("Rifter Hull Section")).toBeInTheDocument();
  });

  test("hides the Used By section for rows outside of a component expansion", () => {
    const output = worksheet.output.items[0];
    render(
      <PlannerContextPanel onPricingChange={vi.fn()} selectedItem={output} worksheet={worksheet} />,
    );

    expect(screen.queryByRole("region", { name: "Used By" })).not.toBeInTheDocument();
  });

  test("applies an individual material policy for market-backed sources", async () => {
    const onPricingChange = vi.fn();
    const material = worksheet.groups[0].items[0];
    render(
      <PlannerContextPanel
        allowMarketPolicyOverride
        onPricingChange={onPricingChange}
        selectedItem={material}
        worksheet={worksheet}
      />,
    );

    await userEvent.click(screen.getByRole("radio", { name: "Pricing policy override" }));
    await userEvent.selectOptions(screen.getByLabelText("Material pricing policy"), "acquireQuantityFromSellOrders");

    expect(onPricingChange).toHaveBeenLastCalledWith({
      typeId: 34,
      role: "material",
      selection: { kind: "market_policy", policy: "acquireQuantityFromSellOrders" },
    });
  });

  test("hides material policy overrides for manual price sources", () => {
    render(
      <PlannerContextPanel
        onPricingChange={vi.fn()}
        selectedItem={worksheet.groups[0].items[0]}
        worksheet={worksheet}
      />,
    );

    expect(screen.queryByRole("radio", { name: "Pricing policy override" })).not.toBeInTheDocument();
  });

  test("offers a Buy toggle but no Build option for a genuine raw material", async () => {
    sdeApi.recipeForProduct.mockResolvedValue(null);
    render(
      <PlannerContextPanel
        onPricingChange={vi.fn()}
        onResolutionChange={vi.fn()}
        selectedItem={worksheet.groups[0].items[0]}
        worksheet={worksheet}
      />,
    );

    await waitFor(() => expect(sdeApi.recipeForProduct).toHaveBeenCalledWith(34));
    expect(await screen.findByRole("radio", { name: "Buy" })).toBeChecked();
    expect(screen.queryByRole("radio", { name: "Build" })).not.toBeInTheDocument();
  });

  test("offers 'Use Inventory' for a genuine raw material that's fully covered", async () => {
    sdeApi.recipeForProduct.mockResolvedValue(null);
    const fullyCoveredItem = {
      ...worksheet.groups[0].items[0],
      availableQuantity: 1000,
      coveredQuantity: 1000,
      missingQuantity: 0,
      coveragePercentage: "100.00",
    };
    render(
      <PlannerContextPanel
        onPricingChange={vi.fn()}
        onResolutionChange={vi.fn()}
        selectedItem={fullyCoveredItem}
        worksheet={worksheet}
      />,
    );

    expect(await screen.findByRole("radio", { name: "Use Inventory" })).toBeChecked();
    expect(screen.queryByRole("radio", { name: "Build" })).not.toBeInTheDocument();
  });

  test("offers a buy/build toggle when the material has a producible recipe", async () => {
    const recipe = { mode: "manufacturing" as const, blueprintTypeId: 57_516 };
    sdeApi.recipeForProduct.mockResolvedValue(recipe);
    const onResolutionChange = vi.fn();
    render(
      <PlannerContextPanel
        onPricingChange={vi.fn()}
        onResolutionChange={onResolutionChange}
        selectedItem={worksheet.groups[0].items[0]}
        worksheet={worksheet}
      />,
    );

    await waitFor(() => expect(screen.getByRole("radio", { name: "Build" })).toBeInTheDocument());
    expect(screen.getByRole("radio", { name: "Buy" })).toBeChecked();
    await userEvent.click(screen.getByRole("radio", { name: "Build" }));
    expect(onResolutionChange).toHaveBeenCalledWith(34, recipe);
  });

  test("shows build as already selected when the row is already build-resolved", async () => {
    const recipe = { mode: "manufacturing" as const, blueprintTypeId: 57_516 };
    sdeApi.recipeForProduct.mockResolvedValue(recipe);
    const onResolutionChange = vi.fn();
    render(
      <PlannerContextPanel
        buildResolvedTypeIds={new Set([34])}
        onPricingChange={vi.fn()}
        onResolutionChange={onResolutionChange}
        selectedItem={worksheet.groups[0].items[0]}
        worksheet={worksheet}
      />,
    );

    await waitFor(() => expect(screen.getByRole("radio", { name: "Build" })).toBeChecked());
    await userEvent.click(screen.getByRole("radio", { name: "Buy" }));
    expect(onResolutionChange).toHaveBeenCalledWith(34, null);
  });

  // Linked-build create / reuse / scope-resync moved out of this panel into
  // `useBuildWorksheetEditor` (so it runs while the inspector is closed).
  // The panel is now purely presentational about it: it renders whatever
  // linked-build state the editor threads down and never issues a lookup or
  // creation itself. See `use-build-worksheet-editor.test.tsx` for the
  // lifecycle behaviour.

  test("is presentational about linked builds -- renders the editor's link and never looks one up", async () => {
    const recipe = { mode: "manufacturing" as const, blueprintTypeId: 57_516 };
    sdeApi.recipeForProduct.mockResolvedValue(recipe);
    render(
      <MemoryRouter>
        <PlannerContextPanel
          buildId="build-1"
          buildResolvedTypeIds={new Set([34])}
          linkedBuildsByTypeId={{ 34: linkedBuildFixture({ id: "linked-build-2" }) }}
          onPricingChange={vi.fn()}
          onResolutionChange={vi.fn()}
          selectedItem={worksheet.groups[0].items[0]}
          worksheet={worksheet}
        />
      </MemoryRouter>,
    );

    expect(
      await screen.findByRole("button", { name: "Open linked build for Tritanium" }),
    ).toBeInTheDocument();
  });

  test("gates the 'View linked build' link on a persisted buildId", async () => {
    const recipe = { mode: "manufacturing" as const, blueprintTypeId: 57_516 };
    sdeApi.recipeForProduct.mockResolvedValue(recipe);
    render(
      <MemoryRouter>
        <PlannerContextPanel
          buildResolvedTypeIds={new Set([34])}
          linkedBuildsByTypeId={{ 34: linkedBuildFixture() }}
          onPricingChange={vi.fn()}
          onResolutionChange={vi.fn()}
          selectedItem={worksheet.groups[0].items[0]}
          worksheet={worksheet}
        />
      </MemoryRouter>,
    );

    await waitFor(() => expect(screen.getByRole("radio", { name: "Build" })).toBeChecked());
    expect(screen.queryByRole("link", { name: "View linked build →" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Open linked build/ })).not.toBeInTheDocument();
  });

  test("shows 'Creating linked build...' while the editor is create-or-reusing this row's link", async () => {
    const recipe = { mode: "manufacturing" as const, blueprintTypeId: 57_516 };
    sdeApi.recipeForProduct.mockResolvedValue(recipe);
    render(
      <MemoryRouter>
        <PlannerContextPanel
          buildId="build-1"
          buildResolvedTypeIds={new Set([34])}
          linkedBuildPending={{ 34: true }}
          onPricingChange={vi.fn()}
          onResolutionChange={vi.fn()}
          selectedItem={worksheet.groups[0].items[0]}
          worksheet={worksheet}
        />
      </MemoryRouter>,
    );

    expect(await screen.findByText("Creating linked build…")).toBeInTheDocument();
  });

  test("shows the linked build's own facility and blueprint read-only instead of editable controls", async () => {
    const recipe = { mode: "manufacturing" as const, blueprintTypeId: 57_516 };
    sdeApi.recipeForProduct.mockResolvedValue(recipe);
    const linked = linkedBuildFixture({
      id: "linked-build-3",
      draftPlanning: {
        input: {
          materialScope: { regionId: 10_000_002, locationId: 60_003_760 },
          outputScope: { regionId: 10_000_002, locationId: 60_003_760 },
          manualPriceListId: null,
          expectedManualPriceListRevision: null,
          materialPricingPolicy: "highestBuy",
          outputPricingPolicy: "lowestSell",
          pricingSelections: [],
          blueprintSelection: {
            mode: "manual", kind: "original", materialEfficiency: 10, timeEfficiency: 20, licensedRuns: null, notes: "",
          },
          manufacturingFacility: {
            facilityProfileId: "facility-mfg", blueprintMe: 10, blueprintTe: 20, estimatedItemValue: null,
          },
          reactionFacility: null,
          facilityEivManual: false,
          componentResolutions: [],
        },
        updatedAt: "2026-08-05T00:00:00Z",
      },
    });
    render(
      <MemoryRouter>
        <PlannerContextPanel
          allFacilities={[manufacturingFacility, reactionFacility]}
          buildId="build-1"
          buildResolvedTypeIds={new Set([34])}
          linkedBuildsByTypeId={{ 34: linked }}
          onPricingChange={vi.fn()}
          onResolutionChange={vi.fn()}
          selectedItem={worksheet.groups[0].items[0]}
          worksheet={worksheet}
        />
      </MemoryRouter>,
    );

    // Stages owns descendant production configuration editing -- a
    // linked Build's own blueprint / facility show read-only here, with
    // navigation to open the linked build directly.
    expect(await screen.findByRole("button", { name: "Open linked build for Tritanium" })).toBeInTheDocument();
    const blueprint = screen.getByRole("region", { name: "Blueprint" });
    expect(within(blueprint).getByText("ME 10 · TE 20")).toBeInTheDocument();
    expect(within(blueprint).queryByLabelText("Material Efficiency (0-10)")).toBeNull();
    expect(within(blueprint).queryByLabelText("Time Efficiency (0-20)")).toBeNull();
    const facility = screen.getByRole("region", { name: "Facility" });
    expect(within(facility).queryByLabelText("Facility")).toBeNull();
  });

  test("renders the linked-build error the editor reports", async () => {
    const recipe = { mode: "manufacturing" as const, blueprintTypeId: 57_516 };
    sdeApi.recipeForProduct.mockResolvedValue(recipe);
    render(
      <MemoryRouter>
        <PlannerContextPanel
          buildId="build-1"
          buildResolvedTypeIds={new Set([34])}
          linkedBuildErrors={{ 34: "Could not create the linked build. Try again." }}
          onPricingChange={vi.fn()}
          onResolutionChange={vi.fn()}
          selectedItem={worksheet.groups[0].items[0]}
          worksheet={worksheet}
        />
      </MemoryRouter>,
    );

    expect(await screen.findByText("Could not create the linked build. Try again.")).toBeInTheDocument();
  });

  test("offers 'Use Inventory' as the default when the row is fully covered by inventory", async () => {
    const recipe = { mode: "manufacturing" as const, blueprintTypeId: 57_516 };
    sdeApi.recipeForProduct.mockResolvedValue(recipe);
    const fullyCoveredItem = {
      ...worksheet.groups[0].items[0],
      availableQuantity: 1000,
      coveredQuantity: 1000,
      missingQuantity: 0,
      coveragePercentage: "100.00",
    };
    render(
      <PlannerContextPanel
        onPricingChange={vi.fn()}
        onResolutionChange={vi.fn()}
        selectedItem={fullyCoveredItem}
        worksheet={worksheet}
      />,
    );

    expect(await screen.findByRole("radio", { name: "Use Inventory" })).toBeChecked();
    expect(screen.getByRole("radio", { name: "Buy" })).not.toBeChecked();
  });

  test("does not offer 'Use Inventory' when the row has a shortfall", async () => {
    const recipe = { mode: "manufacturing" as const, blueprintTypeId: 57_516 };
    sdeApi.recipeForProduct.mockResolvedValue(recipe);
    render(
      <PlannerContextPanel
        onPricingChange={vi.fn()}
        onResolutionChange={vi.fn()}
        selectedItem={worksheet.groups[0].items[0]}
        worksheet={worksheet}
      />,
    );

    await waitFor(() => expect(screen.getByRole("radio", { name: "Buy" })).toBeInTheDocument());
    expect(screen.queryByRole("radio", { name: "Use Inventory" })).not.toBeInTheDocument();
  });

  test("selecting 'Use Inventory' clears the build resolution and any fulfillment scope override", async () => {
    const recipe = { mode: "manufacturing" as const, blueprintTypeId: 57_516 };
    sdeApi.recipeForProduct.mockResolvedValue(recipe);
    const fullyCoveredItem = {
      ...worksheet.groups[0].items[0],
      availableQuantity: 1000,
      coveredQuantity: 1000,
      missingQuantity: 0,
      coveragePercentage: "100.00",
    };
    const onResolutionChange = vi.fn();
    const onFulfillmentScopeChange = vi.fn();
    render(
      <PlannerContextPanel
        buildResolvedTypeIds={new Set([34])}
        onFulfillmentScopeChange={onFulfillmentScopeChange}
        onPricingChange={vi.fn()}
        onResolutionChange={onResolutionChange}
        selectedItem={fullyCoveredItem}
        worksheet={worksheet}
      />,
    );

    await userEvent.click(await screen.findByRole("radio", { name: "Use Inventory" }));
    expect(onResolutionChange).toHaveBeenCalledWith(34, null);
    expect(onFulfillmentScopeChange).toHaveBeenCalledWith(34, null);
  });

  test("selecting 'Buy' on a fully-covered row sets an explicit Full scope so it doesn't snap back to 'Use Inventory'", async () => {
    const recipe = { mode: "manufacturing" as const, blueprintTypeId: 57_516 };
    sdeApi.recipeForProduct.mockResolvedValue(recipe);
    const fullyCoveredItem = {
      ...worksheet.groups[0].items[0],
      availableQuantity: 1000,
      coveredQuantity: 1000,
      missingQuantity: 0,
      coveragePercentage: "100.00",
    };
    const onFulfillmentScopeChange = vi.fn();
    render(
      <PlannerContextPanel
        onFulfillmentScopeChange={onFulfillmentScopeChange}
        onPricingChange={vi.fn()}
        onResolutionChange={vi.fn()}
        selectedItem={fullyCoveredItem}
        worksheet={worksheet}
      />,
    );

    await userEvent.click(await screen.findByRole("radio", { name: "Buy" }));
    expect(onFulfillmentScopeChange).toHaveBeenCalledWith(34, "full");
  });

  test("shows 'Buy' as selected on a fully-covered row that's explicitly Full-scoped", async () => {
    const recipe = { mode: "manufacturing" as const, blueprintTypeId: 57_516 };
    sdeApi.recipeForProduct.mockResolvedValue(recipe);
    const fullyCoveredItem = {
      ...worksheet.groups[0].items[0],
      availableQuantity: 1000,
      coveredQuantity: 1000,
      missingQuantity: 0,
      coveragePercentage: "100.00",
    };
    render(
      <PlannerContextPanel
        fulfillmentScopes={{ 34: "full" }}
        onPricingChange={vi.fn()}
        onResolutionChange={vi.fn()}
        selectedItem={fullyCoveredItem}
        worksheet={worksheet}
      />,
    );

    expect(await screen.findByRole("radio", { name: "Buy" })).toBeChecked();
    expect(screen.getByRole("radio", { name: "Use Inventory" })).not.toBeChecked();
  });

  test("offers a Quantity sub-choice when Build is selected and there's a shortfall, defaulting to 'Shortage only'", async () => {
    const recipe = { mode: "manufacturing" as const, blueprintTypeId: 57_516 };
    sdeApi.recipeForProduct.mockResolvedValue(recipe);
    const onFulfillmentScopeChange = vi.fn();
    render(
      <MemoryRouter>
        <PlannerContextPanel
          buildId="build-1"
          buildResolvedTypeIds={new Set([34])}
          onFulfillmentScopeChange={onFulfillmentScopeChange}
          onPricingChange={vi.fn()}
          onResolutionChange={vi.fn()}
          selectedItem={worksheet.groups[0].items[0]}
          worksheet={worksheet}
        />
      </MemoryRouter>,
    );

    expect(await screen.findByRole("radio", { name: "Shortage only" })).toBeChecked();
    await userEvent.click(screen.getByRole("radio", { name: "Full requirement" }));
    expect(onFulfillmentScopeChange).toHaveBeenCalledWith(34, "full");
  });

  test("offers a role-filtered facility picker once a row is build-resolved", async () => {
    const recipe = { mode: "manufacturing" as const, blueprintTypeId: 57_516 };
    sdeApi.recipeForProduct.mockResolvedValue(recipe);
    render(
      <PlannerContextPanel
        allFacilities={[manufacturingFacility, reactionFacility]}
        buildResolvedTypeIds={new Set([34])}
        onPricingChange={vi.fn()}
        onResolutionChange={vi.fn()}
        selectedItem={worksheet.groups[0].items[0]}
        worksheet={worksheet}
      />,
    );

    const select = await screen.findByRole("combobox", { name: "Facility" });
    const options = within(select).getAllByRole("option").map((option) => option.textContent);
    expect(options).toEqual(["Use build facility", "Test Assembly Array"]);
  });

  test("selecting a per-row facility calls onFacilityOverrideChange with the profile id and revision", async () => {
    const recipe = { mode: "manufacturing" as const, blueprintTypeId: 57_516 };
    sdeApi.recipeForProduct.mockResolvedValue(recipe);
    const onFacilityOverrideChange = vi.fn();
    render(
      <PlannerContextPanel
        allFacilities={[manufacturingFacility, reactionFacility]}
        buildResolvedTypeIds={new Set([34])}
        onFacilityOverrideChange={onFacilityOverrideChange}
        onPricingChange={vi.fn()}
        onResolutionChange={vi.fn()}
        selectedItem={worksheet.groups[0].items[0]}
        worksheet={worksheet}
      />,
    );

    const select = await screen.findByRole("combobox", { name: "Facility" });
    await userEvent.selectOptions(select, "Test Assembly Array");

    expect(onFacilityOverrideChange).toHaveBeenCalledWith(34, {
      facilityProfileId: "facility-mfg",
    });
  });

  test("selecting 'Use build facility' clears the override", async () => {
    const recipe = { mode: "manufacturing" as const, blueprintTypeId: 57_516 };
    sdeApi.recipeForProduct.mockResolvedValue(recipe);
    const onFacilityOverrideChange = vi.fn();
    render(
      <PlannerContextPanel
        allFacilities={[manufacturingFacility, reactionFacility]}
        buildResolvedTypeIds={new Set([34])}
        facilityOverrides={{ 34: { facilityProfileId: "facility-mfg" } }}
        onFacilityOverrideChange={onFacilityOverrideChange}
        onPricingChange={vi.fn()}
        onResolutionChange={vi.fn()}
        selectedItem={worksheet.groups[0].items[0]}
        worksheet={worksheet}
      />,
    );

    const select = await screen.findByRole("combobox", { name: "Facility" });
    await userEvent.selectOptions(select, "Use build facility");

    expect(onFacilityOverrideChange).toHaveBeenCalledWith(34, null);
  });

  test("shows the row's own facility name next to installation cost when overridden", async () => {
    const recipe = { mode: "manufacturing" as const, blueprintTypeId: 57_516 };
    sdeApi.recipeForProduct.mockResolvedValue(recipe);
    render(
      <PlannerContextPanel
        allFacilities={[manufacturingFacility, reactionFacility]}
        buildResolvedTypeIds={new Set([34])}
        facilityOverrides={{ 34: { facilityProfileId: "facility-mfg" } }}
        onPricingChange={vi.fn()}
        onResolutionChange={vi.fn()}
        selectedItem={worksheet.groups[0].items[0]}
        worksheet={worksheet}
      />,
    );

    await waitFor(() => expect(screen.getByRole("region", { name: "Value" })).toHaveTextContent("Test Assembly Array"));
  });

  test("offers an edit-blueprint control only for a build-resolved manufacturing row", async () => {
    sdeApi.recipeForProduct.mockResolvedValue({ mode: "manufacturing", blueprintTypeId: 57_516 });
    render(
      <PlannerContextPanel
        buildResolvedTypeIds={new Set([34])}
        onPricingChange={vi.fn()}
        onResolutionChange={vi.fn()}
        selectedItem={worksheet.groups[0].items[0]}
        worksheet={worksheet}
      />,
    );

    const blueprint = await screen.findByRole("region", { name: "Blueprint" });
    expect(within(blueprint).getByRole("button", { name: "Enter manually" })).toBeInTheDocument();
    expect(within(blueprint).getByLabelText("Material Efficiency (0-10)")).toBeInTheDocument();
  });

  test("does not offer an edit-blueprint control for a reaction row", async () => {
    sdeApi.recipeForProduct.mockResolvedValue({ mode: "reaction", reactionFormulaTypeId: 200 });
    render(
      <PlannerContextPanel
        buildResolvedTypeIds={new Set([34])}
        onPricingChange={vi.fn()}
        onResolutionChange={vi.fn()}
        selectedItem={worksheet.groups[0].items[0]}
        worksheet={worksheet}
      />,
    );

    await waitFor(() => expect(screen.getByRole("radio", { name: "Build" })).toBeChecked());
    expect(screen.queryByRole("region", { name: "Blueprint" })).not.toBeInTheDocument();
  });

  test("submitting a manual blueprint selection calls onBlueprintSelectionChange", async () => {
    sdeApi.recipeForProduct.mockResolvedValue({ mode: "manufacturing", blueprintTypeId: 57_516 });
    const onBlueprintSelectionChange = vi.fn();
    render(
      <PlannerContextPanel
        buildResolvedTypeIds={new Set([34])}
        onBlueprintSelectionChange={onBlueprintSelectionChange}
        onPricingChange={vi.fn()}
        onResolutionChange={vi.fn()}
        selectedItem={worksheet.groups[0].items[0]}
        worksheet={worksheet}
      />,
    );

    const blueprint = await screen.findByRole("region", { name: "Blueprint" });
    const me = within(blueprint).getByLabelText("Material Efficiency (0-10)");
    await userEvent.clear(me);
    await userEvent.type(me, "10");
    await userEvent.tab();

    await waitFor(() =>
      expect(onBlueprintSelectionChange).toHaveBeenCalledWith(34, {
        mode: "manual",
        kind: "original",
        materialEfficiency: 10,
        timeEfficiency: 0,
        licensedRuns: null,
        notes: "",
      }),
    );
  });

  test("shows the current blueprint selection as a summary", async () => {
    sdeApi.recipeForProduct.mockResolvedValue({ mode: "manufacturing", blueprintTypeId: 57_516 });
    render(
      <PlannerContextPanel
        blueprintSelections={{
          34: {
            mode: "manual",
            kind: "original",
            materialEfficiency: 10,
            timeEfficiency: 20,
            licensedRuns: null,
            notes: "",
          },
        }}
        buildResolvedTypeIds={new Set([34])}
        onPricingChange={vi.fn()}
        onResolutionChange={vi.fn()}
        selectedItem={worksheet.groups[0].items[0]}
        worksheet={worksheet}
      />,
    );

    const blueprint = await screen.findByRole("region", { name: "Blueprint" });
    expect(within(blueprint).getByLabelText("Material Efficiency (0-10)")).toHaveValue("10");
    expect(within(blueprint).getByLabelText("Time Efficiency (0-20)")).toHaveValue("20");
  });

  test("does not check for a producible recipe in read-only mode", () => {
    render(
      <PlannerContextPanel
        onPricingChange={vi.fn()}
        readOnly
        selectedItem={worksheet.groups[0].items[0]}
        worksheet={worksheet}
      />,
    );

    expect(sdeApi.recipeForProduct).not.toHaveBeenCalled();
    expect(screen.queryByRole("region", { name: "Sourcing" })).not.toBeInTheDocument();
  });
});
