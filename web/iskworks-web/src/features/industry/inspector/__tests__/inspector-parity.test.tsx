import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { Build, WorksheetItem } from "../../../../api/industry";
import type { ProductionEnrichment } from "../../builds/graph/graph-inspector-enrichment";
import type { BuildGraphNodeData } from "../../builds/graph/to-react-flow";
import type { BuildSettings } from "../../builds/use-build-settings";
import { buildGraphInspector } from "../adapters/graph-node";
import { buildWorksheetInspector } from "../adapters/worksheet-item";
import { INSPECTOR_SECTION_ORDER, type InspectorModel } from "../inspector-model";
import { InspectorCollapseProvider } from "../inspector-collapse";
import { UnifiedItemInspector } from "../unified-item-inspector";

/** Which canonical sections a model would render, in canonical order. */
function sectionShape(model: InspectorModel): string[] {
  return INSPECTOR_SECTION_ORDER.filter((id) => {
    if (id === "duration") return model.durationSeconds != null;
    return (model as unknown as Record<string, unknown>)[id] != null;
  });
}

function rawMaterial(overrides: Partial<WorksheetItem> = {}): WorksheetItem {
  return {
    typeId: 34,
    typeName: "Tritanium",
    role: "material",
    requiredQuantity: 100,
    availableQuantity: 20,
    coveredQuantity: 20,
    missingQuantity: 80,
    coveragePercentage: "20.00",
    projectedInventoryCost: "100.0000",
    pricing: {
      selectionKind: "default",
      effectivePolicy: "highestBuy",
      unitPrice: "5.0000",
      manualUnitPrice: null,
      missing: false,
      sourceNote: "",
    },
    lineTotal: "500.0000",
    contributions: [{ parentTypeId: null, parentTypeName: "Rifter", quantity: 100 }],
    isBuildResolved: false,
    installationCost: null,
    ...overrides,
  };
}

const linkedBuild = {
  id: "linked-1",
  revision: 3,
  recipe: {
    kind: "manufacturing",
    blueprintTypeId: 900,
    blueprintName: "Widget Blueprint",
  },
  selectedBlueprintOrigin: "original",
  draftPlanning: {
    input: {
      blueprintSelection: {
        mode: "manual",
        kind: "original",
        materialEfficiency: 10,
        timeEfficiency: 20,
        licensedRuns: null,
        notes: "",
      },
      manufacturingFacility: { facilityProfileId: "fac-1" },
      reactionFacility: null,
    },
  },
} as unknown as Build;

const enrichment: ProductionEnrichment = {
  blueprintName: "Widget Blueprint",
  blueprintOrigin: "original",
  formulaName: null,
  me: 10,
  te: 20,
  meFromOwnedBlueprint: false,
  facility: {
    id: "fac-1",
    name: "Sotiyo",
    solarSystemName: "Jita",
    structureTypeName: "Sotiyo",
    materialReductionPercent: 2,
    timeReductionPercent: 30,
    rigs: [],
  } as unknown as ProductionEnrichment["facility"],
  facilityUnresolved: false,
  plannedDurationSeconds: null,
  installationCost: "1000.0000",
  totalCost: "3000.0000",
  computing: false,
};

const graphLinkedNode: BuildGraphNodeData = {
  nodeType: "production",
  depth: 1,
  node: {
    graphNodeId: "build:linked-1",
    buildId: "linked-1",
    parentBuildId: "root",
    parentComponentTypeId: 900,
    typeId: 900,
    typeName: "Widget",
    kind: "manufacturing",
    recipe: { mode: "manufacturing", blueprintTypeId: 900 },
    runs: 4,
    persistedRuns: 4,
    requiredQuantity: 40,
    netRequiredQuantity: 40,
    producingQuantity: 40,
    surplus: 0,
    estimatedCost: "2000.0000",
    materialComponentCost: "1500.0000",
    ownInstallationCost: "500.0000",
    costState: "known",
    recipeCurrency: "current",
    effectiveMe: 10,
    effectiveTe: 20,
    children: [],
  },
} as unknown as BuildGraphNodeData;

describe("Worksheet / Graph inspector are one component", () => {
  it("both adapters feed the same UnifiedItemInspector for a raw material", async () => {
    const onPricingChange = vi.fn();
    const onResolutionChange = vi.fn();
    const worksheet = buildWorksheetInspector({
      item: rawMaterial(),
      buildResolved: false,
      linkedBuild: null,
      creatingLinkedBuild: false,
      linkedBuildError: "",
      allFacilities: [],
      rootTypeId: 500,
      recipe: null,
      observations: [],
      readOnly: false,
      allowMarketPolicyOverride: true,
      linkedSettings: null,
      handlers: { onPricingChange, onResolutionChange },
    });
    const graph = buildGraphInspector(
      {
        nodeType: "acquisition",
        depth: 1,
        node: {
          graphNodeId: "buy:root:34",
          parentBuildId: "root",
          typeId: 34,
          typeName: "Tritanium",
          requiredQuantity: 100,
          missingQuantity: 80,
          buildableRecipe: null,
          estimatedCost: "500.0000",
          costState: "known",
          warning: null,
        },
      } as unknown as BuildGraphNodeData,
      {
        worksheetItem: rawMaterial(),
        allowMarketPolicyOverride: true,
        rootTypeId: 500,
        handlers: { onPricingChange: vi.fn() },
      },
    );

    // Same identity classification + same set of canonical sections.
    expect(worksheet.model.identity.kind).toBe("buyMaterial");
    expect(graph.model.identity.kind).toBe("buyMaterial");
    expect(sectionShape(graph.model)).toEqual(sectionShape(worksheet.model));
    for (const model of [worksheet.model, graph.model]) {
      const shape = sectionShape(model);
      expect(shape).toContain("coverage");
      expect(shape).toContain("pricing");
      expect(shape).toContain("value");
      expect(shape).toContain("provenance");
      expect(shape).not.toContain("blueprint");
      expect(shape).not.toContain("facility");
    }

    // Both render through UnifiedItemInspector with a working Pricing control.
    for (const { model, actions } of [worksheet, graph]) {
      const { unmount } = render(
        <InspectorCollapseProvider>
          <UnifiedItemInspector actions={actions} model={model} />
        </InspectorCollapseProvider>,
      );
      const pricing = screen.getByRole("region", { name: "Pricing" });
      await userEvent.click(within(pricing).getByRole("radio", { name: "Manual price" }));
      unmount();
    }
  });

  it("both adapters produce the same section shape for the same linked Build", () => {
    const worksheet = buildWorksheetInspector({
      item: rawMaterial({ typeId: 900, typeName: "Widget", isBuildResolved: true }),
      buildResolved: true,
      linkedBuild,
      creatingLinkedBuild: false,
      linkedBuildError: "",
      allFacilities: [],
      parentBuildId: "root",
      rootTypeId: 500,
      recipe: { mode: "manufacturing", blueprintTypeId: 900 },
      observations: [],
      readOnly: false,
      allowMarketPolicyOverride: true,
      linkedSettings: null,
      handlers: { onPricingChange: vi.fn(), onResolutionChange: vi.fn() },
    });
    const graph = buildGraphInspector(graphLinkedNode, { enrichment });

    for (const model of [worksheet.model, graph.model]) {
      const shape = sectionShape(model);
      expect(shape).toContain("blueprint");
      expect(shape).toContain("facility");
      expect(shape).toContain("cost");
    }
    // The Graph node also exposes the shared sourcing + provenance sections.
    expect(sectionShape(graph.model)).toContain("sourcing");
  });

  it("canonical section ids are the only keys either adapter emits", () => {
    const graph = buildGraphInspector(graphLinkedNode, { enrichment });
    for (const id of sectionShape(graph.model)) {
      expect(INSPECTOR_SECTION_ORDER).toContain(id as (typeof INSPECTOR_SECTION_ORDER)[number]);
    }
  });
});

// ─── strong parity: SAME linked Build, both selection sources ────────────

/** A serialisable digest of everything a slice contributes to the rendered
 * inspector -- functions omitted, so two models that differ only in which
 * host wired the callbacks compare equal. */
function modelDigest(model: InspectorModel): unknown {
  return JSON.parse(
    JSON.stringify(model, (key, value) => (typeof value === "function" ? undefined : value)),
  );
}

/** The rendered inspector as { sections: [{label, text}], controls: [...] } --
 * structural, whitespace-normalised, id-free. */
function renderedShape(model: InspectorModel, actions: Parameters<typeof UnifiedItemInspector>[0]["actions"]) {
  const { container, unmount } = render(
    <InspectorCollapseProvider>
      <UnifiedItemInspector actions={actions} model={model} />
    </InspectorCollapseProvider>,
  );
  const norm = (s: string | null) => (s ?? "").replace(/\s+/g, " ").trim();
  const sections = [...container.querySelectorAll<HTMLElement>("section[aria-label]")].map((el) => ({
    label: el.getAttribute("aria-label"),
    text: norm(el.textContent),
  }));
  const controls = [...container.querySelectorAll<HTMLElement>("button, input, select, [role='radio']")].map(
    (el) =>
      norm(
        el.getAttribute("aria-label") ||
          el.textContent ||
          el.closest("label")?.textContent ||
          el.getAttribute("name"),
      ),
  );
  unmount();
  return { sections, controls };
}

describe("linked Build inspector is identical from Worksheet and Graph", () => {
  const FACILITY = {
    id: "fac-X",
    name: "Sotiyo X",
    solarSystemName: "Jita",
    structureTypeName: "Sotiyo",
    materialReductionPercent: 2,
    timeReductionPercent: 30,
    archivedAt: null,
    role: "manufacturing",
    revision: 4,
    rigs: [],
  } as unknown as import("../../../../api/industry").FacilityProfile;

  const ionRow: WorksheetItem = {
    typeId: 900,
    typeName: "Ion Thruster",
    role: "material",
    requiredQuantity: 67,
    availableQuantity: 0,
    coveredQuantity: 0,
    missingQuantity: 67,
    coveragePercentage: "0.00",
    projectedInventoryCost: null,
    pricing: {
      selectionKind: "market_policy",
      effectivePolicy: "highestBuy",
      unitPrice: "1000.0000",
      manualUnitPrice: null,
      missing: false,
      sourceNote: "Jita 4-4 buy",
    },
    lineTotal: "3350000.0000",
    contributions: [{ parentTypeId: 500, parentTypeName: "Ishtar", quantity: 67 }],
    isBuildResolved: true,
    installationCost: "350000.0000",
  };

  const ionBuild = {
    id: "ion-1",
    revision: 2,
    runs: 67,
    recipe: {
      kind: "manufacturing",
      blueprintTypeId: 900,
      blueprintName: "Ion Thruster Blueprint",
      products: [{ typeId: 900, typeName: "Ion Thruster", quantityPerRun: 1, sortOrder: 0 }],
    },
    selectedBlueprintOrigin: "original",
    recipeCurrency: "current",
    draftPlanning: {
      input: {
        blueprintSelection: {
          mode: "manual",
          kind: "original",
          materialEfficiency: 10,
          timeEfficiency: 20,
          licensedRuns: null,
          notes: "",
        },
        manufacturingFacility: { facilityProfileId: "fac-X" },
        reactionFacility: null,
      },
    },
  } as unknown as Build;

  const linkedSettings = {
    build: ionBuild,
    pending: false,
    error: null,
    updateBlueprintSelection: vi.fn(),
    updateFacility: vi.fn(),
    updatePricing: vi.fn(),
    refresh: vi.fn(),
  } as unknown as BuildSettings & {
    updateBlueprintSelection: ReturnType<typeof vi.fn>;
    updateFacility: ReturnType<typeof vi.fn>;
  };

  const ionNode: BuildGraphNodeData = {
    nodeType: "production",
    depth: 1,
    node: {
      graphNodeId: "build:ion-1",
      buildId: "ion-1",
      parentBuildId: "root",
      parentComponentTypeId: 900,
      typeId: 900,
      typeName: "Ion Thruster",
      kind: "manufacturing",
      recipe: { mode: "manufacturing", blueprintTypeId: 900 },
      runs: 67,
      persistedRuns: 67,
      requiredQuantity: 67,
      netRequiredQuantity: 67,
      producingQuantity: 67,
      surplus: 0,
      // No surplus (producing == required), so this node's own full
      // operation cost equals what the parent's worksheet row shows as
      // consumed -- material = lineTotal - installationCost, matching
      // `ionRow` below exactly.
      estimatedCost: "3350000.0000",
      materialComponentCost: "3000000.0000",
      ownInstallationCost: "350000.0000",
      costState: "known",
      recipeCurrency: "current",
      effectiveMe: 10,
      effectiveTe: 20,
      children: [],
    },
  } as unknown as BuildGraphNodeData;

  const ionEnrichment: ProductionEnrichment = {
    blueprintName: "Ion Thruster Blueprint",
    blueprintOrigin: "original",
    formulaName: null,
    me: 10,
    te: 20,
    meFromOwnedBlueprint: false,
    facility: FACILITY as unknown as ProductionEnrichment["facility"],
    facilityUnresolved: false,
    plannedDurationSeconds: null,
    installationCost: null,
    totalCost: null,
    computing: false,
  };

  function fromWorksheet() {
    return buildWorksheetInspector({
      item: ionRow,
      buildResolved: true,
      linkedBuild: ionBuild,
      creatingLinkedBuild: false,
      linkedBuildError: "",
      allFacilities: [FACILITY],
      parentBuildId: "root",
      rootTypeId: 500,
      recipe: { mode: "manufacturing", blueprintTypeId: 900 },
      observations: [],
      pricingContext: null,
      readOnly: false,
      allowMarketPolicyOverride: true,
      linkedSettings,
      handlers: {
        onPricingChange: vi.fn(),
        onResolutionChange: vi.fn(),
        onFulfillmentScopeChange: vi.fn(),
        onOpenLinkedBuild: vi.fn(),
        onCopyBuildId: vi.fn(),
      },
    });
  }

  function fromGraph() {
    return buildGraphInspector(ionNode, {
      enrichment: ionEnrichment,
      warnings: [],
      recipeCurrency: "current",
      linkedSettings,
      allFacilities: [FACILITY],
      observations: [],
      worksheetItem: ionRow,
      pricingContext: null,
      allowMarketPolicyOverride: true,
      rootTypeId: 500,
      fulfillmentScope: "missing",
      handlers: {
        onBuy: vi.fn(),
        onScope: vi.fn(),
        onOpenLinkedBuild: vi.fn(),
        onCopyBuildId: vi.fn(),
        onPricingChange: vi.fn(),
      },
    });
  }

  it("produces the same canonical model (identity, every slice, section order)", () => {
    const w = fromWorksheet();
    const g = fromGraph();
    expect(sectionShape(g.model)).toEqual(sectionShape(w.model));
    expect(sectionShape(w.model)).toEqual([
      "quantities",
      "sourcing",
      "blueprint",
      "facility",
      "cost",
      "pricing",
      "usedBy",
      "provenance",
    ]);
    expect(modelDigest(g.model)).toEqual(modelDigest(w.model));
  });

  it("renders identical sections, values and controls through UnifiedItemInspector", () => {
    const w = fromWorksheet();
    const g = fromGraph();
    const wShape = renderedShape(w.model, { ...w.actions, onClose: vi.fn() });
    const gShape = renderedShape(g.model, { ...g.actions, onClose: vi.fn() });
    expect(gShape.sections).toEqual(wShape.sections);
    expect(gShape.controls).toEqual(wShape.controls);
    // And it really is the linked-Build section set, not a coincidental match.
    expect(wShape.sections.map((s) => s.label)).toEqual([
      "Quantities",
      "Sourcing",
      "Blueprint",
      "Facility",
      "Cost",
      "Pricing",
      "Used By",
      "Provenance",
    ]);
  });

  it("renders the same read-only ME/TE from either source -- Stages, not Worksheet/Graph, edits it", () => {
    for (const build of [fromWorksheet(), fromGraph()]) {
      linkedSettings.updateBlueprintSelection.mockClear();
      const { unmount } = render(
        <InspectorCollapseProvider>
          <UnifiedItemInspector actions={{ ...build.actions, onClose: vi.fn() }} model={build.model} />
        </InspectorCollapseProvider>,
      );
      const blueprint = screen.getByRole("region", { name: "Blueprint" });
      expect(within(blueprint).getByText("ME 10 · TE 20")).toBeInTheDocument();
      expect(within(blueprint).queryByLabelText("Material Efficiency (0-10)")).toBeNull();
      expect(linkedSettings.updateBlueprintSelection).not.toHaveBeenCalled();
      unmount();
    }
  });
});
