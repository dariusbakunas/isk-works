import { describe, expect, it, vi } from "vitest";

import type { GraphWarning, WorksheetItem } from "../../../../api/industry";
import type { ProductionEnrichment } from "../../builds/graph/graph-inspector-enrichment";
import type { BuildGraphNodeData } from "../../builds/graph/to-react-flow";
import { buildGraphInspector } from "../adapters/graph-node";
import { buildWorksheetInspector } from "../adapters/worksheet-item";

function production(kind: "manufacturing" | "reaction" | "rootManufacturing" = "manufacturing") {
  return {
    nodeType: kind === "rootManufacturing" ? "root" : "production",
    depth: 1,
    node: {
      graphNodeId: "build:x",
      buildId: "x",
      parentBuildId: "root",
      parentComponentTypeId: 900,
      typeId: 900,
      typeName: "Widget",
      kind,
      recipe:
        kind === "reaction"
          ? { mode: "reaction", reactionFormulaTypeId: 5 }
          : { mode: "manufacturing", blueprintTypeId: 900 },
      runs: 45,
      requiredQuantity: 5024,
      netRequiredQuantity: 5024,
      producingQuantity: 5625,
      surplus: 601,
      estimatedCost: "25360000.0000",
      costState: "known",
      recipeCurrency: "current",
      effectiveMe: 10,
      effectiveTe: 20,
      children: [],
    },
  } as unknown as BuildGraphNodeData;
}

const enrichment: ProductionEnrichment = {
  blueprintName: "Widget Blueprint",
  blueprintOrigin: "original",
  formulaName: "Widget Reaction",
  me: 10,
  te: 20,
  meFromOwnedBlueprint: false,
  facility: {
    id: "fac-1",
    name: "GEZ-IXX Keepstar",
    solarSystemName: "C-J6MT",
    structureTypeName: "Keepstar",
    materialReductionPercent: 2,
    timeReductionPercent: 30,
    rigs: [],
  } as unknown as ProductionEnrichment["facility"],
  facilityUnresolved: false,
  plannedDurationSeconds: 6480,
  installationCost: "4200000.0000",
  totalCost: "29560000.0000",
  computing: false,
};

describe("buildGraphInspector", () => {
  it("linked manufacturing node: quantities, sourcing, blueprint, facility, split cost, provenance", () => {
    const { model } = buildGraphInspector(production(), {
      enrichment,
      linkedSettings: { build: { draftPlanning: { input: {} } }, pending: false } as never,
    });
    expect(model.identity.kind).toBe("linkedBuild");
    expect(model.identity.summary).toBe("Need 5,024 · Making 5,625");
    // Canonical QUANTITIES merges material demand + production operation.
    expect(model.quantities?.metrics.map((m) => m.label)).toEqual([
      "Required",
      "Available",
      "Covered",
      "Shortage",
      "Making",
      "Surplus",
      "Runs",
    ]);
    expect(model.sourcing?.mode).toBe("build");
    expect(model.blueprint?.kind).toBe("blueprint");
    // Stages owns descendant production configuration editing --
    // Graph shows blueprint/facility read-only.
    expect(model.blueprint?.editable).toBe(false);
    expect(model.recipe).toBeUndefined();
    expect(model.facility?.name).toBe("GEZ-IXX Keepstar");
    expect(model.cost).toMatchObject({ state: "known" });
    expect(model.provenance?.buildId).toBe("x");
  });

  it("reaction node exposes Recipe, never a Blueprint slice", () => {
    const { model } = buildGraphInspector(production("reaction"), {
      enrichment: { ...enrichment, blueprintName: null, me: null, te: null },
    });
    expect(model.identity.kind).toBe("reaction");
    expect(model.recipe?.kind).toBe("formula");
    expect(model.blueprint).toBeUndefined();
  });

  it("a root node is not a graph-node target -- the host builds it separately", () => {
    // The root Build is built from the shared editor via `buildRootInspector`
    // (see `root-build-parity.test.tsx`), never through `buildGraphInspector`.
    expect(() => buildGraphInspector(production("rootManufacturing"), { enrichment })).toThrow(
      /root nodes are handled by buildRootInspector/,
    );
  });

  it("recovers a stale observedAsset selection after an ESI re-sync minted fresh observation rows", () => {
    // The Build saved `observationId: "obs-old"`; a later sync replaced that
    // row with "obs-new" (same physical BPC). The inspector must still show
    // the blueprint as selected, matching the Build page.
    const freshObservation = {
      id: "obs-new",
      workspaceId: "ws",
      ownerId: "owner-1",
      ownerName: "Valka",
      eveItemId: 42,
      blueprintTypeId: 900,
      blueprintName: "Widget Blueprint",
      kind: "copy" as const,
      materialEfficiency: 10,
      timeEfficiency: 20,
      licensedRuns: 90,
      locationId: 60_003_760,
      locationFlag: "Hangar",
      locationName: "C-J6MT",
      observedAt: "2026-09-07T13:52:00Z",
      importedAt: "2026-09-07T13:52:00Z",
    };
    const { model } = buildGraphInspector(production(), {
      // After the sync the old observation is gone, so neither the Build nor
      // the enrichment still resolves an origin -- the recovered observation
      // supplies it.
      enrichment: { ...enrichment, blueprintOrigin: null, me: null, te: null, meFromOwnedBlueprint: true },
      linkedSettings: {
        build: {
          draftPlanning: { input: { blueprintSelection: { mode: "observedAsset", observationId: "obs-old" } } },
        },
        pending: false,
      } as never,
      observations: [freshObservation] as never,
    });

    expect(model.blueprint?.mode).toBe("existing");
    expect(model.blueprint?.selectedObservationId).toBe("obs-new");
    // ME/TE fall through to the recovered observation when enrichment has none.
    expect(model.blueprint?.me).toBe(10);
    expect(model.blueprint?.te).toBe(20);
    expect(model.blueprint?.origin).toBe("BPC");
  });

  it("acquisition node borrows the material slices from the matching worksheet row", () => {
    const onPricingChange = vi.fn();
    const { model, actions } = buildGraphInspector(
      {
        nodeType: "acquisition",
        depth: 2,
        node: {
          graphNodeId: "buy:x:34",
          parentBuildId: "x",
          typeId: 34,
          typeName: "Tritanium",
          requiredQuantity: 100,
          missingQuantity: 40,
          buildableRecipe: null,
          estimatedCost: "500.0000",
          costState: "known",
          warning: null,
        },
      } as unknown as BuildGraphNodeData,
      {
        worksheetItem: {
          typeId: 34,
          typeName: "Tritanium",
          role: "material",
          requiredQuantity: 100,
          availableQuantity: 60,
          coveredQuantity: 60,
          missingQuantity: 40,
          coveragePercentage: "60.00",
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
          contributions: [],
          isBuildResolved: false,
          installationCost: null,
        } as never,
        allowMarketPolicyOverride: true,
        handlers: { onPricingChange },
      },
    );
    expect(model.identity.kind).toBe("buyMaterial");
    expect(model.coverage?.percentage).toBe(60);
    expect(model.pricing?.kind === "root" ? null : model.pricing?.typeId).toBe(34);
    expect(model.value).toBeDefined();
    expect(model.cost).toBeUndefined();
    expect(actions.pricing?.onChange).toBe(onPricingChange);
  });

  // ── acquisition sourcing is computed from live coverage, never
  //    hard-coded. ───────────────────────────────────────────────────────
  function acquisitionNode(missingQuantity: number, requiredQuantity = 2250) {
    return {
      nodeType: "acquisition",
      depth: 2,
      node: {
        graphNodeId: "buy:x:34",
        parentBuildId: "x",
        typeId: 34,
        typeName: "Mexallon",
        requiredQuantity,
        missingQuantity,
        buildableRecipe: null,
        estimatedCost: null,
        costState: "notComputed",
        warning: null,
      },
    } as unknown as BuildGraphNodeData;
  }

  it("acquisition, full coverage: fullyCoveredByInventory + usingInventory + 'Use Inventory'", () => {
    const { model } = buildGraphInspector(acquisitionNode(0), {});
    expect(model.sourcing?.fullyCoveredByInventory).toBe(true);
    expect(model.sourcing?.usingInventory).toBe(true);
    expect(model.sourcing?.summary).toBe("Use Inventory");
    expect(model.sourcing?.fulfillmentSentence).toBe("Use 2,250 from inventory.");
    expect(model.identity.summary).toBe("Required 2,250");
  });

  it("acquisition, partial coverage: not usingInventory, summary communicates Buy + shortage", () => {
    const { model } = buildGraphInspector(acquisitionNode(1250), {});
    expect(model.sourcing?.fullyCoveredByInventory).toBe(false);
    expect(model.sourcing?.usingInventory).toBe(false);
    expect(model.sourcing?.summary).toBe("Buy · Missing (1,250)");
    expect(model.sourcing?.fulfillmentSentence).toBe(
      "Use 1,000 from inventory and buy 1,250.",
    );
    expect(model.identity.summary).toBe("Required 2,250 · 1,250 short");
  });

  it("acquisition, no coverage: summary is plain 'Buy'", () => {
    const { model } = buildGraphInspector(acquisitionNode(2250), {});
    expect(model.sourcing?.fullyCoveredByInventory).toBe(false);
    expect(model.sourcing?.usingInventory).toBe(false);
    expect(model.sourcing?.summary).toBe("Buy");
    expect(model.sourcing?.fulfillmentSentence).toBeNull();
  });

  it("acquisition, explicit Full scope: classifies from the DTO's missingQuantity, not physical stock", () => {
    // Backend sends missing === required for a Full-scoped node. Even with a
    // worksheet row showing physical stock fully covers it, the sourcing
    // summary stays "Buy".
    const { model } = buildGraphInspector(acquisitionNode(2250), {
      worksheetItem: {
        typeId: 34,
        typeName: "Mexallon",
        role: "material",
        requiredQuantity: 2250,
        availableQuantity: 9999,
        coveredQuantity: 2250,
        missingQuantity: 0,
        coveragePercentage: "100.00",
        projectedInventoryCost: "1.0000",
        pricing: {
          selectionKind: "default",
          effectivePolicy: "highestBuy",
          unitPrice: "1.0000",
          manualUnitPrice: null,
          missing: false,
          sourceNote: "",
        },
        lineTotal: "2250.0000",
        contributions: [],
        isBuildResolved: false,
        installationCost: null,
      } as never,
    });
    expect(model.sourcing?.usingInventory).toBe(false);
    expect(model.sourcing?.fullyCoveredByInventory).toBe(false);
    expect(model.sourcing?.summary).toBe("Buy");
  });

  it("acquisition, defensive: missing > required clamps to zero inventory, stays 'Buy'", () => {
    const { model } = buildGraphInspector(acquisitionNode(9000, 2250), {});
    expect(model.sourcing?.usingInventory).toBe(false);
    expect(model.sourcing?.missingQuantity).toBe(2250);
    expect(model.sourcing?.summary).toBe("Buy");
  });
});

// ── The SHARED linked-Build inspector's inventory-aware sourcing digest.
//    Same `linkedBuildInspector(input)` the Worksheet build-resolved row
//    feeds; `netRequiredQuantity` (or `required - reusedQuantity`) is the one
//    scope-aware "remaining after inventory" figure both hosts pass. ──────
describe("linkedBuildInspector — inventory-aware sourcing", () => {
  function linkedProduction(opts: {
    kind?: "manufacturing" | "reaction";
    requiredQuantity?: number;
    netRequiredQuantity?: number;
  } = {}) {
    const kind = opts.kind ?? "manufacturing";
    const required = opts.requiredQuantity ?? 100;
    const net = opts.netRequiredQuantity ?? required;
    return {
      nodeType: "production",
      depth: 1,
      node: {
        graphNodeId: "build:x",
        buildId: "x",
        parentBuildId: "root",
        parentComponentTypeId: 900,
        typeId: 900,
        typeName: "Widget",
        kind,
        recipe:
          kind === "reaction"
            ? { mode: "reaction", reactionFormulaTypeId: 5 }
            : { mode: "manufacturing", blueprintTypeId: 900 },
        runs: 10,
        requiredQuantity: required,
        netRequiredQuantity: net,
        producingQuantity: required,
        surplus: 0,
        estimatedCost: "1000.0000",
        costState: "known",
        recipeCurrency: "current",
        effectiveMe: 10,
        effectiveTe: 20,
        children: [],
      },
    } as unknown as BuildGraphNodeData;
  }

  function stockedRow(overrides: Partial<WorksheetItem> = {}): WorksheetItem {
    return {
      typeId: 900,
      typeName: "Widget",
      role: "material",
      requiredQuantity: 100,
      availableQuantity: 500,
      coveredQuantity: 100,
      missingQuantity: 0,
      coveragePercentage: "100.00",
      projectedInventoryCost: "1.0000",
      pricing: {
        selectionKind: "default",
        effectivePolicy: "highestBuy",
        unitPrice: "1.0000",
        manualUnitPrice: null,
        missing: false,
        sourceNote: "",
      },
      lineTotal: "100.0000",
      contributions: [],
      isBuildResolved: true,
      installationCost: null,
      ...overrides,
    };
  }

  // A. Build + Missing scope + inventory covers the whole current demand.
  it("Build, full inventory coverage: usingInventory + 'Use Inventory'", () => {
    const { model } = buildGraphInspector(linkedProduction({ netRequiredQuantity: 0 }), {});
    expect(model.sourcing?.mode).toBe("build");
    expect(model.sourcing?.usingInventory).toBe(true);
    expect(model.sourcing?.fullyCoveredByInventory).toBe(true);
    expect(model.sourcing?.hasShortfall).toBe(false);
    expect(model.sourcing?.summary).toBe("Use Inventory");
    expect(model.sourcing?.fulfillmentSentence).toBe("Use 100 from inventory.");
  });

  // B. Build + Missing scope + inventory covers only part of the demand.
  it("Build, partial coverage: 'Build · Missing (60)' + 40-from-inventory sentence", () => {
    const { model } = buildGraphInspector(
      linkedProduction({ requiredQuantity: 100, netRequiredQuantity: 60 }),
      {},
    );
    expect(model.sourcing?.usingInventory).toBe(false);
    expect(model.sourcing?.fullyCoveredByInventory).toBe(false);
    expect(model.sourcing?.hasShortfall).toBe(true);
    expect(model.sourcing?.requiredQuantity).toBe(100);
    expect(model.sourcing?.missingQuantity).toBe(60);
    expect(model.sourcing?.summary).toBe("Build · Missing (60)");
    expect(model.sourcing?.fulfillmentSentence).toBe(
      "Use 40 from inventory and build 60.",
    );
  });

  // C. Build + no inventory: plain Build sourcing, no split, no sentence.
  it("Build, no coverage: plain 'Build'", () => {
    const { model } = buildGraphInspector(
      linkedProduction({ requiredQuantity: 100, netRequiredQuantity: 100 }),
      {},
    );
    expect(model.sourcing?.usingInventory).toBe(false);
    expect(model.sourcing?.fullyCoveredByInventory).toBe(false);
    expect(model.sourcing?.hasShortfall).toBe(false);
    expect(model.sourcing?.summary).toBe("Build");
    expect(model.sourcing?.fulfillmentSentence).toBeNull();
  });

  // D. Explicit Full scope: backend sends net === required. Even with a
  //    worksheet row whose physical stock fully covers the component, the
  //    sourcing digest stays Build and physical stock still shows in
  //    QUANTITIES.
  it("Build, explicit Full scope: stays 'Build' despite physical stock", () => {
    const { model } = buildGraphInspector(
      linkedProduction({ requiredQuantity: 100, netRequiredQuantity: 100 }),
      { fulfillmentScope: "full", worksheetItem: stockedRow() },
    );
    expect(model.sourcing?.scope).toBe("full");
    expect(model.sourcing?.usingInventory).toBe(false);
    expect(model.sourcing?.fullyCoveredByInventory).toBe(false);
    expect(model.sourcing?.summary).toBe("Build");
    // Physical Available / Covered are still surfaced.
    expect(model.quantities?.metrics.find((m) => m.label === "Available")?.value).toBe("500");
    expect(model.quantities?.metrics.find((m) => m.label === "Covered")?.value).toBe("100");
  });

  // E. Reaction vocabulary for full + partial coverage.
  it("Reaction, full coverage: 'Use Inventory'", () => {
    const { model } = buildGraphInspector(
      linkedProduction({ kind: "reaction", netRequiredQuantity: 0 }),
      {},
    );
    expect(model.identity.kind).toBe("reaction");
    expect(model.sourcing?.usingInventory).toBe(true);
    expect(model.sourcing?.summary).toBe("Use Inventory");
    expect(model.sourcing?.fulfillmentSentence).toBe("Use 100 from inventory.");
  });

  it("Reaction, partial coverage: 'Reaction · Missing (60)' + react verb", () => {
    const { model } = buildGraphInspector(
      linkedProduction({ kind: "reaction", requiredQuantity: 100, netRequiredQuantity: 60 }),
      {},
    );
    expect(model.sourcing?.summary).toBe("Reaction · Missing (60)");
    expect(model.sourcing?.fulfillmentSentence).toBe(
      "Use 40 from inventory and react 60.",
    );
  });

  // F. Parity: the Worksheet build-resolved row (partial coverage encoded as
  //    `reusedQuantity`) and the Graph production node (partial coverage
  //    encoded as `netRequiredQuantity`) produce the same sourcing digest.
  it("parity: Worksheet reusedQuantity and Graph netRequiredQuantity agree", () => {
    const linkedBuild = {
      id: "x",
      revision: 1,
      runs: 10,
      recipe: {
        kind: "manufacturing",
        blueprintTypeId: 900,
        blueprintName: "Widget Blueprint",
        products: [{ typeId: 900, typeName: "Widget", quantityPerRun: 10, sortOrder: 0 }],
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
          manufacturingFacility: null,
          reactionFacility: null,
        },
      },
    } as never;

    const worksheet = buildWorksheetInspector({
      item: stockedRow({
        requiredQuantity: 100,
        availableQuantity: 40,
        coveredQuantity: 40,
        missingQuantity: 60,
        coveragePercentage: "40.00",
        reusedQuantity: 40,
      }),
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
      handlers: { onPricingChange: () => {}, onResolutionChange: () => {} },
    });
    const graph = buildGraphInspector(
      linkedProduction({ requiredQuantity: 100, netRequiredQuantity: 60 }),
      {},
    );

    const digest = (s: typeof worksheet.model.sourcing) => ({
      usingInventory: s?.usingInventory,
      fullyCoveredByInventory: s?.fullyCoveredByInventory,
      hasShortfall: s?.hasShortfall,
      requiredQuantity: s?.requiredQuantity,
      missingQuantity: s?.missingQuantity,
      summary: s?.summary,
      fulfillmentSentence: s?.fulfillmentSentence,
    });
    expect(digest(graph.model.sourcing)).toEqual(digest(worksheet.model.sourcing));
    expect(digest(worksheet.model.sourcing)).toMatchObject({
      usingInventory: false,
      hasShortfall: true,
      summary: "Build · Missing (60)",
      fulfillmentSentence: "Use 40 from inventory and build 60.",
    });
  });
});

// ─── saved-vs-effective runs, cost split, warning tone ────────────────────

describe("buildGraphInspector — Slice C1.1 Graph node presentation", () => {
  function node(overrides: {
    runs?: number;
    persistedRuns?: number;
    materialComponentCost?: string | null;
    ownInstallationCost?: string | null;
    estimatedCost?: string | null;
    costState?: string;
  } = {}) {
    return {
      nodeType: "production",
      depth: 1,
      node: {
        graphNodeId: "build:x",
        buildId: "x",
        parentBuildId: "root",
        parentComponentTypeId: 900,
        typeId: 900,
        typeName: "Widget",
        kind: "manufacturing",
        recipe: { mode: "manufacturing", blueprintTypeId: 900 },
        runs: overrides.runs ?? 20,
        persistedRuns: overrides.persistedRuns ?? overrides.runs ?? 20,
        requiredQuantity: 20,
        netRequiredQuantity: 20,
        producingQuantity: 20,
        surplus: 0,
        estimatedCost: "estimatedCost" in overrides ? overrides.estimatedCost : "1000.0000",
        materialComponentCost:
          "materialComponentCost" in overrides ? overrides.materialComponentCost : "800.0000",
        ownInstallationCost:
          "ownInstallationCost" in overrides ? overrides.ownInstallationCost : "200.0000",
        costState: overrides.costState ?? "known",
        recipeCurrency: "current",
        effectiveMe: 10,
        effectiveTe: 20,
        children: [],
      },
    } as unknown as BuildGraphNodeData;
  }

  it("exposes the node's own effective (projected) runs as the primary Runs metric", () => {
    const { model } = buildGraphInspector(node({ runs: 20, persistedRuns: 30 }), {});
    const runsMetric = model.quantities?.metrics.find((m) => m.label === "Runs");
    expect(runsMetric?.value).toBe("20");
  });

  it("surfaces the saved Build's own persisted runs only when it diverges from the effective runs", () => {
    const diverged = buildGraphInspector(node({ runs: 20, persistedRuns: 30 }), {});
    const savedMetric = diverged.model.quantities?.metrics.find((m) => m.label === "Saved Build runs");
    expect(savedMetric?.value).toBe("30");

    const aligned = buildGraphInspector(node({ runs: 20, persistedRuns: 20 }), {});
    expect(
      aligned.model.quantities?.metrics.find((m) => m.label === "Saved Build runs"),
    ).toBeUndefined();
  });

  it("exposes the node's own material / installation / total cost split, additive", () => {
    const { model } = buildGraphInspector(
      node({ materialComponentCost: "800.0000", ownInstallationCost: "200.0000", estimatedCost: "1000.0000" }),
      {},
    );
    expect(model.cost?.material).toBe("800 ISK");
    expect(model.cost?.installation).toBe("200 ISK");
    expect(model.cost?.total).toBe("1,000 ISK");
  });

  it("never zero-substitutes an incomplete cost component -- null stays null, not '0 ISK'", () => {
    const { model } = buildGraphInspector(
      node({
        materialComponentCost: "800.0000",
        ownInstallationCost: null,
        estimatedCost: null,
        costState: "incomplete",
      }),
      {},
    );
    expect(model.cost?.material).toBe("800 ISK");
    expect(model.cost?.installation).toBeNull();
    expect(model.cost?.total).toBeNull();
    expect(model.cost?.state).toBe("incomplete");
  });

  it("classifies runsDiverged and staleMarketEvidence as informational, not blocking", () => {
    const warnings: GraphWarning[] = [
      { graphNodeId: "build:x", code: "runsDiverged", message: "Saved Build has 30 runs; this plan currently requires 20." },
      { graphNodeId: "build:x", code: "staleMarketEvidence", message: "stale" },
      { graphNodeId: "build:x", code: "linkedBuildUnresolved", message: "unresolved" },
    ];
    const { model } = buildGraphInspector(node(), { warnings });
    const byLabel = new Map(model.warnings.map((w) => [w.label, w]));
    expect(byLabel.get("Runs diverged")?.tone).toBe("neutral");
    expect(byLabel.get("Recipe changed")).toBeUndefined(); // sanity: label mapping is per-code
    const stale = model.warnings.find((w) => w.detail === "stale");
    expect(stale?.tone).toBe("neutral");
    const unresolved = model.warnings.find((w) => w.detail === "unresolved");
    expect(unresolved?.tone).toBe("blocking");
  });
});
