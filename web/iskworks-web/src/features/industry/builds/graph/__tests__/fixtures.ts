import type {
  AcquisitionNode,
  Build,
  BuildGraphProjection,
  FacilityProfile,
  GraphChild,
  ProductionKind,
  ProductionNode,
  UnresolvedBuildNode,
} from "../../../../../api/industry";

export const ROOT_BUILD_ID = "root-build-1";

export function productionNode(
  overrides: Partial<ProductionNode> & Pick<ProductionNode, "graphNodeId" | "buildId" | "typeId">,
): ProductionNode {
  const kind: ProductionKind = overrides.kind ?? "manufacturing";
  return {
    parentBuildId: null,
    parentComponentTypeId: null,
    typeName: `Type ${overrides.typeId}`,
    kind,
    recipe: { mode: "manufacturing", blueprintTypeId: 1000 + overrides.typeId },
    runs: 1,
    persistedRuns: 1,
    requiredQuantity: null,
    netRequiredQuantity: null,
    producingQuantity: 1,
    surplus: 0,
    estimatedCost: null,
    materialComponentCost: null,
    ownInstallationCost: null,
    costState: "notComputed",
    recipeCurrency: "current",
    effectiveMe: null,
    effectiveTe: null,
    children: [],
    ...overrides,
  };
}

export function productionChild(
  node: ProductionNode,
  parentBuildId: string,
  parentComponentTypeId: number,
): GraphChild {
  return {
    nodeKind: "production",
    ...node,
    parentBuildId,
    parentComponentTypeId,
  };
}

/** A buildable acquisition child by default (carries a recipe). Pass
 * `buildableRecipe: null` for a raw/terminal acquisition node. */
export function acquisitionChild(
  overrides: Partial<AcquisitionNode> &
    Pick<AcquisitionNode, "graphNodeId" | "parentBuildId" | "typeId">,
): GraphChild {
  const requiredQuantity = overrides.requiredQuantity ?? 10;
  return {
    nodeKind: "acquisition",
    typeName: `Type ${overrides.typeId}`,
    requiredQuantity,
    // Default: no inventory coverage (`missing === required`), so a fixture
    // that only sets `requiredQuantity` presents as a plain BUY node.
    missingQuantity: requiredQuantity,
    buildableRecipe: { mode: "manufacturing", blueprintTypeId: 2000 + overrides.typeId },
    estimatedCost: null,
    costState: "notComputed",
    warning: null,
    ...overrides,
  };
}

/** A raw/terminal acquisition child -- no `buildableRecipe`, no BUILD action. */
export function rawAcquisitionChild(
  overrides: Partial<AcquisitionNode> &
    Pick<AcquisitionNode, "graphNodeId" | "parentBuildId" | "typeId">,
): GraphChild {
  return acquisitionChild({ ...overrides, buildableRecipe: null });
}

export function unresolvedBuildChild(
  overrides: Partial<UnresolvedBuildNode> &
    Pick<UnresolvedBuildNode, "graphNodeId" | "parentBuildId" | "typeId">,
): GraphChild {
  return {
    nodeKind: "unresolvedBuild",
    typeName: `Type ${overrides.typeId}`,
    recipe: { mode: "manufacturing", blueprintTypeId: 3000 + overrides.typeId },
    requiredQuantity: 10,
    netRequiredQuantity: 10,
    ...overrides,
  };
}

export function projection(root: ProductionNode): BuildGraphProjection {
  return { root, warnings: [], generatedAt: "2026-01-01T00:00:00Z", marketEvidence: [] };
}

// ---- lazy inspector enrichment fixtures ---------------------------------

/** A persisted `Build` as `GET /api/builds/:id` returns it. Defaults to a
 * manufacturing build with a manual ME 8 / TE 14 blueprint on facility
 * `fac-1`. Pass `recipe`/`draftPlanning` overrides for the reaction case. */
export function buildDetail(overrides: Partial<Build> = {}): Build {
  return {
    id: "child-1",
    workspaceId: "ws-1",
    ownerId: "owner-1",
    name: "Composite build",
    recipe: {
      kind: "manufacturing",
      sourceSdeDatasetId: "sde-1",
      sourceSdeVersion: "1",
      blueprintTypeId: 1234,
      blueprintName: "Composite Blueprint",
      durationSecondsPerRun: 600,
      materials: [],
      products: [{ typeId: 900, typeName: "Composite", quantityPerRun: 1, sortOrder: 0 }],
      fingerprint: "fp",
    },
    runs: 3,
    notes: "",
    revision: 1,
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    draftPlanning: {
      updatedAt: "2026-01-01T00:00:00Z",
      input: {
        materialScope: { regionId: 10_000_002, locationId: 60_003_760 },
        outputScope: { regionId: 10_000_002, locationId: 60_003_760 },
        manualPriceListId: null,
        expectedManualPriceListRevision: null,
        materialPricingPolicy: "highestBuy",
        outputPricingPolicy: "lowestSell",
        pricingSelections: [],
        blueprintSelection: {
          mode: "manual",
          kind: "original",
          materialEfficiency: 8,
          timeEfficiency: 14,
          licensedRuns: null,
          notes: "",
        },
        manufacturingFacility: {
          facilityProfileId: "fac-1",
          blueprintMe: 8,
          blueprintTe: 14,
          estimatedItemValue: null,
        },
        reactionFacility: null,
        facilityEivManual: false,
        componentResolutions: [],
        fulfillmentScopes: [],
      },
    },
    recipeCurrency: "current",
    activeSdeVersion: "1",
    productCategoryName: null,
    productGroupName: null,
    selectedBlueprintOrigin: "original",
    hasOwnedBlueprint: false,
    ...overrides,
  };
}

/** A reaction persisted `Build`: reaction formula recipe + reaction facility,
 * no blueprint selection. */
export function reactionBuildDetail(overrides: Partial<Build> = {}): Build {
  const base = buildDetail();
  return {
    ...base,
    name: "Reaction build",
    recipe: {
      kind: "reaction",
      sourceSdeDatasetId: "sde-1",
      sourceSdeVersion: "1",
      reactionFormulaTypeId: 4321,
      reactionFormulaName: "Composite Reaction",
      durationSecondsPerRun: 3600,
      materials: [],
      products: [{ typeId: 900, typeName: "Composite", quantityPerRun: 10, sortOrder: 0 }],
      fingerprint: "fp",
    },
    draftPlanning: {
      updatedAt: "2026-01-01T00:00:00Z",
      input: {
        ...base.draftPlanning!.input,
        blueprintSelection: null,
        manufacturingFacility: null,
        reactionFacility: {
          facilityProfileId: "fac-1",
          estimatedItemValue: null,
        },
      },
    },
    selectedBlueprintOrigin: null,
    ...overrides,
  };
}

export function facilityProfile(overrides: Partial<FacilityProfile> = {}): FacilityProfile {
  return {
    id: "fac-1",
    workspaceId: "ws-1",
    name: "Sotiyo — Assembly",
    kind: "upwellStructure",
    role: "manufacturing",
    structureId: 1,
    structureTypeId: 35_827,
    structureTypeName: "Sotiyo",
    solarSystemId: 30_000_142,
    solarSystemName: "Jita",
    securityClass: "highSec",
    materialReductionPercent: "1.0",
    timeReductionPercent: "30.0",
    jobCostReductionPercent: "0",
    facilityTaxPercent: "1.0",
    sccSurchargePercent: "0.4",
    allianceSurchargePercent: "0",
    fixedSupplementalCost: "0",
    manualSystemCostIndex: null,
    notes: "",
    rigs: [],
    archivedAt: null,
    revision: 1,
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    ...overrides,
  };
}

/**
 * Mixed hierarchy: root Manufacturing -> Reaction child -> Manufacturing
 * grandchild. Exercises reaction/manufacturing neutrality.
 */
export function mixedChainProjection(): BuildGraphProjection {
  const grandchild = productionNode({
    graphNodeId: "build:m-grandchild",
    buildId: "m-grandchild",
    typeId: 600,
    kind: "manufacturing",
    recipe: { mode: "manufacturing", blueprintTypeId: 4600 },
    parentBuildId: "r-child",
    parentComponentTypeId: 600,
    requiredQuantity: 20,
    netRequiredQuantity: 20,
    producingQuantity: 20,
  });
  const child = productionNode({
    graphNodeId: "build:r-child",
    buildId: "r-child",
    typeId: 700,
    kind: "reaction",
    recipe: { mode: "reaction", reactionFormulaTypeId: 3700 },
    parentBuildId: ROOT_BUILD_ID,
    parentComponentTypeId: 700,
    requiredQuantity: 10,
    netRequiredQuantity: 10,
    producingQuantity: 10,
    children: [productionChild(grandchild, "r-child", 600)],
  });
  const root = productionNode({
    graphNodeId: `root:${ROOT_BUILD_ID}`,
    buildId: ROOT_BUILD_ID,
    typeId: 500,
    kind: "rootManufacturing",
    runs: 1,
    children: [productionChild(child, ROOT_BUILD_ID, 700)],
  });
  return projection(root);
}

/**
 * root
 * ├─ A ─ B
 * │    └ C
 * └─ D
 * Plus an actionable Buy under A. For collapse tests.
 */
export function branchingProjection(): BuildGraphProjection {
  const b = productionNode({
    graphNodeId: "build:B",
    buildId: "B",
    typeId: 201,
    parentBuildId: "A",
    parentComponentTypeId: 201,
  });
  const c = productionNode({
    graphNodeId: "build:C",
    buildId: "C",
    typeId: 202,
    parentBuildId: "A",
    parentComponentTypeId: 202,
  });
  const a = productionNode({
    graphNodeId: "build:A",
    buildId: "A",
    typeId: 200,
    parentBuildId: ROOT_BUILD_ID,
    parentComponentTypeId: 200,    children: [
      productionChild(b, "A", 201),
      productionChild(c, "A", 202),
      acquisitionChild({
        graphNodeId: "buy:A:210",
        parentBuildId: "A",
        typeId: 210,
        typeName: "Pyerite",
      }),
    ],
  });
  const d = productionNode({
    graphNodeId: "build:D",
    buildId: "D",
    typeId: 300,
    parentBuildId: ROOT_BUILD_ID,
    parentComponentTypeId: 300,
  });
  const root = productionNode({
    graphNodeId: `root:${ROOT_BUILD_ID}`,
    buildId: ROOT_BUILD_ID,
    typeId: 500,
    kind: "rootManufacturing",
    children: [productionChild(a, ROOT_BUILD_ID, 200), productionChild(d, ROOT_BUILD_ID, 300)],
  });
  return projection(root);
}

/** root -> production child (comp 900) -> production grandchild (comp 700). */
export function nestedProjection(): BuildGraphProjection {
  const grandchild = productionNode({
    graphNodeId: "build:grandchild-1",
    buildId: "grandchild-1",
    typeId: 700,
    kind: "reaction",
    recipe: { mode: "reaction", reactionFormulaTypeId: 3700 },
    parentBuildId: "child-1",
    parentComponentTypeId: 700,
    requiredQuantity: 200,
    netRequiredQuantity: 200,
    producingQuantity: 200,
  });
  const child = productionNode({
    graphNodeId: "build:child-1",
    buildId: "child-1",
    typeId: 900,
    parentBuildId: ROOT_BUILD_ID,
    parentComponentTypeId: 900,
    requiredQuantity: 4,
    netRequiredQuantity: 4,
    producingQuantity: 4,    children: [productionChild(grandchild, "child-1", 700)],
  });
  const root = productionNode({
    graphNodeId: `root:${ROOT_BUILD_ID}`,
    buildId: ROOT_BUILD_ID,
    typeId: 500,
    kind: "rootManufacturing",
    runs: 2,    children: [productionChild(child, ROOT_BUILD_ID, 900)],
  });
  return projection(root);
}
