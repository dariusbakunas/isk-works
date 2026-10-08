import { expect, test, type Page } from "@playwright/test";

const workspace = {
  configured: true,
  workspace: { id: "workspace-1", name: "Industry", ownerId: "owner-1", createdAt: "2026-07-29T10:00:00Z", updatedAt: "2026-07-29T10:00:00Z" },
  owner: { id: "owner-1", workspaceId: "workspace-1", kind: "manual", displayName: "Industry", hidden: true },
};

const plan = {
  id: "plan-1",
  revision: 1,
  runs: 1,
  recipeFingerprint: "recipe",
  snapshot: {
    id: "snapshot-1",
    priceSourceId: "source-1",
    sourceName: "Jita 4-4",
    sourceRevision: 1,
    createdAt: "2026-07-29T10:00:00Z",
    items: [
      { typeId: 34, typeName: "Tritanium", price: "4.0000", pricingPolicy: "highestBuy", missing: false, sourceNote: "", sortOrder: 0 },
      { typeId: 587, typeName: "Rifter", price: "1000000.0000", pricingPolicy: "lowestSell", missing: false, sourceNote: "", sortOrder: 1 },
    ],
  },
  pricingComplete: true,
  estimatedMaterialCost: "128000.0000",
  expectedRevenue: "1000000.0000",
  estimatedMargin: "872000.0000",
  missingPriceCount: 0,
  active: true,
  plannedAt: "2026-07-29T10:00:00Z",
  supersededAt: null,
  materialLines: [{ typeId: 34, typeName: "Tritanium", quantityPerRun: 32000, totalQuantity: 32000, unitPrice: "4.0000", lineTotal: "128000.0000", missing: false }],
  facility: null,
  blueprint: null,
};

const plannedBuild = {
  id: "build-planned",
  workspaceId: "workspace-1",
  ownerId: "owner-1",
  name: "Rifter build",
  status: "planned",
  recipe: {
    kind: "manufacturing",
    sourceSdeDatasetId: "sde-1",
    sourceSdeVersion: "test",
    blueprintTypeId: 691,
    blueprintName: "Rifter Blueprint",
    durationSecondsPerRun: 6000,
    materials: [{ typeId: 34, typeName: "Tritanium", quantityPerRun: 32000, sortOrder: 0 }],
    products: [{ typeId: 587, typeName: "Rifter", quantityPerRun: 1, sortOrder: 0 }],
    fingerprint: "recipe",
  },
  runs: 1,
  notes: "",
  activePlanId: "plan-1",
  revision: 2,
  createdAt: "2026-07-29T10:00:00Z",
  updatedAt: "2026-07-29T10:00:00Z",
  plannedAt: "2026-07-29T10:00:00Z",
  productionStartedAt: null,
  completedAt: null,
  historicalMaterialCost: null,
  historicalMaterialCostQuality: null,
  productionPlanRevision: null,
  plans: [plan],
  draftPlanning: null,
  recipeCurrency: "current",
  activeSdeVersion: "test",
};

const draftBuild = {
  ...plannedBuild,
  id: "build-draft",
  name: "Saved Rifter build",
  status: "draft",
  activePlanId: null,
  plannedAt: null,
  plans: [],
  revision: 1,
};

const worksheet = {
  buildId: plannedBuild.id,
  buildRevision: 2,
  planRevision: 1,
  worksheet: {
    groups: [{
      key: "mineral",
      label: "Mineral",
      items: [{
        typeId: 34,
        typeName: "Tritanium",
        role: "material",
        requiredQuantity: 32000,
        availableQuantity: 100000,
        coveredQuantity: 32000,
        missingQuantity: 0,
        coveragePercentage: "100.00",
        projectedInventoryCost: "144000.0000",
        pricing: { selectionKind: "default", effectivePolicy: "highestBuy", unitPrice: "4.0000", manualUnitPrice: null, missing: false, sourceNote: "" },
        lineTotal: "128000.0000",
      }],
    }],
    output: {
      key: "output",
      label: "Output",
      items: [{
        typeId: 587,
        typeName: "Rifter",
        role: "output",
        requiredQuantity: 1,
        availableQuantity: 0,
        coveredQuantity: 0,
        missingQuantity: 0,
        coveragePercentage: "0.00",
        projectedInventoryCost: null,
        pricing: { selectionKind: "default", effectivePolicy: "lowestSell", unitPrice: "1000000.0000", manualUnitPrice: null, missing: false, sourceNote: "" },
        lineTotal: "1000000.0000",
      }],
    },
    summary: {
      materialCost: "128000.0000",
      installationCost: null,
      totalCost: "128000.0000",
      expectedRevenue: "1000000.0000",
      estimatedMargin: "872000.0000",
      pricingComplete: true,
      quantityCoverageComplete: true,
      costCoverageComplete: true,
      warnings: [],
    },
  },
};

const wholeBuildWorksheet = {
  scope: { rootBuildId: plannedBuild.id, focusedProducerId: null, label: "Rifter build" },
  groups: [{
    key: "mineral", label: "Mineral", rowCount: 2, complete: true,
    rows: [
      { id: "buy:34", typeId: 34, typeName: "Tritanium", categoryId: 4, categoryName: "Material", groupId: 18, groupName: "Mineral", sourcing: "buy", requiredQuantity: 32000, coveredQuantity: 32000, shortageQuantity: 0, coveragePercentage: "100.00", evidenceState: "complete", pricing: { state: "complete", classification: "marketPolicy", policy: "highestBuy", sourceNote: "Jita market-depth-v1" }, unitCost: "4.0000", totalValue: "128000.0000", producerBuildId: null, retainedSurplusQuantity: null, retainedSurplusBasis: null, warnings: [] },
      { id: "build:900", typeId: 900, typeName: "Composite Armor Plate", categoryId: 4, categoryName: "Material", groupId: 334, groupName: "Construction Components", sourcing: "manufacturing", requiredQuantity: 161, coveredQuantity: 0, shortageQuantity: 161, coveragePercentage: "0.00", evidenceState: "complete", pricing: { state: "complete", classification: "production", policy: null, sourceNote: "market-depth-v1; requested 107; observed timestamp" }, unitCost: "40000.0000", totalValue: "5000000.0000", producerBuildId: "producer-1", retainedSurplusQuantity: 9, retainedSurplusBasis: "360000.0000", warnings: [] },
    ],
  }],
  output: { typeId: 587, typeName: "Rifter", quantity: 1, unitValue: "128000.0000", totalValue: "128000.0000", evidenceState: "complete" },
  warnings: [], economicsAreAdditive: false, generatedAt: "2026-09-26T00:00:00Z",
};

const coverage = {
  buildId: plannedBuild.id,
  ownerId: "owner-1",
  buildStatus: "planned",
  buildRevision: 2,
  recipeFingerprint: "recipe",
  runs: 1,
  completeQuantityCoverage: true,
  completeCostCoverage: true,
  hasActiveReservations: false,
  canReserve: true,
  canRefreshReservations: false,
  canReleaseReservations: false,
  canStartProduction: false,
  materialLines: [],
  warnings: [],
};

async function installApiFixtures(page: Page) {
  await page.route("**/api/**", async (route) => {
    const url = new URL(route.request().url());
    const path = url.pathname;
    if (!path.startsWith("/api/")) {
      await route.continue();
      return;
    }
    let body: unknown = [];

    if (path === "/api/auth/session") {
      body = { authenticated: true, characterName: "Aeva Stark", workspaceId: "workspace-1", inviteRequired: false };
    } else if (path === "/api/workspace") body = workspace;
    else if (path === "/api/sde") body = { configured: true, active: { id: "sde-1", version: "test", importedAt: "2026-09-26T00:00:00Z" } };
    else if (path === "/api/blueprints/691/plan") body = { blueprintTypeId: 691, blueprintName: "Rifter Blueprint", runs: 1, durationSeconds: 6000, materials: [{ typeId: 34, typeName: "Tritanium", quantityPerRun: 32000, totalQuantity: 32000 }], products: [{ typeId: 587, typeName: "Rifter", quantityPerRun: 1, totalQuantity: 1 }] };
    else if (path === "/api/builds") body = [draftBuild, plannedBuild];
    else if (path === `/api/builds/${plannedBuild.id}`) body = plannedBuild;
    else if (path === `/api/builds/${plannedBuild.id}/worksheet`) {
      body = route.request().method() === "POST" ? wholeBuildWorksheet : worksheet;
    }
    else if (path === `/api/builds/${plannedBuild.id}/coverage`) body = coverage;
    else if (path === `/api/builds/${plannedBuild.id}/decision-summary`) {
      body = {
        buildId: plannedBuild.id,
        buildStatus: "planned",
        readiness: "readyToReserve",
        headline: "This Build is ready for material reservation.",
        supportingText: "Reserve materials before starting production.",
        tone: "positive",
        lifecycleSteps: [],
        nextAction: "reserveMaterials",
        blockers: [],
        warnings: [],
        metrics: {
          expectedRevenue: "1000000.0000",
          marketMaterialCost: "128000.0000",
          plannedInstallationCost: null,
          estimatedTotalManufacturingCost: "128000.0000",
          estimatedGrossSpread: "872000.0000",
          marginPercentage: "87.20",
          markupPercentage: "681.25",
          exclusions: [],
        },
        costComparison: {
          marketMaterialCost: "128000.0000",
          projectedInventoryCost: "144000.0000",
          difference: "16000.0000",
          differencePercentage: "12.50",
          costQuality: "known",
          explanation: "",
        },
        materialSummary: { materialCount: 1, coveredMaterialCount: 1, fullyReservedMaterialCount: 0, missingMaterialCount: 0, missingUnitCount: 0, reservedUnitCount: 0, allAvailable: true, allReserved: false, costResolvable: true },
        pricingSummary: { sourceName: "Jita 4-4", snapshotId: "snapshot-1", capturedAt: "2026-07-29T10:00:00Z", ageSeconds: 60, freshness: "fresh", pricedItemCount: 2, totalItemCount: 2, materialPolicies: ["highestBuy"], outputPolicies: ["lowestSell"] },
        facilityGuidance: { state: "notConfigured", facilityName: null, message: "Facility not configured.", installationCost: null, blocksProduction: false },
        latestChange: null,
      };
    } else if (path === "/api/price-sources") {
      body = [{ id: "source-1", workspaceId: "workspace-1", name: "Jita 4-4", description: "", kind: "eveClientMarketExport", isDefault: true, revision: 1, itemCount: 2, recentBuildCount: 1, items: [], createdAt: "2026-07-29T10:00:00Z", updatedAt: "2026-07-29T10:00:00Z" }];
    }

    await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(body) });
  });
}

test("Draft and Planned Builds share the canonical workflow", async ({ page }) => {
  await installApiFixtures(page);
  await page.goto("/builds");

  await expect(page.getByRole("heading", { name: "Builds" })).toBeVisible();
  const draftRow = page.getByRole("link", { name: "Open Rifter build" }).first();
  const plannedRow = page.getByRole("link", { name: "Open Rifter build" }).last();
  await expect(draftRow.locator("xpath=..")).toContainText("Saved Rifter build");
  await expect(plannedRow.locator("xpath=..")).toContainText("Rifter build");

  await plannedRow.click();
  await expect(page).toHaveURL(/\/builds\/build-planned$/);
  await expect(page.getByRole("heading", { name: plannedBuild.name })).toBeVisible();
});

test("legacy edit URL redirects to the canonical Build URL", async ({ page }) => {
  await installApiFixtures(page);
  await page.goto(`/builds/${plannedBuild.id}/edit`);

  await expect(page).toHaveURL(new RegExp(`/builds/${plannedBuild.id}$`));
  await expect(page.getByRole("heading", { name: plannedBuild.name })).toBeVisible();
});

test("Worksheet is a read-only non-additive whole-Build view", async ({ page }) => {
  await installApiFixtures(page);
  await page.goto(`/builds/${plannedBuild.id}`);

  expect(await page.getByRole("tab").allTextContents()).toEqual(["Worksheet", "Plan", "Logistics", "Graph"]);
  await expect(page.getByRole("tab", { name: "Worksheet" })).toHaveAttribute("aria-selected", "true");
  await expect(page).toHaveURL(new RegExp(`/builds/${plannedBuild.id}$`));
  await expect(page.getByRole("heading", { name: "Production Worksheet" })).toBeVisible();
  await expect(page.getByRole("table", { name: "Production Worksheet" })).toContainText("Tritanium");
  await expect(page.getByRole("table", { name: "Production Worksheet" })).toContainText("Composite Armor Plate");
  await expect(page.getByRole("columnheader", { name: "Total Value ⓘ" })).toHaveAttribute("title", /not additive/);
  await expect(page.getByRole("row", { name: /Composite Armor Plate/ })).toContainText("Production");
  await expect(page.getByRole("row", { name: /Composite Armor Plate/ })).not.toContainText("market-depth-v1");
  await expect(page.getByRole("row", { name: /Tritanium/ })).toContainText("Highest buy");
  await expect(page.getByRole("row", { name: /Tritanium/ })).not.toHaveAttribute("tabindex");
  expect(await page.getByRole("row", { name: /Composite Armor Plate/ }).evaluate((row) => row.getBoundingClientRect().height)).toBeLessThanOrEqual(36);
  await expect(page.getByText("Grand Total")).toHaveCount(0);
});

test("Worksheet keeps every economics column in a local scroller at 375px", async ({ page }) => {
  await installApiFixtures(page);
  await page.setViewportSize({ width: 375, height: 812 });
  await page.goto(`/builds/${plannedBuild.id}?view=worksheet`);

  const scroller = page.locator("[data-operational-table-scroll]");
  await expect(scroller).toBeVisible();
  await expect(page.getByRole("columnheader", { name: "Pricing" })).toBeAttached();
  await expect(page.getByRole("columnheader", { name: "Unit Cost" })).toBeAttached();
  await expect(page.getByRole("columnheader", { name: "Total Value ⓘ" })).toBeAttached();
  expect(await scroller.evaluate((element) => ({ clientWidth: element.clientWidth, scrollWidth: element.scrollWidth }))).toMatchObject({ clientWidth: expect.any(Number), scrollWidth: expect.any(Number) });
  expect(await scroller.evaluate((element) => element.scrollWidth > element.clientWidth)).toBe(true);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= document.documentElement.clientWidth)).toBe(true);
});
