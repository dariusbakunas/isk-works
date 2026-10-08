import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test, vi } from "vitest";

import type { FacilityProfile, MarketScope, PriceSource } from "../../../../../api/industry";
import { BuildSettingsSummary } from "../build-settings-summary";

vi.mock("../../../../../hooks/use-market-scope-label", () => ({
  useMarketScopeLabel: (scope: MarketScope) => ({
    regionName: scope.regionId === 10_000_002 ? "The Forge" : "Domain",
    locationName: scope.locationId === undefined ? "All locations" : "Jita IV - Moon 4 - Caldari Navy Assembly Plant",
  }),
}));

const manufacturingFacility: FacilityProfile = {
  id: "facility-1",
  workspaceId: "workspace-1",
  name: "GEZ Sotiyo",
  kind: "manual",
  role: "manufacturing",
  structureId: 1050487654321,
  structureTypeId: 35827,
  structureTypeName: "Sotiyo",
  solarSystemId: 30000505,
  solarSystemName: "C-J6MT",
  securityClass: "nullSec",
  materialReductionPercent: "1.0",
  timeReductionPercent: "30.0",
  jobCostReductionPercent: "5.0",
  facilityTaxPercent: "1.0",
  sccSurchargePercent: "4.0",
  allianceSurchargePercent: "0",
  fixedSupplementalCost: "0.0000",
  manualSystemCostIndex: "0.0979",
  notes: "",
  rigs: [],
  archivedAt: null,
  revision: 1,
  createdAt: "2026-07-27T10:00:00Z",
  updatedAt: "2026-07-27T10:00:00Z",
};

const reactionFacility: FacilityProfile = {
  ...manufacturingFacility,
  id: "facility-2",
  name: "Athanor",
  role: "reaction",
};

const priceSource: PriceSource = {
  id: "src-1",
  workspaceId: "workspace-1",
  name: "Conservative fallback",
  description: "",
  kind: "manual",
  revision: 1,
  itemCount: 0,
  recentBuildCount: 0,
  items: [],
  createdAt: "2026-07-27T10:00:00Z",
  updatedAt: "2026-07-27T10:00:00Z",
};

const jita: MarketScope = { regionId: 10_000_002, locationId: 60_003_760 };

function renderSummary(overrides: Partial<Parameters<typeof BuildSettingsSummary>[0]> = {}) {
  const onEdit = vi.fn();
  render(
    <BuildSettingsSummary
      manufacturingFacilities={[manufacturingFacility]}
      manufacturingFacilityId=""
      reactionFacilities={[reactionFacility]}
      reactionFacilityId=""
      rootFacilityRole="manufacturing"
      materialScope={jita}
      materialPolicy="highestBuy"
      outputScope={jita}
      outputPolicy="lowestSell"
      sourceId=""
      sources={[priceSource]}
      updating={false}
      onEdit={onEdit}
      {...overrides}
    />,
  );
  return { onEdit };
}

describe("BuildSettingsSummary", () => {
  test("shows a concise value for every group", () => {
    renderSummary({
      manufacturingFacilityId: manufacturingFacility.id,
      materialPolicy: "acquireQuantityFromSellOrders",
      outputPolicy: "liquidateQuantityIntoBuyOrders",
      sourceId: priceSource.id,
    });

    expect(within(screen.getByRole("group", { name: "Manufacturing" })).getByText("GEZ Sotiyo")).toBeInTheDocument();
    expect(within(screen.getByRole("group", { name: "Reactions" })).getByText("Not selected")).toBeInTheDocument();
    expect(within(screen.getByRole("group", { name: "Materials" }))
      .getByText("The Forge · Jita IV - Moon 4 - Caldari Navy Assembly Plant · Buy immediately")).toBeInTheDocument();
    expect(within(screen.getByRole("group", { name: "Output" }))
      .getByText("The Forge · Jita IV - Moon 4 - Caldari Navy Assembly Plant · Sell immediately")).toBeInTheDocument();
    expect(within(screen.getByRole("group", { name: "Overrides" })).getByText("Conservative fallback")).toBeInTheDocument();
  });

  test("shows None for the Overrides cell when no fallback price list is set", () => {
    renderSummary();

    expect(within(screen.getByRole("group", { name: "Overrides" })).getByText("None")).toBeInTheDocument();
  });

  test("uses short pricing-policy labels, not the dialog's fuller option text", () => {
    renderSummary({ materialPolicy: "highestBuy", outputPolicy: "lowestSell" });

    expect(screen.getByText(/Highest buy$/)).toBeInTheDocument();
    expect(screen.getByText(/Lowest sell$/)).toBeInTheDocument();
    expect(screen.queryByText(/Use highest buy order/)).not.toBeInTheDocument();
  });

  test("warns when the root facility (manufacturing) is unselected", () => {
    renderSummary({ rootFacilityRole: "manufacturing", manufacturingFacilityId: "" });

    const manufacturing = within(screen.getByRole("group", { name: "Manufacturing" }));
    expect(manufacturing.getByText("Not selected")).toHaveClass("text-warning");
  });

  test("does not warn about an unselected non-root facility", () => {
    renderSummary({ rootFacilityRole: "manufacturing", manufacturingFacilityId: manufacturingFacility.id, reactionFacilityId: "" });

    const reactions = within(screen.getByRole("group", { name: "Reactions" }));
    expect(reactions.getByText("Not selected")).not.toHaveClass("text-warning");
  });

  test("warns on the reaction facility instead when it is the root role", () => {
    renderSummary({ rootFacilityRole: "reaction", manufacturingFacilityId: "", reactionFacilityId: "" });

    expect(within(screen.getByRole("group", { name: "Manufacturing" })).getByText("Not selected")).not.toHaveClass("text-warning");
    expect(within(screen.getByRole("group", { name: "Reactions" })).getByText("Not selected")).toHaveClass("text-warning");
  });

  test("the warning is visible without opening the settings dialog", () => {
    renderSummary({ rootFacilityRole: "manufacturing", manufacturingFacilityId: "" });

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(within(screen.getByRole("group", { name: "Manufacturing" })).getByText("Not selected")).toHaveClass("text-warning");
  });

  test("clicking Edit build settings invokes onEdit", async () => {
    const user = userEvent.setup();
    const { onEdit } = renderSummary();

    await user.click(screen.getByRole("button", { name: "Edit build settings" }));

    expect(onEdit).toHaveBeenCalledTimes(1);
  });

  test("renders the caller-supplied leading Blueprint cell ahead of the settings chips", () => {
    renderSummary({ leading: <div data-testid="leading-cell">Blueprint cell</div> });

    expect(screen.getByTestId("leading-cell")).toBeInTheDocument();
  });

  test("truncated values expose their full text via the title attribute", () => {
    renderSummary({ manufacturingFacilityId: manufacturingFacility.id });

    expect(within(screen.getByRole("group", { name: "Manufacturing" })).getByText("GEZ Sotiyo")).toHaveAttribute("title", "GEZ Sotiyo");
  });

  test("lays out cells in a wrapping flex row, not a fixed equal-width grid", () => {
    renderSummary();

    const row = screen.getByRole("region", { name: "Planning assumptions" }).firstElementChild;
    expect(row).toHaveClass("flex", "flex-wrap");
    expect(row).not.toHaveClass("grid");
  });
});
