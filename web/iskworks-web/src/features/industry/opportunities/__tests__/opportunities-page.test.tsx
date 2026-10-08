import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes } from "react-router";
import { beforeEach, describe, expect, test, vi } from "vitest";

import type { Build, FacilityProfile } from "../../../../api/industry";
import type { OpportunityCandidate, OpportunityEvaluation, ProfitabilityScopeDefinition } from "../../../../api/opportunities";
import { OpportunitiesPage } from "../opportunities-page";

const industryApi = vi.hoisted(() => ({
  listFacilities: vi.fn(),
  createBuild: vi.fn(),
}));

const opportunitiesApi = vi.hoisted(() => ({
  listOpportunityScopes: vi.fn(),
  evaluateOpportunities: vi.fn(),
  requestOpportunityRefresh: vi.fn(),
}));

vi.mock("../../../../api/industry", async () => {
  const actual = await vi.importActual<typeof import("../../../../api/industry")>("../../../../api/industry");
  return { ...actual, ...industryApi };
});

vi.mock("../../../../api/opportunities", async () => {
  const actual = await vi.importActual<typeof import("../../../../api/opportunities")>("../../../../api/opportunities");
  return { ...actual, ...opportunitiesApi };
});

function scope(overrides: Partial<ProfitabilityScopeDefinition> = {}): ProfitabilityScopeDefinition {
  return {
    id: "t1-frigates",
    label: "T1 Frigates",
    family: "Ships",
    description: "Published Tech I T1 Frigates with all immediate materials purchased from sell orders.",
    limitations: [],
    recipeKind: "manufacturing",
    ...overrides,
  };
}

function facility(overrides: Partial<FacilityProfile> = {}): FacilityProfile {
  return {
    id: "facility-1",
    workspaceId: "workspace-1",
    name: "C-J6MT — GEZ — T2 Ships, Comps, Structures",
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

function candidate(overrides: Partial<OpportunityCandidate> = {}): OpportunityCandidate {
  return {
    scopeId: "t1-frigates",
    productTypeId: 5_876,
    productName: "Rifter",
    recipe: { kind: "manufacturing", blueprintTypeId: 68_357, blueprintName: "Rifter Blueprint" },
    sdeVersion: "3389399",
    recipeFingerprint: "fingerprint",
    runs: 1,
    outputQuantity: 1,
    baseDurationSeconds: 1_800,
    effectiveDurationSeconds: 1_800,
    materialEfficiency: 10,
    timeEfficiency: 20,
    facilityProfileId: "facility-1",
    facilityRevision: 1,
    metrics: {
      materialCost: "1820000",
      installationCost: "5820",
      totalEstimatedManufacturingCost: "1825820",
      estimatedOutputValue: "5140000",
      estimatedGrossProfit: "3314180",
      grossMarginPercent: "64.46",
      estimatedGrossProfitPerUnit: "3314180",
      estimatedGrossProfitPerRun: "3314180",
      estimatedGrossProfitPerManufacturingHour: "6628360",
      capitalRequired: "1825820",
    },
    completeness: "complete",
    warnings: [],
    missingPriceTypeIds: [],
    valuations: {
      sellSide: {
        revenue: "5140000",
        grossProfit: "3314180",
        grossMarginPercent: "64.46",
        grossProfitPerManufacturingHour: "6628360",
        completeness: "complete",
      },
      immediateLiquidation: {
        revenue: "3810000",
        grossProfit: "1984180",
        grossMarginPercent: "52.08",
        grossProfitPerManufacturingHour: "3968360",
        completeness: "complete",
      },
    },
    outputMarketEvidence: null,
    eivBasis: { complete: true, requiredMaterialCount: 1, observedMaterialCount: 1, missingMaterials: [] },
    eligibility: { status: "eligible", exclusionReasons: [] },
    quality: { evidenceQuality: "strong" },
    ...overrides,
  };
}

function evaluation(overrides: Partial<OpportunityEvaluation> = {}): OpportunityEvaluation {
  return {
    context: {
      scopeId: "t1-frigates",
      facilityProfileId: "facility-1",
      facilityRevision: 1,
      marketRegionId: 10_000_002,
      marketLocationId: 60_003_760,
      materialEfficiency: 10,
      timeEfficiency: 20,
      runs: 1,
      materialPricingPolicy: "acquireQuantityFromSellOrders",
      outputPricingPolicy: "lowestSell",
      inventoryReuseEnabled: false,
      recursiveComponentExpansionEnabled: false,
    },
    calculatedAt: "2026-08-20T12:00:00Z",
    elapsedMilliseconds: 100,
    candidateCount: 2,
    completeCount: 2,
    incompleteCount: 0,
    defaultRankingEligibleCount: 2,
    excludedCount: 0,
    strongEvidenceCount: 2,
    qualifiedEvidenceCount: 0,
    weakEvidenceCount: 0,
    readiness: {
      registeredAt: "2026-08-20T12:00:00Z",
      localReadAt: "2026-08-20T12:00:00Z",
      requiredMaterialTypeCount: 1,
      requiredOutputTypeCount: 2,
      marketFreshCount: 3,
      marketStaleCount: 0,
      marketMissingCount: 0,
      marketPendingCount: 0,
      marketFailedCount: 0,
      oldestMarketObservedAt: "2026-08-20T11:50:00Z",
      newestMarketObservedAt: "2026-08-20T11:55:00Z",
      adjustedPrices: {
        state: "fresh",
        usable: true,
        observedAt: "2026-08-20T11:50:00Z",
        ageSeconds: 600,
        refreshPending: false,
        lastRefreshError: null,
      },
      systemIndexSource: "manual",
      systemIndex: {
        state: "fresh",
        usable: true,
        observedAt: null,
        ageSeconds: null,
        refreshPending: false,
        lastRefreshError: null,
      },
      staleButCompleteCount: 0,
      refreshPending: false,
    },
    candidates: [
      candidate(),
      candidate({
        productTypeId: 54_731,
        productName: "Skybreaker",
        recipe: { kind: "manufacturing", blueprintTypeId: 54_842, blueprintName: "Skybreaker Blueprint" },
        quality: { evidenceQuality: "weak" },
        warnings: [{ kind: "thinOutputBook", message: "Visible sell-order depth for the output is thin.", typeIds: [54_731] }],
      }),
    ],
    rankings: {
      sellSideGrossProfit: [],
      immediateLiquidationGrossProfit: [],
      sellSideGrossMargin: [],
      immediateLiquidationGrossMargin: [],
      sellSideGrossProfitPerManufacturingHour: [],
      immediateLiquidationGrossProfitPerManufacturingHour: [],
    },
    excludedCosts: [],
    warnings: [],
    assumptions: [],
    exclusions: [],
    ...overrides,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  industryApi.listFacilities.mockResolvedValue([facility()]);
  opportunitiesApi.listOpportunityScopes.mockResolvedValue([scope(), scope({ id: "t1-battleships", label: "T1 Battleships" })]);
  opportunitiesApi.evaluateOpportunities.mockResolvedValue(evaluation());
});

function build(overrides: Partial<Build> = {}): Build {
  return {
    id: "build-99",
    workspaceId: "workspace-1",
    ownerId: "owner-1",
    name: "Rifter",
    recipe: {
      kind: "manufacturing",
      sourceSdeDatasetId: "dataset-1",
      sourceSdeVersion: "3389399",
      blueprintTypeId: 68_357,
      blueprintName: "Rifter Blueprint",
      durationSecondsPerRun: 1_800,
      materials: [],
      products: [],
      fingerprint: "recipe",
    },
    runs: 1,
    notes: "",
    revision: 1,
    createdAt: "2026-08-20T12:00:00Z",
    updatedAt: "2026-08-20T12:00:00Z",
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

function refreshAcceptance(overrides: Partial<import("../../../../api/opportunities").OpportunityRefreshAcceptance> = {}) {
  return {
    scopeId: "t1-frigates" as const,
    market: "accepted" as const,
    adjustedPrices: "notRequired" as const,
    systemIndex: "alreadyPending" as const,
    acceptedAt: "2026-08-20T12:00:05Z",
    ...overrides,
  };
}

describe("OpportunitiesPage", () => {
  test("loads scopes, defaults to the first one, and evaluates it", async () => {
    render(
      <MemoryRouter>
        <OpportunitiesPage />
      </MemoryRouter>,
    );

    const scopeSelect = await screen.findByLabelText("Category");
    expect(within(scopeSelect).getByRole("option", { name: "T1 Frigates" })).toBeInTheDocument();
    expect(within(scopeSelect).getByRole("option", { name: "T1 Battleships" })).toBeInTheDocument();

    await waitFor(() => {
      expect(opportunitiesApi.evaluateOpportunities).toHaveBeenCalledWith(
        expect.objectContaining({
          scopeId: "t1-frigates",
          facilityProfileId: "facility-1",
          marketScope: { regionId: 10_000_002, locationId: 60_003_760 },
        }),
        expect.anything(),
      );
    });

    expect(await screen.findByRole("cell", { name: /Rifter/ })).toBeInTheDocument();
    expect(screen.getByRole("cell", { name: /Skybreaker/ })).toBeInTheDocument();
  });

  test("switching the scope selector re-evaluates the new scope", async () => {
    const user = userEvent.setup();
    render(
      <MemoryRouter>
        <OpportunitiesPage />
      </MemoryRouter>,
    );

    await screen.findByRole("cell", { name: /Rifter/ });
    await user.selectOptions(screen.getByLabelText("Category"), "t1-battleships");

    await waitFor(() => {
      expect(opportunitiesApi.evaluateOpportunities).toHaveBeenCalledWith(
        expect.objectContaining({ scopeId: "t1-battleships" }),
        expect.anything(),
      );
    });
  });

  test("shows an empty state when no manufacturing facility is configured", async () => {
    industryApi.listFacilities.mockResolvedValue([facility({ role: "reaction" })]);
    render(
      <MemoryRouter>
        <OpportunitiesPage />
      </MemoryRouter>,
    );

    expect(await screen.findByText("No manufacturing facility configured")).toBeInTheDocument();
    expect(opportunitiesApi.evaluateOpportunities).not.toHaveBeenCalled();
  });

  test("switching valuation mode swaps revenue/profit/margin/profit-per-hour without a re-evaluate", async () => {
    const user = userEvent.setup();
    render(
      <MemoryRouter>
        <OpportunitiesPage />
      </MemoryRouter>,
    );

    await screen.findByRole("cell", { name: /Rifter/ });
    expect(screen.getByRole("row", { name: /Rifter/ })).toHaveTextContent("5.1M");

    const callCountBeforeToggle = opportunitiesApi.evaluateOpportunities.mock.calls.length;
    await user.click(screen.getByRole("button", { name: "Liquidation" }));

    expect(screen.getByRole("row", { name: /Rifter/ })).toHaveTextContent("3.8M");
    expect(opportunitiesApi.evaluateOpportunities.mock.calls.length).toBe(callCountBeforeToggle);
  });

  test("row order follows the backend's ranking for the active sort, not insertion order", async () => {
    // Candidates array order is [Rifter, Skybreaker] (insertion order), but
    // the backend's own sellSideGrossProfitPerManufacturingHour ranking
    // (the default active sort) puts Skybreaker first -- if the table ever
    // fell back to insertion order or a naively re-derived client sort,
    // this would catch the drift.
    opportunitiesApi.evaluateOpportunities.mockResolvedValue(
      evaluation({
        rankings: {
          sellSideGrossProfit: [],
          immediateLiquidationGrossProfit: [],
          sellSideGrossMargin: [],
          immediateLiquidationGrossMargin: [],
          sellSideGrossProfitPerManufacturingHour: [
            { productTypeId: 54_731, recipeTypeId: 54_842 },
            { productTypeId: 5_876, recipeTypeId: 68_357 },
          ],
          immediateLiquidationGrossProfitPerManufacturingHour: [],
        },
      }),
    );
    render(
      <MemoryRouter>
        <OpportunitiesPage />
      </MemoryRouter>,
    );

    await screen.findByRole("cell", { name: /Rifter/ });
    const rows = screen.getAllByRole("row").filter((row) => row.hasAttribute("data-row-key"));
    expect(rows[0]).toHaveTextContent("Skybreaker");
    expect(rows[1]).toHaveTextContent("Rifter");
  });

  test("clicking a sortable column header toggles direction and reorders rows", async () => {
    const user = userEvent.setup();
    opportunitiesApi.evaluateOpportunities.mockResolvedValue(
      evaluation({
        rankings: {
          sellSideGrossProfit: [
            { productTypeId: 5_876, recipeTypeId: 68_357 },
            { productTypeId: 54_731, recipeTypeId: 54_842 },
          ],
          immediateLiquidationGrossProfit: [],
          sellSideGrossMargin: [],
          immediateLiquidationGrossMargin: [],
          sellSideGrossProfitPerManufacturingHour: [
            { productTypeId: 54_731, recipeTypeId: 54_842 },
            { productTypeId: 5_876, recipeTypeId: 68_357 },
          ],
          immediateLiquidationGrossProfitPerManufacturingHour: [],
        },
      }),
    );
    render(
      <MemoryRouter>
        <OpportunitiesPage />
      </MemoryRouter>,
    );

    await screen.findByRole("cell", { name: /Rifter/ });
    const profitHeader = screen.getByRole("columnheader", { name: /Profit\/h/ });
    expect(profitHeader).toHaveAttribute("aria-sort", "descending");

    await user.click(screen.getByRole("button", { name: "Profit" }));
    let rows = screen.getAllByRole("row").filter((row) => row.hasAttribute("data-row-key"));
    expect(rows[0]).toHaveTextContent("Rifter");
    expect(screen.getByRole("columnheader", { name: "Profit" })).toHaveAttribute("aria-sort", "descending");

    await user.click(screen.getByRole("button", { name: "Profit" }));
    rows = screen.getAllByRole("row").filter((row) => row.hasAttribute("data-row-key"));
    expect(rows[0]).toHaveTextContent("Skybreaker");
    expect(screen.getByRole("columnheader", { name: "Profit" })).toHaveAttribute("aria-sort", "ascending");
  });

  test("excluded candidates never appear in the ranked table", async () => {
    opportunitiesApi.evaluateOpportunities.mockResolvedValue(
      evaluation({
        candidates: [
          candidate(),
          candidate({
            productTypeId: 77_114,
            productName: "Metamorphosis",
            eligibility: {
              status: "excludedFromDefaultRanking",
              exclusionReasons: [
                { code: "sdeSpecialEdition", message: "Excluded.", marketGroupId: 1_612, marketGroupName: "Special Edition Ships" },
              ],
            },
          }),
        ],
      }),
    );
    render(
      <MemoryRouter>
        <OpportunitiesPage />
      </MemoryRouter>,
    );

    await screen.findByRole("cell", { name: /Rifter/ });
    expect(screen.queryByText("Metamorphosis")).not.toBeInTheDocument();
  });

  test("excluded candidates appear in the collapsed excluded section, expandable to show the exclusion reason", async () => {
    opportunitiesApi.evaluateOpportunities.mockResolvedValue(
      evaluation({
        candidates: [
          candidate(),
          candidate({
            productTypeId: 77_114,
            productName: "Metamorphosis",
            eligibility: {
              status: "excludedFromDefaultRanking",
              exclusionReasons: [
                { code: "sdeSpecialEdition", message: "Excluded as a Special Edition Ships variant.", marketGroupId: 1_612, marketGroupName: "Special Edition Ships" },
              ],
            },
          }),
        ],
      }),
    );
    const user = userEvent.setup();
    render(
      <MemoryRouter>
        <OpportunitiesPage />
      </MemoryRouter>,
    );

    await screen.findByRole("cell", { name: /Rifter/ });
    const toggle = screen.getByRole("button", { name: /Excluded \/ nonstandard recipes \(1\)/ });
    expect(screen.queryByText("Metamorphosis")).not.toBeInTheDocument();

    await user.click(toggle);

    expect(screen.getByText("Metamorphosis")).toBeInTheDocument();
    expect(screen.getByText("Special Edition Ships")).toBeInTheDocument();
    // Still absent from the ranked table itself, not just visually separate.
    expect(screen.queryByRole("cell", { name: "Metamorphosis" })).not.toBeInTheDocument();
  });
});

describe("OpportunitiesPage freshness banner", () => {
  test("shows the market data age and the freshness target derived from a stale candidate's warning", async () => {
    const oldestMarketObservedAt = new Date(Date.now() - 3_600_000).toISOString();
    opportunitiesApi.evaluateOpportunities.mockResolvedValue(
      evaluation({
        readiness: {
          registeredAt: "2026-08-20T12:00:00Z",
          localReadAt: "2026-08-20T12:00:00Z",
          requiredMaterialTypeCount: 1,
          requiredOutputTypeCount: 2,
          marketFreshCount: 1,
          marketStaleCount: 1,
          marketMissingCount: 0,
          marketPendingCount: 0,
          marketFailedCount: 0,
          oldestMarketObservedAt,
          newestMarketObservedAt: oldestMarketObservedAt,
          adjustedPrices: { state: "fresh", usable: true, observedAt: null, ageSeconds: null, refreshPending: false, lastRefreshError: null },
          systemIndexSource: "manual",
          systemIndex: { state: "fresh", usable: true, observedAt: null, ageSeconds: null, refreshPending: false, lastRefreshError: null },
          staleButCompleteCount: 1,
          refreshPending: false,
        },
        candidates: [
          candidate(),
          candidate({
            productTypeId: 54_731,
            productName: "Skybreaker",
            recipe: { kind: "manufacturing", blueprintTypeId: 54_842, blueprintName: "Skybreaker Blueprint" },
            quality: { evidenceQuality: "qualified" },
            warnings: [
              {
                kind: "staleMarketEvidence",
                message: "Observed market data is older than the freshness target.",
                typeIds: [54_731],
                details: {
                  type: "staleMarketEvidence",
                  observedAt: oldestMarketObservedAt,
                  ageSeconds: 3_600,
                  freshnessTargetSeconds: 900,
                  refreshState: "stale",
                },
              },
            ],
          }),
        ],
      }),
    );
    render(
      <MemoryRouter>
        <OpportunitiesPage />
      </MemoryRouter>,
    );

    await screen.findByRole("cell", { name: /Rifter/ });
    expect(screen.getByText(/Market data updated 1h ago/)).toBeInTheDocument();
    expect(screen.getByText(/15m target/)).toBeInTheDocument();
  });

  test("omits the freshness target clause when no candidate is stale", async () => {
    render(
      <MemoryRouter>
        <OpportunitiesPage />
      </MemoryRouter>,
    );

    await screen.findByRole("cell", { name: /Rifter/ });
    expect(screen.getByText(/Market data updated/)).toBeInTheDocument();
    expect(screen.queryByText(/target/)).not.toBeInTheDocument();
  });

  test("shows a non-blocking note when a background refresh failed but old evidence is still usable", async () => {
    opportunitiesApi.evaluateOpportunities.mockResolvedValue(
      evaluation({
        readiness: {
          registeredAt: "2026-08-20T12:00:00Z",
          localReadAt: "2026-08-20T12:00:00Z",
          requiredMaterialTypeCount: 1,
          requiredOutputTypeCount: 2,
          marketFreshCount: 3,
          marketStaleCount: 0,
          marketMissingCount: 0,
          marketPendingCount: 0,
          marketFailedCount: 0,
          oldestMarketObservedAt: "2026-08-20T11:50:00Z",
          newestMarketObservedAt: "2026-08-20T11:55:00Z",
          adjustedPrices: {
            state: "fresh",
            usable: true,
            observedAt: "2026-08-20T11:50:00Z",
            ageSeconds: 600,
            refreshPending: false,
            lastRefreshError: "ESI request timed out",
          },
          systemIndexSource: "manual",
          systemIndex: { state: "fresh", usable: true, observedAt: null, ageSeconds: null, refreshPending: false, lastRefreshError: null },
          staleButCompleteCount: 0,
          refreshPending: false,
        },
      }),
    );
    render(
      <MemoryRouter>
        <OpportunitiesPage />
      </MemoryRouter>,
    );

    await screen.findByRole("cell", { name: /Rifter/ });
    expect(screen.getByText(/Last background refresh failed/)).toBeInTheDocument();
    expect(screen.getByText(/ESI request timed out/)).toBeInTheDocument();
    // The evaluation itself succeeded, so the table stays visible and interactive.
    expect(screen.getByRole("cell", { name: /Rifter/ })).toBeInTheDocument();
  });

  test("clicking Refresh calls requestOpportunityRefresh with the current context and shows the returned dispositions without blocking the table", async () => {
    opportunitiesApi.requestOpportunityRefresh.mockResolvedValue(refreshAcceptance());
    const user = userEvent.setup();
    render(
      <MemoryRouter>
        <OpportunitiesPage />
      </MemoryRouter>,
    );

    await screen.findByRole("cell", { name: /Rifter/ });
    await user.click(screen.getByRole("button", { name: "Refresh" }));

    expect(opportunitiesApi.requestOpportunityRefresh).toHaveBeenCalledWith(
      expect.objectContaining({
        scopeId: "t1-frigates",
        facilityProfileId: "facility-1",
        marketScope: { regionId: 10_000_002, locationId: 60_003_760 },
      }),
    );
    expect(await screen.findByText(/Market refresh accepted/)).toBeInTheDocument();
    expect(screen.getByText(/Adjusted prices not required/)).toBeInTheDocument();
    expect(screen.getByText(/System index already pending/)).toBeInTheDocument();
    // Table stays visible and interactive throughout.
    expect(screen.getByRole("cell", { name: /Rifter/ })).toBeInTheDocument();
  });
});

describe("OpportunitiesPage context strip", () => {
  test("shows the resolved facility/scope/ME-TE and re-evaluates with the applied values on Edit -> Apply", async () => {
    industryApi.listFacilities.mockResolvedValue([facility(), facility({ id: "facility-2", name: "Jita IV - Moon 4" })]);
    const user = userEvent.setup();
    render(
      <MemoryRouter>
        <OpportunitiesPage />
      </MemoryRouter>,
    );

    await screen.findByRole("cell", { name: /Rifter/ });
    expect(screen.getByText(/C-J6MT/)).toBeInTheDocument();
    expect(screen.getByText(/ME10\/TE20/)).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Edit" }));
    await user.selectOptions(screen.getByLabelText("Facility"), "facility-2");
    await user.clear(screen.getByLabelText("Material Efficiency (0-10)"));
    await user.type(screen.getByLabelText("Material Efficiency (0-10)"), "8");
    await user.clear(screen.getByLabelText("Time Efficiency (0-20)"));
    await user.type(screen.getByLabelText("Time Efficiency (0-20)"), "16");
    await user.click(screen.getByRole("button", { name: "Apply" }));

    await waitFor(() => {
      expect(opportunitiesApi.evaluateOpportunities).toHaveBeenLastCalledWith(
        expect.objectContaining({
          facilityProfileId: "facility-2",
          marketScope: { regionId: 10_000_002, locationId: 60_003_760 },
          materialEfficiency: 8,
          timeEfficiency: 16,
        }),
        expect.anything(),
      );
    });
  });

  test("Create Build (inspector) calls createBuild with the candidate's blueprintTypeId and navigates to the new Build", async () => {
    industryApi.createBuild.mockResolvedValue(build());
    const user = userEvent.setup();
    render(
      <MemoryRouter initialEntries={["/opportunities"]}>
        <Routes>
          <Route element={<OpportunitiesPage />} path="/opportunities" />
          <Route element={<div>Build workspace for build-99</div>} path="/builds/:buildId" />
        </Routes>
      </MemoryRouter>,
    );

    await screen.findByRole("cell", { name: /Rifter/ });
    await user.click(screen.getByRole("row", { name: /Rifter/ }));
    const heading = await screen.findByRole("heading", { name: "Rifter" });
    const inspector = heading.closest("aside") as HTMLElement;

    await user.click(within(inspector).getByRole("button", { name: "Create Build" }));

    expect(industryApi.createBuild).toHaveBeenCalledWith({
      name: "Rifter",
      recipe: { mode: "manufacturing", blueprintTypeId: 68_357 },
      runs: 1,
      notes: "",
    });
    expect(await screen.findByText("Build workspace for build-99")).toBeInTheDocument();
  });

  test("Create Build (row) only shows on the selected row and calls createBuild for that candidate", async () => {
    industryApi.createBuild.mockResolvedValue(build({ id: "build-42" }));
    const user = userEvent.setup();
    render(
      <MemoryRouter initialEntries={["/opportunities"]}>
        <Routes>
          <Route element={<OpportunitiesPage />} path="/opportunities" />
          <Route element={<div>Build workspace for build-42</div>} path="/builds/:buildId" />
        </Routes>
      </MemoryRouter>,
    );

    await screen.findByRole("cell", { name: /Rifter/ });
    expect(screen.queryByRole("button", { name: "Create Build" })).not.toBeInTheDocument();

    await user.click(screen.getByRole("row", { name: /Rifter/ }));
    const rifterRow = screen.getByRole("row", { name: /Rifter/ });
    await user.click(within(rifterRow).getByRole("button", { name: "Create Build" }));

    expect(industryApi.createBuild).toHaveBeenCalledWith(
      expect.objectContaining({ recipe: { mode: "manufacturing", blueprintTypeId: 68_357 } }),
    );
    expect(await screen.findByText("Build workspace for build-42")).toBeInTheDocument();
  });

  test("Cancel closes the editor without applying or re-evaluating", async () => {
    const user = userEvent.setup();
    render(
      <MemoryRouter>
        <OpportunitiesPage />
      </MemoryRouter>,
    );

    await screen.findByRole("cell", { name: /Rifter/ });
    const callCountBeforeEdit = opportunitiesApi.evaluateOpportunities.mock.calls.length;

    await user.click(screen.getByRole("button", { name: "Edit" }));
    await user.clear(screen.getByLabelText("Material Efficiency (0-10)"));
    await user.type(screen.getByLabelText("Material Efficiency (0-10)"), "5");
    await user.click(screen.getByRole("button", { name: "Cancel" }));

    expect(screen.queryByLabelText("Material Efficiency (0-10)")).not.toBeInTheDocument();
    expect(opportunitiesApi.evaluateOpportunities.mock.calls.length).toBe(callCountBeforeEdit);
  });
});

describe("OpportunitiesPage filters", () => {
  function filterEvaluation() {
    return evaluation({
      candidates: [
        candidate(), // Rifter: strong, margin 64.46%, capital 1,825,820 ISK, duration 1,800s (30m)
        candidate({
          productTypeId: 54_731,
          productName: "Skybreaker",
          recipe: { kind: "manufacturing", blueprintTypeId: 54_842, blueprintName: "Skybreaker Blueprint" },
          effectiveDurationSeconds: 90_000, // 25h
          quality: { evidenceQuality: "weak" },
          warnings: [{ kind: "thinOutputBook", message: "Visible sell-order depth for the output is thin.", typeIds: [54_731] }],
          metrics: {
            materialCost: "40000000",
            installationCost: "100000",
            totalEstimatedManufacturingCost: "40100000",
            estimatedOutputValue: "42000000",
            estimatedGrossProfit: "1900000",
            grossMarginPercent: "4.74",
            estimatedGrossProfitPerUnit: "1900000",
            estimatedGrossProfitPerRun: "1900000",
            estimatedGrossProfitPerManufacturingHour: "76000",
            capitalRequired: "40100000",
          },
          valuations: {
            sellSide: {
              revenue: "42000000",
              grossProfit: "1900000",
              grossMarginPercent: "4.74",
              grossProfitPerManufacturingHour: "76000",
              completeness: "complete",
            },
            immediateLiquidation: {
              revenue: "41000000",
              grossProfit: "900000",
              grossMarginPercent: "2.25",
              grossProfitPerManufacturingHour: "36000",
              completeness: "complete",
            },
          },
        }),
      ],
    });
  }

  async function renderWithFiltersOpen() {
    const user = userEvent.setup();
    render(
      <MemoryRouter>
        <OpportunitiesPage />
      </MemoryRouter>,
    );
    await screen.findByRole("cell", { name: /Rifter/ });
    await user.click(screen.getByRole("button", { name: "Filters" }));
    return user;
  }

  test("unchecking the Weak evidence-quality checkbox hides weak candidates", async () => {
    opportunitiesApi.evaluateOpportunities.mockResolvedValue(filterEvaluation());
    const user = await renderWithFiltersOpen();
    expect(screen.getByText("Skybreaker")).toBeInTheDocument();

    await user.click(screen.getByRole("checkbox", { name: "Weak" }));

    expect(screen.queryByText("Skybreaker")).not.toBeInTheDocument();
    expect(screen.getByText("Rifter")).toBeInTheDocument();
  });

  test("the Hide Weak evidence checkbox hides weak candidates", async () => {
    opportunitiesApi.evaluateOpportunities.mockResolvedValue(filterEvaluation());
    const user = await renderWithFiltersOpen();

    await user.click(screen.getByRole("checkbox", { name: "Hide Weak evidence" }));

    expect(screen.queryByText("Skybreaker")).not.toBeInTheDocument();
    expect(screen.getByText("Rifter")).toBeInTheDocument();
  });

  test("minimum gross margin filters out low-margin candidates", async () => {
    opportunitiesApi.evaluateOpportunities.mockResolvedValue(filterEvaluation());
    await renderWithFiltersOpen();

    fireEvent.change(screen.getByLabelText("Min. gross margin"), { target: { value: "20" } });

    expect(screen.queryByText("Skybreaker")).not.toBeInTheDocument();
    expect(screen.getByText("Rifter")).toBeInTheDocument();
  });

  test("maximum capital required filters out expensive candidates", async () => {
    opportunitiesApi.evaluateOpportunities.mockResolvedValue(filterEvaluation());
    await renderWithFiltersOpen();

    fireEvent.change(screen.getByLabelText("Max capital required"), { target: { value: "10000000" } });

    expect(screen.queryByText("Skybreaker")).not.toBeInTheDocument();
    expect(screen.getByText("Rifter")).toBeInTheDocument();
  });

  test("maximum duration filters out long-running candidates", async () => {
    opportunitiesApi.evaluateOpportunities.mockResolvedValue(filterEvaluation());
    await renderWithFiltersOpen();

    fireEvent.change(screen.getByLabelText("Max duration"), { target: { value: "3" } }); // 12 hours

    expect(screen.queryByText("Skybreaker")).not.toBeInTheDocument();
    expect(screen.getByText("Rifter")).toBeInTheDocument();
  });

  test("combined filters intersect, and Reset restores the full candidate list", async () => {
    opportunitiesApi.evaluateOpportunities.mockResolvedValue(filterEvaluation());
    const user = await renderWithFiltersOpen();

    await user.click(screen.getByRole("checkbox", { name: "Weak" }));
    fireEvent.change(screen.getByLabelText("Min. gross margin"), { target: { value: "50" } });
    expect(screen.queryByText("Skybreaker")).not.toBeInTheDocument();
    expect(screen.getByText("Rifter")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Reset" }));

    expect(screen.getByText("Skybreaker")).toBeInTheDocument();
    expect(screen.getByText("Rifter")).toBeInTheDocument();
  });

  test("filtering out every candidate shows the filter-specific empty state with a reset action, not the generic one", async () => {
    opportunitiesApi.evaluateOpportunities.mockResolvedValue(filterEvaluation());
    const user = await renderWithFiltersOpen();

    await user.click(screen.getByRole("checkbox", { name: "Strong" }));
    await user.click(screen.getByRole("checkbox", { name: "Qualified" }));
    await user.click(screen.getByRole("checkbox", { name: "Weak" }));

    expect(await screen.findByText("No results match your filters")).toBeInTheDocument();
    expect(screen.queryByText("No eligible candidates")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Reset filters" }));
    expect(screen.getByText("Rifter")).toBeInTheDocument();
  });

  test("zero eligible candidates before any filtering shows the generic empty state, no reset action", async () => {
    opportunitiesApi.evaluateOpportunities.mockResolvedValue(
      evaluation({
        candidates: [
          candidate({
            eligibility: {
              status: "excludedFromDefaultRanking",
              exclusionReasons: [
                { code: "sdeSpecialEdition", message: "Excluded.", marketGroupId: 1_612, marketGroupName: "Special Edition Ships" },
              ],
            },
          }),
        ],
        defaultRankingEligibleCount: 0,
        excludedCount: 1,
      }),
    );
    render(
      <MemoryRouter>
        <OpportunitiesPage />
      </MemoryRouter>,
    );

    expect(await screen.findByText("No eligible candidates")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Reset filters" })).not.toBeInTheDocument();
  });
});

describe("OpportunitiesPage candidate inspector", () => {
  test("clicking a row opens the inspector, updates the URL, and closing removes it", async () => {
    const user = userEvent.setup();
    render(
      <MemoryRouter>
        <OpportunitiesPage />
      </MemoryRouter>,
    );

    await screen.findByRole("cell", { name: /Rifter/ });
    expect(screen.queryByRole("heading", { name: "Rifter" })).not.toBeInTheDocument();

    await user.click(screen.getByRole("row", { name: /Rifter/ }));

    expect(await screen.findByRole("heading", { name: "Rifter" })).toBeInTheDocument();
    expect(screen.getByText("Rifter Blueprint")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Close item inspector" }));
    expect(screen.queryByRole("heading", { name: "Rifter" })).not.toBeInTheDocument();
  });

  test("closing the inspector with Escape returns focus to the triggering row", async () => {
    const user = userEvent.setup();
    render(
      <MemoryRouter>
        <OpportunitiesPage />
      </MemoryRouter>,
    );

    await screen.findByRole("cell", { name: /Rifter/ });
    const row = screen.getByRole("row", { name: /Rifter/ });
    await user.click(row);
    await screen.findByRole("heading", { name: "Rifter" });

    await user.keyboard("{Escape}");

    expect(screen.queryByRole("heading", { name: "Rifter" })).not.toBeInTheDocument();
    expect(row).toHaveFocus();
  });

  test("a strong-evidence candidate with no warnings shows the no-warnings copy", async () => {
    const user = userEvent.setup();
    render(
      <MemoryRouter>
        <OpportunitiesPage />
      </MemoryRouter>,
    );

    await screen.findByRole("cell", { name: /Rifter/ });
    await user.click(screen.getByRole("row", { name: /Rifter/ }));

    const evidenceSection = screen.getByRole("region", { name: "Evidence Quality" });
    expect(evidenceSection).toHaveTextContent("No evidence warnings for this candidate.");
  });

  test("a weak-evidence candidate renders its warning message and EIV Basis section when incomplete", async () => {
    opportunitiesApi.evaluateOpportunities.mockResolvedValue(
      evaluation({
        candidates: [
          candidate(),
          candidate({
            productTypeId: 54_731,
            productName: "Skybreaker",
            recipe: { kind: "manufacturing", blueprintTypeId: 54_842, blueprintName: "Skybreaker Blueprint" },
            quality: { evidenceQuality: "weak" },
            warnings: [{ kind: "thinOutputBook", message: "Visible sell-order depth for the output is thin.", typeIds: [54_731] }],
            eivBasis: {
              complete: false,
              requiredMaterialCount: 3,
              observedMaterialCount: 2,
              missingMaterials: [{ typeId: 34, typeName: "Tritanium" }],
            },
          }),
        ],
      }),
    );
    const user = userEvent.setup();
    render(
      <MemoryRouter>
        <OpportunitiesPage />
      </MemoryRouter>,
    );

    await screen.findByRole("cell", { name: /Rifter/ });
    await user.click(screen.getByRole("row", { name: /Skybreaker/ }));

    expect(await screen.findByRole("heading", { name: "Skybreaker" })).toBeInTheDocument();
    const evidenceSection = screen.getByRole("region", { name: "Evidence Quality" });
    expect(evidenceSection).toHaveTextContent("Visible sell-order depth for the output is thin.");

    const eivSection = screen.getByRole("region", { name: "EIV Basis" });
    expect(eivSection).toHaveTextContent("2 / 3");
    expect(eivSection).toHaveTextContent("Tritanium");
  });

  test("Skybreaker-shaped fixture renders all three structured warning kinds with their detail payloads", async () => {
    opportunitiesApi.evaluateOpportunities.mockResolvedValue(
      evaluation({
        candidates: [
          candidate(),
          candidate({
            productTypeId: 54_731,
            productName: "Skybreaker",
            recipe: { kind: "manufacturing", blueprintTypeId: 54_842, blueprintName: "Skybreaker Blueprint" },
            quality: { evidenceQuality: "weak" },
            eivBasis: {
              complete: false,
              requiredMaterialCount: 3,
              observedMaterialCount: 2,
              missingMaterials: [{ typeId: 34, typeName: "Tritanium" }],
            },
            warnings: [
              {
                kind: "thinOutputBook",
                message: "Visible sell-order depth for the output is thin.",
                typeIds: [54_731],
                details: {
                  type: "thinBook",
                  reasons: ["bestLevelAtOrBelowOutputQuantity", "visibleVolumeBelowTwentyRunEquivalents"],
                },
              },
              {
                kind: "staleMarketEvidence",
                message: "Observed market data is older than the freshness target.",
                typeIds: [54_731],
                details: {
                  type: "staleMarketEvidence",
                  observedAt: "2026-08-20T11:00:00Z",
                  ageSeconds: 8_100,
                  freshnessTargetSeconds: 900,
                  refreshState: "stale",
                },
              },
              {
                kind: "incompleteEivBasis",
                message: "Some material adjusted-prices are missing.",
                typeIds: [34],
                details: { type: "incompleteEivBasis", missingMaterials: [{ typeId: 34, typeName: "Tritanium" }] },
              },
            ],
          }),
        ],
      }),
    );
    const user = userEvent.setup();
    render(
      <MemoryRouter>
        <OpportunitiesPage />
      </MemoryRouter>,
    );

    await screen.findByRole("cell", { name: /Rifter/ });
    const skybreakerRow = screen.getByRole("row", { name: /Skybreaker/ });
    expect(within(skybreakerRow).getByText("⚠ 3")).toHaveAttribute(
      "title",
      "Thin output book, Stale market evidence, Incomplete EIV basis",
    );

    await user.click(skybreakerRow);
    await screen.findByRole("heading", { name: "Skybreaker" });

    const evidenceSection = screen.getByRole("region", { name: "Evidence Quality" });
    expect(evidenceSection).toHaveTextContent("Thin output book");
    expect(evidenceSection).toHaveTextContent("Best price level doesn't cover the output quantity");
    expect(evidenceSection).toHaveTextContent("Stale market evidence");
    expect(evidenceSection).toHaveTextContent("2h 15m ago");
    expect(evidenceSection).toHaveTextContent("15m target");
    expect(evidenceSection).toHaveTextContent("Incomplete EIV basis");
    expect(evidenceSection).toHaveTextContent("Missing: Tritanium");
  });

  test("Not Included in Estimate renders the evaluation's excludedCosts", async () => {
    opportunitiesApi.evaluateOpportunities.mockResolvedValue(
      evaluation({ excludedCosts: [{ code: "brokerFees", message: "Broker fees are not modeled." }] }),
    );
    const user = userEvent.setup();
    render(
      <MemoryRouter>
        <OpportunitiesPage />
      </MemoryRouter>,
    );

    await screen.findByRole("cell", { name: /Rifter/ });
    await user.click(screen.getByRole("row", { name: /Rifter/ }));

    const excludedSection = screen.getByRole("region", { name: "Not Included in Estimate" });
    expect(excludedSection).toHaveTextContent("Broker fees are not modeled.");
  });
});

function reactionScope(overrides: Partial<ProfitabilityScopeDefinition> = {}): ProfitabilityScopeDefinition {
  return {
    id: "reactions",
    label: "Reactions",
    family: "Industry",
    description: "Published reaction formulas with all immediate inputs purchased from sell orders.",
    limitations: [],
    recipeKind: "reaction",
    ...overrides,
  };
}

function reactionCandidate(overrides: Partial<OpportunityCandidate> = {}): OpportunityCandidate {
  return {
    ...candidate(),
    scopeId: "reactions",
    productTypeId: 16_656,
    productName: "Fernite Alloy",
    recipe: {
      kind: "reaction",
      reactionFormulaTypeId: 46_171,
      reactionFormulaName: "Fernite Alloy Reaction Formula",
    },
    materialEfficiency: null,
    timeEfficiency: null,
    ...overrides,
  };
}

describe("OpportunitiesPage reactions", () => {
  beforeEach(() => {
    opportunitiesApi.listOpportunityScopes.mockResolvedValue([scope(), reactionScope()]);
    industryApi.listFacilities.mockResolvedValue([facility(), facility({ id: "facility-reaction", name: "0-VG7A - Reactions", role: "reaction" })]);
  });

  test("the scope selector groups scopes into optgroups by family", async () => {
    render(
      <MemoryRouter>
        <OpportunitiesPage />
      </MemoryRouter>,
    );

    const select = await screen.findByLabelText("Category");
    const groups = within(select).getAllByRole("group") as HTMLOptGroupElement[];
    expect(groups.map((group) => group.label)).toEqual(["Ships", "Industry"]);
    expect(within(groups[0]).getByRole("option", { name: "T1 Frigates" })).toBeInTheDocument();
    expect(within(groups[1]).getByRole("option", { name: "Reactions" })).toBeInTheDocument();
  });

  test("switching to a reaction scope evaluates against the reaction-role facility with no ME/TE", async () => {
    // First evaluate (page load, default t1-frigates scope) returns Rifter;
    // the second (after switching scopes) returns the reaction candidate.
    opportunitiesApi.evaluateOpportunities.mockResolvedValueOnce(evaluation()).mockResolvedValueOnce(
      evaluation({
        context: {
          scopeId: "reactions",
          facilityProfileId: "facility-reaction",
          facilityRevision: 1,
          marketRegionId: 10_000_002,
          marketLocationId: 60_003_760,
          materialEfficiency: null,
          timeEfficiency: null,
          runs: 1,
          materialPricingPolicy: "acquireQuantityFromSellOrders",
          outputPricingPolicy: "lowestSell",
          inventoryReuseEnabled: false,
          recursiveComponentExpansionEnabled: false,
        },
        candidates: [reactionCandidate()],
      }),
    );
    const user = userEvent.setup();
    render(
      <MemoryRouter>
        <OpportunitiesPage />
      </MemoryRouter>,
    );

    await screen.findByRole("cell", { name: /Rifter/ });
    await user.selectOptions(screen.getByLabelText("Category"), "reactions");

    await waitFor(() => {
      expect(opportunitiesApi.evaluateOpportunities).toHaveBeenLastCalledWith(
        expect.objectContaining({
          scopeId: "reactions",
          facilityProfileId: "facility-reaction",
          materialEfficiency: null,
          timeEfficiency: null,
        }),
        expect.anything(),
      );
    });

    expect(await screen.findByRole("cell", { name: /Fernite Alloy/ })).toBeInTheDocument();
    // The context strip shows the reaction facility, not the manufacturing one.
    expect(screen.getByText(/0-VG7A - Reactions/)).toBeInTheDocument();
  });

  test("no manufacturing facility configured for the active scope shows a reaction-specific empty state", async () => {
    industryApi.listFacilities.mockResolvedValue([facility()]); // manufacturing only, no reaction facility
    opportunitiesApi.listOpportunityScopes.mockResolvedValue([reactionScope()]);
    render(
      <MemoryRouter>
        <OpportunitiesPage />
      </MemoryRouter>,
    );

    expect(await screen.findByText("No reaction facility configured")).toBeInTheDocument();
    expect(opportunitiesApi.evaluateOpportunities).not.toHaveBeenCalled();
  });

  test("the inspector omits the ME/TE line for a reaction candidate", async () => {
    opportunitiesApi.listOpportunityScopes.mockResolvedValue([reactionScope()]);
    opportunitiesApi.evaluateOpportunities.mockResolvedValue(evaluation({ candidates: [reactionCandidate()] }));
    const user = userEvent.setup();
    render(
      <MemoryRouter>
        <OpportunitiesPage />
      </MemoryRouter>,
    );

    await screen.findByRole("cell", { name: /Fernite Alloy/ });
    await user.click(screen.getByRole("row", { name: /Fernite Alloy/ }));

    const overview = await screen.findByRole("region", { name: "Overview" });
    expect(overview).not.toHaveTextContent("ME");
    expect(overview).not.toHaveTextContent("TE");
  });

  test("Create Build for a reaction candidate sends a reaction recipe selection", async () => {
    opportunitiesApi.listOpportunityScopes.mockResolvedValue([reactionScope()]);
    opportunitiesApi.evaluateOpportunities.mockResolvedValue(evaluation({ candidates: [reactionCandidate()] }));
    industryApi.createBuild.mockResolvedValue(build({ id: "build-reaction" }));
    const user = userEvent.setup();
    render(
      <MemoryRouter initialEntries={["/opportunities"]}>
        <Routes>
          <Route element={<OpportunitiesPage />} path="/opportunities" />
          <Route element={<div>Build workspace for build-reaction</div>} path="/builds/:buildId" />
        </Routes>
      </MemoryRouter>,
    );

    await screen.findByRole("cell", { name: /Fernite Alloy/ });
    await user.click(screen.getByRole("row", { name: /Fernite Alloy/ }));
    const heading = await screen.findByRole("heading", { name: "Fernite Alloy" });
    const inspector = heading.closest("aside") as HTMLElement;

    await user.click(within(inspector).getByRole("button", { name: "Create Build" }));

    expect(industryApi.createBuild).toHaveBeenCalledWith(
      expect.objectContaining({ recipe: { mode: "reaction", reactionFormulaTypeId: 46_171 } }),
    );
    expect(await screen.findByText("Build workspace for build-reaction")).toBeInTheDocument();
  });

  test("switching manufacturing -> reaction -> manufacturing round-trips the facility and ME/TE correctly", async () => {
    opportunitiesApi.listOpportunityScopes.mockResolvedValue([scope(), reactionScope()]);
    opportunitiesApi.evaluateOpportunities.mockResolvedValue(evaluation());
    const user = userEvent.setup();
    render(
      <MemoryRouter>
        <OpportunitiesPage />
      </MemoryRouter>,
    );

    await screen.findByRole("cell", { name: /Rifter/ });
    await waitFor(() => {
      expect(opportunitiesApi.evaluateOpportunities).toHaveBeenLastCalledWith(
        expect.objectContaining({ scopeId: "t1-frigates", facilityProfileId: "facility-1", materialEfficiency: 10 }),
        expect.anything(),
      );
    });

    opportunitiesApi.evaluateOpportunities.mockResolvedValue(evaluation({ candidates: [reactionCandidate()] }));
    await user.selectOptions(screen.getByLabelText("Category"), "reactions");
    await waitFor(() => {
      expect(opportunitiesApi.evaluateOpportunities).toHaveBeenLastCalledWith(
        expect.objectContaining({ scopeId: "reactions", facilityProfileId: "facility-reaction", materialEfficiency: null }),
        expect.anything(),
      );
    });

    opportunitiesApi.evaluateOpportunities.mockResolvedValue(evaluation());
    await user.selectOptions(screen.getByLabelText("Category"), "t1-frigates");
    await waitFor(() => {
      expect(opportunitiesApi.evaluateOpportunities).toHaveBeenLastCalledWith(
        expect.objectContaining({ scopeId: "t1-frigates", facilityProfileId: "facility-1", materialEfficiency: 10 }),
        expect.anything(),
      );
    });
  });
});
