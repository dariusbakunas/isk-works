import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, test, vi } from "vitest";

import type {
  MarketHubSummary,
  MarketLocation,
  MarketLocationSearchResult,
  MarketRegion,
  MarketScope,
  MarketStructureSummary,
} from "../../api/industry";
import { MarketScopeSelector } from "../market-scope-selector";

const api = vi.hoisted(() => ({
  listMarketRegions: vi.fn(),
  listMarketRegionLocations: vi.fn(),
  listMarketHubs: vi.fn(),
  listMarketStructures: vi.fn(),
  searchMarketLocations: vi.fn(),
}));

vi.mock("../../api/industry", async () => {
  const actual = await vi.importActual<typeof import("../../api/industry")>("../../api/industry");
  return {
    ...actual,
    listMarketRegions: api.listMarketRegions,
    listMarketRegionLocations: api.listMarketRegionLocations,
    listMarketHubs: api.listMarketHubs,
    listMarketStructures: api.listMarketStructures,
    searchMarketLocations: api.searchMarketLocations,
  };
});

const noFreshness = { trackedTypeCount: 0, observedTypeCount: 0, mostRecentObservedAt: null };

const regions: MarketRegion[] = [
  { regionId: 10_000_002, regionName: "The Forge" },
  { regionId: 10_000_043, regionName: "Domain" },
];

const forgeLocations: MarketLocation[] = [
  {
    locationId: 60_003_760,
    locationName: "Jita IV - Moon 4 - Caldari Navy Assembly Plant",
    kind: "npcStation",
    solarSystemId: 30_000_142,
    solarSystemName: "Jita",
    stationTypeId: 1_529,
    stationTypeName: "Caldari Navy Assembly Plant",
    securityClass: "highSec",
    structureTypeId: null,
    freshness: noFreshness,
  },
];

const domainLocations: MarketLocation[] = [
  {
    locationId: 60_011_866,
    locationName: "Amarr VIII (Oris) - Emperor Family Academy",
    kind: "npcStation",
    solarSystemId: 30_002_187,
    solarSystemName: "Amarr",
    stationTypeId: 1_931,
    stationTypeName: "Amarr Trade Hub",
    securityClass: "highSec",
    structureTypeId: null,
    freshness: noFreshness,
  },
];

const hubs: MarketHubSummary[] = [
  {
    locationId: 60_003_760,
    locationName: "Jita IV - Moon 4 - Caldari Navy Assembly Plant",
    shortName: "Jita 4-4",
    solarSystemId: 30_000_142,
    solarSystemName: "Jita",
    regionId: 10_000_002,
    regionName: "The Forge",
    freshness: { trackedTypeCount: 10, observedTypeCount: 10, mostRecentObservedAt: "2026-08-25T12:00:00Z" },
  },
];

const structures: MarketStructureSummary[] = [
  {
    locationId: 1_049_995_520_085,
    locationName: "C-J6MT - Weaselior University T2 Lab",
    structureTypeId: 35_827,
    structureTypeName: "Astrahus",
    solarSystemId: 30_000_772,
    solarSystemName: "C-J6MT",
    regionId: 10_000_058,
    regionName: "Feythabolis",
    securityClass: "wormhole",
    accessState: "confirmed",
    accessCharacterName: "Kira Vayne",
    accessCheckedAt: "2026-08-25T12:00:00Z",
    freshness: noFreshness,
  },
];

function renderSelector(
  onChange = vi.fn(),
  scope: MarketScope = { regionId: 10_000_002, locationId: 60_003_760 },
) {
  return { onChange, ...render(<MarketScopeSelector onChange={onChange} scope={scope} />) };
}

async function openSelector() {
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: /The Forge/ }));
  return { user, dialog: screen.getByRole("dialog", { name: "Select market scope" }) };
}

describe("MarketScopeSelector", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    window.localStorage.clear();
    api.listMarketRegions.mockResolvedValue(regions);
    api.listMarketRegionLocations.mockImplementation((regionId: number) =>
      Promise.resolve(regionId === 10_000_002 ? forgeLocations : domainLocations),
    );
    api.listMarketHubs.mockResolvedValue(hubs);
    api.listMarketStructures.mockResolvedValue(structures);
    api.searchMarketLocations.mockResolvedValue([]);
  });

  test("the trigger resolves and shows the current scope's region and location names", async () => {
    renderSelector();

    const trigger = await screen.findByRole("button", {
      name: /The Forge.*Jita IV - Moon 4 - Caldari Navy Assembly Plant/,
    });
    expect(trigger).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  test("a region-wide scope (no location) shows 'All locations'", async () => {
    renderSelector(vi.fn(), { regionId: 10_000_002, locationId: undefined });

    expect(await screen.findByRole("button", { name: /The Forge.*All locations/ })).toBeInTheDocument();
  });

  test("opens on the Major Hubs tab by default and selecting a hub commits immediately, no Apply step", async () => {
    const { onChange } = renderSelector();
    const { dialog } = await openSelector();

    expect(within(dialog).getByRole("tab", { name: "Major Hubs", selected: true })).toBeInTheDocument();
    await screen.findByText("Jita 4-4");

    const user = userEvent.setup();
    await user.click(within(dialog).getByText("Jita 4-4"));

    expect(onChange).toHaveBeenCalledWith({ regionId: 10_000_002, locationId: 60_003_760 });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  test("My Structures tab lazy-loads on first visit and selecting a structure commits immediately", async () => {
    const { onChange } = renderSelector();
    const { user, dialog } = await openSelector();

    expect(api.listMarketStructures).not.toHaveBeenCalled();
    await user.click(within(dialog).getByRole("tab", { name: "My Structures" }));

    await screen.findByText("C-J6MT - Weaselior University T2 Lab");
    expect(screen.getByText(/via Kira Vayne/)).toBeInTheDocument();

    await user.click(within(dialog).getByText("C-J6MT - Weaselior University T2 Lab"));

    expect(onChange).toHaveBeenCalledWith({ regionId: 10_000_058, locationId: 1_049_995_520_085 });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  test("an expired structure's access state is surfaced instead of its freshness", async () => {
    api.listMarketStructures.mockResolvedValue([
      { ...structures[0], accessState: "expired", accessCharacterName: "Drake Orin" },
    ]);
    renderSelector();
    const { user, dialog } = await openSelector();
    await user.click(within(dialog).getByRole("tab", { name: "My Structures" }));

    expect(await screen.findByText("Access expired")).toBeInTheDocument();
  });

  test("switching the draft region in the Regions tab loads that region's own locations", async () => {
    renderSelector();
    const { user, dialog } = await openSelector();

    await user.click(within(dialog).getByRole("tab", { name: "Regions" }));
    expect(within(dialog).getByText("Jita IV - Moon 4 - Caldari Navy Assembly Plant")).toBeInTheDocument();

    await user.click(within(dialog).getByRole("button", { name: "Domain" }));
    expect(await within(dialog).findByText("Amarr VIII (Oris) - Emperor Family Academy")).toBeInTheDocument();
    expect(within(dialog).queryByText("Jita IV - Moon 4 - Caldari Navy Assembly Plant")).not.toBeInTheDocument();
  });

  test("Regions tab: Apply scope commits the draft; Cancel discards it", async () => {
    const { onChange } = renderSelector();
    const { user, dialog } = await openSelector();

    await user.click(within(dialog).getByRole("tab", { name: "Regions" }));
    await user.click(within(dialog).getByRole("button", { name: "Domain" }));
    await user.click(await within(dialog).findByRole("button", { name: "Amarr VIII (Oris) - Emperor Family Academy" }));
    await user.click(within(dialog).getByRole("button", { name: "Apply scope" }));

    expect(onChange).toHaveBeenCalledWith({ regionId: 10_000_043, locationId: 60_011_866 });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  test("Regions tab: Cancel closes without calling onChange", async () => {
    const { onChange } = renderSelector();
    const { user, dialog } = await openSelector();

    await user.click(within(dialog).getByRole("tab", { name: "Regions" }));
    await user.click(within(dialog).getByRole("button", { name: "Domain" }));
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));

    expect(onChange).not.toHaveBeenCalled();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  test("Regions tab: picking 'All locations' clears the draft location", async () => {
    const { onChange } = renderSelector();
    const { user, dialog } = await openSelector();

    await user.click(within(dialog).getByRole("tab", { name: "Regions" }));
    await user.click(within(dialog).getByRole("button", { name: "All locations" }));
    await user.click(within(dialog).getByRole("button", { name: "Apply scope" }));

    expect(onChange).toHaveBeenCalledWith({ regionId: 10_000_002, locationId: undefined });
  });

  test("typing a query shows merged search results instead of Recent/tabs, and a result commits immediately", async () => {
    const searchResults: MarketLocationSearchResult[] = [
      {
        kind: "hub",
        locationId: 60_003_760,
        displayName: "Jita 4-4",
        solarSystemId: 30_000_142,
        solarSystemName: "Jita",
        regionId: 10_000_002,
        regionName: "The Forge",
        freshness: { trackedTypeCount: 5, observedTypeCount: 5, mostRecentObservedAt: "2026-08-25T12:00:00Z" },
      },
    ];
    api.searchMarketLocations.mockResolvedValue(searchResults);
    const { onChange } = renderSelector();
    const { user, dialog } = await openSelector();

    await user.type(within(dialog).getByRole("textbox", { name: /Search regions/ }), "Jita");

    await waitFor(() => expect(api.searchMarketLocations).toHaveBeenCalledWith("Jita"));
    expect(within(dialog).queryByRole("tab", { name: "Major Hubs" })).not.toBeInTheDocument();
    const result = await within(dialog).findByText("Jita 4-4");
    await user.click(result);

    expect(onChange).toHaveBeenCalledWith({ regionId: 10_000_002, locationId: 60_003_760 });
  });

  test("a selected hub appears in Recent the next time the same selector is opened", async () => {
    renderSelector();
    const first = await openSelector();
    await first.user.click(await screen.findByText("Jita 4-4"));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();

    const { dialog } = await openSelector();

    expect(within(dialog).getByText("Recent")).toBeInTheDocument();
    expect(within(dialog).getAllByText("Jita 4-4").length).toBeGreaterThan(0);
  });
});
