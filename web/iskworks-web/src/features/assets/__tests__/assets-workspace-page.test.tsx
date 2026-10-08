import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { beforeEach, expect, test, vi } from "vitest";

import {
  exportAssets,
  queryAssets,
  syncAssets,
} from "../../../api/assets";
import { AssetsWorkspacePage } from "../assets-workspace-page";
import type { LoadedAssetPage } from "../use-assets-query";

vi.mock("../../../api/assets", async (importOriginal) => ({
  ...await importOriginal<typeof import("../../../api/assets")>(),
  exportAssets: vi.fn(),
  queryAssets: vi.fn(),
  syncAssets: vi.fn(),
}));

const page: LoadedAssetPage = {
  rows: [{
    eveItemId: 9001,
    typeId: 34,
    typeName: "Tritanium",
    quantity: 27_075,
    packagedVolume: "0.01",
    totalPackagedVolume: "270.75",
    ownerId: "owner",
    ownerName: "Valka",
    connectionId: "connection",
    characterId: 2112345678,
    characterName: "Valka",
    locationId: 60003760,
    locationName: "Jita IV - Moon 4 - Caldari Navy Assembly Plant",
    locationFlag: "Hangar",
    containerItemId: 8001,
    containerName: "Minerals",
    groupId: 18,
    groupName: "Mineral",
    assetKind: "material",
    observedAt: "2026-08-06T12:00:00Z",
    blueprint: null,
    reconciliation: {
      state: "difference",
      observedOwnerTypeQuantity: 27_075,
      accountedOwnerTypeQuantity: 20_000,
      scope: "ownerType",
    },
    isContainer: false,
  }],
  total: 1,
  nextCursor: null,
  summary: {
    locationCount: 1,
    characterCount: 1,
    stackCount: 1,
    totalQuantity: 27_075,
    totalPackagedVolume: "270.7500",
    latestObservedAt: "2026-08-06T12:00:00Z",
    unresolvedLocationCount: 0,
    syncStates: [],
  },
  facets: {
    characters: [{ value: "connection", label: "Valka", count: 1 }],
    locations: [{ value: "60003760", label: "Jita IV - Moon 4", count: 1 }],
    assetKinds: [],
    groups: [{ value: "18", label: "Mineral", count: 1 }],
    blueprintKinds: [],
    reconciliationStates: [],
  },
};

beforeEach(() => {
  vi.mocked(queryAssets).mockReset().mockResolvedValue(page);
  vi.mocked(exportAssets).mockReset();
  vi.mocked(syncAssets).mockReset().mockResolvedValue([]);
});

test("renders a dense flat asset table and opens contextual details", async () => {
  render(<MemoryRouter><AssetsWorkspacePage /></MemoryRouter>);

  expect(await screen.findByRole("table", { name: "Synchronized assets" })).toBeInTheDocument();
  expect(screen.getAllByText("270.75 m³")).toHaveLength(2);
  expect(screen.getByText("27,075")).toBeInTheDocument();
  expect(screen.queryByText("Unresolved container")).not.toBeInTheDocument();

  fireEvent.click(screen.getByText("Tritanium"));
  const inspector = screen.getByRole("complementary", { name: "Selected asset" });
  expect(inspector).toBeInTheDocument();
  expect(inspector).toHaveTextContent("Minerals");
  expect(screen.getByRole("link", { name: "View in Inventory" })).toBeInTheDocument();

  fireEvent.click(screen.getByRole("button", { name: "Close asset details" }));
  expect(screen.queryByRole("complementary", { name: "Selected asset" })).not.toBeInTheDocument();
});

test("debounces search before querying the server", async () => {
  render(<MemoryRouter><AssetsWorkspacePage /></MemoryRouter>);
  await screen.findByText("Tritanium");
  vi.mocked(queryAssets).mockClear();

  fireEvent.change(screen.getByRole("textbox", { name: "Search assets" }), {
    target: { value: "trit" },
  });

  expect(queryAssets).not.toHaveBeenCalled();
  await waitFor(() => expect(queryAssets).toHaveBeenCalledWith(
    expect.objectContaining({ search: "trit" }),
  ));
});

test("returns the asset table to the top when filters change", async () => {
  vi.mocked(queryAssets).mockResolvedValue({
    ...page,
    facets: {
      ...page.facets,
      locations: [
        ...page.facets.locations,
        { value: "60008494", label: "Amarr VIII - Emperor Family Academy", count: 1 },
      ],
    },
  });
  render(<MemoryRouter><AssetsWorkspacePage /></MemoryRouter>);

  await screen.findByText("Tritanium");
  const viewport = document.querySelector<HTMLElement>("[data-asset-scroll-viewport]");
  const workspace = document.querySelector<HTMLElement>("[data-assets-workspace]");
  expect(viewport).not.toBeNull();
  expect(workspace).not.toBeNull();
  viewport!.scrollTop = 480;
  workspace!.scrollTop = 320;

  fireEvent.click(screen.getByRole("checkbox", { name: /Jita IV - Moon 4/i }));

  await waitFor(() => expect(queryAssets).toHaveBeenLastCalledWith(
    expect.objectContaining({ locationIds: [60008494] }),
  ));
  expect(viewport!.scrollTop).toBe(0);
  expect(workspace!.scrollTop).toBe(0);
});

test("supports selecting none and all characters", async () => {
  render(<MemoryRouter><AssetsWorkspacePage /></MemoryRouter>);
  await screen.findByText("Tritanium");
  vi.mocked(queryAssets).mockClear();

  fireEvent.click(screen.getByRole("button", { name: "Select no characters" }));
  expect(screen.getByText("No synchronized assets match these filters.")).toBeInTheDocument();
  expect(queryAssets).not.toHaveBeenCalled();

  fireEvent.click(screen.getByRole("checkbox", { name: /Valka/i }));
  await waitFor(() => expect(queryAssets).toHaveBeenCalledWith(
    expect.objectContaining({ connectionIds: ["connection"] }),
  ));

  fireEvent.click(screen.getByRole("button", { name: "Select all characters" }));
  await waitFor(() => expect(queryAssets).toHaveBeenLastCalledWith(
    expect.objectContaining({ connectionIds: [] }),
  ));
});

test("supports selecting none and all locations", async () => {
  render(<MemoryRouter><AssetsWorkspacePage /></MemoryRouter>);
  await screen.findByText("Tritanium");
  vi.mocked(queryAssets).mockClear();

  fireEvent.click(screen.getByRole("button", { name: "Select no locations" }));
  expect(screen.getByText("No synchronized assets match these filters.")).toBeInTheDocument();
  expect(queryAssets).not.toHaveBeenCalled();

  fireEvent.click(screen.getByRole("checkbox", { name: /Jita IV - Moon 4/i }));
  await waitFor(() => expect(queryAssets).toHaveBeenCalledWith(
    expect.objectContaining({ locationIds: [60003760] }),
  ));

  fireEvent.click(screen.getByRole("button", { name: "Select all locations" }));
  await waitFor(() => expect(queryAssets).toHaveBeenLastCalledWith(
    expect.objectContaining({ locationIds: [] }),
  ));
});

test("supports selecting none and all asset kinds", async () => {
  render(<MemoryRouter><AssetsWorkspacePage /></MemoryRouter>);
  await screen.findByText("Tritanium");
  vi.mocked(queryAssets).mockClear();

  fireEvent.click(screen.getByRole("button", { name: "Select no asset kinds" }));
  expect(screen.getByText("No synchronized assets match these filters.")).toBeInTheDocument();
  expect(queryAssets).not.toHaveBeenCalled();

  fireEvent.click(screen.getByRole("checkbox", { name: "Material" }));
  await waitFor(() => expect(queryAssets).toHaveBeenCalledWith(
    expect.objectContaining({ assetKinds: ["material"] }),
  ));

  fireEvent.click(screen.getByRole("button", { name: "Select all asset kinds" }));
  await waitFor(() => expect(queryAssets).toHaveBeenLastCalledWith(
    expect.objectContaining({ assetKinds: [] }),
  ));
});

test("supports selecting none and all item groups", async () => {
  render(<MemoryRouter><AssetsWorkspacePage /></MemoryRouter>);
  await screen.findByText("Tritanium");
  vi.mocked(queryAssets).mockClear();
  fireEvent.click(screen.getByRole("button", { name: "Item group filters" }));

  fireEvent.click(screen.getByRole("button", { name: "Select no item groups" }));
  expect(screen.getByText("No synchronized assets match these filters.")).toBeInTheDocument();
  expect(queryAssets).not.toHaveBeenCalled();

  fireEvent.click(screen.getByRole("checkbox", { name: "Mineral" }));
  await waitFor(() => expect(queryAssets).toHaveBeenCalledWith(
    expect.objectContaining({ groupIds: [18] }),
  ));

  fireEvent.click(screen.getByRole("button", { name: "Select all item groups" }));
  await waitFor(() => expect(queryAssets).toHaveBeenLastCalledWith(
    expect.objectContaining({ groupIds: [] }),
  ));
});
