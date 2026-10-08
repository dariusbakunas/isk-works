import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { MarketImportsPage } from "../features/industry/market/market-imports-page";

const api = vi.hoisted(() => ({
  previewMarketExports: vi.fn(),
  resolveMarketLocations: vi.fn(),
  importMarketExports: vi.fn(),
  listMarketImports: vi.fn(),
}));

vi.mock("../api/industry", async (original) => ({
  ...(await original()),
  ...api,
}));

const preview = {
  files: [{
    filename: "Insmother-Tritanium-2026.07.26 192639.txt",
    fileChecksum: "file",
    normalizedChecksum: "normalized",
    fileSizeBytes: 822,
    typeId: 34,
    typeName: "Tritanium",
    locationId: 1049588174021,
    locationName: "Structure 1049588174021",
    solarSystemId: 30000772,
    solarSystemName: null,
    regionId: 10000009,
    regionName: null,
    observedAt: "2026-07-26T19:26:39Z",
    timestampSource: "filename",
    rowCount: 6,
    buyOrderCount: 2,
    sellOrderCount: 4,
    lowestSell: "3.9700",
    highestBuy: "3.8100",
    totalBuyVolume: 213447515,
    totalSellVolume: 1426795800,
    duplicateOrderCount: 0,
    alreadyImported: false,
    canImport: true,
    warnings: [],
    errors: [],
  }],
  totalFiles: 1,
  validFiles: 1,
  invalidFiles: 0,
  duplicateFiles: 0,
  itemCount: 1,
  locationCount: 1,
  totalRows: 6,
  oldestObservationAt: "2026-07-26T19:26:39Z",
  newestObservationAt: "2026-07-26T19:26:39Z",
};

describe("market imports", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    api.listMarketImports.mockResolvedValue([]);
    api.previewMarketExports.mockResolvedValue(preview);
    api.resolveMarketLocations.mockImplementation(async (locationIds: number[]) => ({
      configured: true,
      resolved: [],
      unresolvedLocationIds: locationIds,
      eligibleCharacterCount: 1,
      needsReconnection: false,
      warnings: [],
    }));
  });

  it("previews selected client exports with exact order summaries", async () => {
    const user = userEvent.setup();
    const { container } = render(
      <MemoryRouter>
        <MarketImportsPage />
      </MemoryRouter>,
    );
    const input = container.querySelector('input[type="file"]') as HTMLInputElement;
    const file = new File(["fixture"], preview.files[0].filename, { type: "text/plain" });
    await user.upload(input, file);
    expect(await screen.findByRole("dialog", { name: "Preview Market Exports" })).toBeInTheDocument();
    expect(await screen.findByText(/Tritanium ·/)).toBeInTheDocument();
    expect(screen.getByLabelText("3.9700 ISK")).toBeInTheDocument();
    expect(screen.getByLabelText("3.8100 ISK")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Import Market Observations/ })).toBeEnabled();
  });

  it("replaces numeric structure IDs with authenticated ESI names", async () => {
    const user = userEvent.setup();
    api.resolveMarketLocations.mockResolvedValue({
      configured: true,
      resolved: [{
        locationId: 1049588174021,
        locationName: "C-J6MT - GEZ - Industry",
      }],
      unresolvedLocationIds: [],
      eligibleCharacterCount: 1,
      needsReconnection: false,
      warnings: [],
    });
    const { container } = render(
      <MemoryRouter>
        <MarketImportsPage />
      </MemoryRouter>,
    );
    const input = container.querySelector('input[type="file"]') as HTMLInputElement;
    await user.upload(input, new File(["fixture"], preview.files[0].filename, { type: "text/plain" }));

    expect(await screen.findByText(/Tritanium · C-J6MT - GEZ - Industry/)).toBeInTheDocument();
    expect(api.resolveMarketLocations).toHaveBeenCalledWith([1049588174021]);
    expect(screen.queryByRole("button", { name: "Retry location names" })).not.toBeInTheDocument();
  });

  it("directs users to reconnect when structure scope is missing", async () => {
    const user = userEvent.setup();
    api.resolveMarketLocations.mockResolvedValue({
      configured: true,
      resolved: [],
      unresolvedLocationIds: [1049588174021],
      eligibleCharacterCount: 0,
      needsReconnection: true,
      warnings: [],
    });
    const { container } = render(
      <MemoryRouter>
        <MarketImportsPage />
      </MemoryRouter>,
    );
    const input = container.querySelector('input[type="file"]') as HTMLInputElement;
    await user.upload(input, new File(["fixture"], preview.files[0].filename, { type: "text/plain" }));

    expect(await screen.findByText("Structure permission required")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Open Characters" })).toHaveAttribute("href", "/characters");
    expect(screen.getByRole("button", { name: "Retry location names" })).toBeEnabled();
  });

  it("cancels the modal without importing or discarding selected files", async () => {
    const user = userEvent.setup();
    const { container } = render(
      <MemoryRouter>
        <MarketImportsPage />
      </MemoryRouter>,
    );
    const input = container.querySelector('input[type="file"]') as HTMLInputElement;
    await user.upload(input, new File(["fixture"], preview.files[0].filename, { type: "text/plain" }));
    await screen.findByRole("dialog", { name: "Preview Market Exports" });
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Preview Market Exports" })).not.toBeInTheDocument());
    expect(screen.getByText("1 file selected")).toBeInTheDocument();
    expect(api.importMarketExports).not.toHaveBeenCalled();
  });

  it("accepts multiple files through drag and drop", async () => {
    const { container } = render(
      <MemoryRouter>
        <MarketImportsPage />
      </MemoryRouter>,
    );
    const zone = screen.getByText("Drop EVE market exports here").parentElement!.parentElement!;
    const files = [
      new File(["a"], "Tritanium.txt"),
      new File(["b"], "Pyerite.csv"),
    ];
    fireEvent.drop(zone, { dataTransfer: { files } });
    expect(container.textContent).toContain("2 files selected");
    await waitFor(() => expect(api.previewMarketExports).toHaveBeenCalledWith(files));
  });

  it("disables import when every selected file is already imported", async () => {
    const user = userEvent.setup();
    api.previewMarketExports.mockResolvedValue({
      ...preview,
      files: [{ ...preview.files[0], alreadyImported: true, canImport: false }],
      validFiles: 0,
      invalidFiles: 0,
      duplicateFiles: 1,
    });
    const { container } = render(
      <MemoryRouter>
        <MarketImportsPage />
      </MemoryRouter>,
    );
    const input = container.querySelector('input[type="file"]') as HTMLInputElement;
    await user.upload(input, new File(["fixture"], preview.files[0].filename, { type: "text/plain" }));
    expect(await screen.findByText("Every selected file is already stored. No new market observations will be added.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Already imported" })).toBeDisabled();
    expect(api.importMarketExports).not.toHaveBeenCalled();
  });

});
