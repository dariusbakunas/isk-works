import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, test, vi } from "vitest";

import type { MarketImportPreview, MarketImportPreviewFile, MarketImportResult } from "../../../../api/industry";
import { MarketImportDialog } from "../market-import-dialog";

const api = vi.hoisted(() => ({
  previewMarketExports: vi.fn(),
  importMarketExports: vi.fn(),
  resolveMarketLocations: vi.fn(),
}));

vi.mock("../../../../api/industry", async () => {
  const actual = await vi.importActual<typeof import("../../../../api/industry")>("../../../../api/industry");
  return {
    ...actual,
    previewMarketExports: api.previewMarketExports,
    importMarketExports: api.importMarketExports,
    resolveMarketLocations: api.resolveMarketLocations,
  };
});

function file(overrides: Partial<MarketImportPreviewFile> = {}): MarketImportPreviewFile {
  return {
    filename: "market-export.txt",
    fileChecksum: "checksum-1",
    normalizedChecksum: "normalized-1",
    fileSizeBytes: 1024,
    typeId: 34,
    typeName: "Tritanium",
    locationId: 60_003_760,
    locationName: "Jita IV - Moon 4 - Caldari Navy Assembly Plant",
    solarSystemId: 30_000_142,
    solarSystemName: "Jita",
    regionId: 10_000_002,
    regionName: "The Forge",
    observedAt: "2026-08-24T12:00:00Z",
    timestampSource: "filename",
    rowCount: 6,
    buyOrderCount: 2,
    sellOrderCount: 4,
    lowestSell: "4.2500",
    highestBuy: "4.0000",
    totalBuyVolume: 1000,
    totalSellVolume: 2000,
    duplicateOrderCount: 0,
    alreadyImported: false,
    canImport: true,
    warnings: [],
    errors: [],
    ...overrides,
  };
}

function preview(files: MarketImportPreviewFile[]): MarketImportPreview {
  return {
    files,
    totalFiles: files.length,
    validFiles: files.filter((entry) => entry.canImport).length,
    invalidFiles: files.filter((entry) => !entry.canImport).length,
    duplicateFiles: files.filter((entry) => entry.alreadyImported).length,
    itemCount: new Set(files.map((entry) => entry.typeId)).size,
    locationCount: new Set(files.map((entry) => entry.locationId)).size,
    totalRows: files.reduce((sum, entry) => sum + entry.rowCount, 0),
    oldestObservationAt: null,
    newestObservationAt: null,
  };
}

const importedResult: MarketImportResult = {
  batch: {
    id: "batch-1",
    workspaceId: "workspace-1",
    observedAtMin: "2026-08-24T12:00:00Z",
    observedAtMax: "2026-08-24T12:00:00Z",
    importedAt: "2026-08-24T12:01:00Z",
    fileCount: 1,
    itemCount: 1,
    locationCount: 1,
    observationCount: 6,
    skippedDuplicateFileCount: 0,
    warnings: [],
    files: [],
  },
  importedFiles: 1,
  skippedDuplicateFiles: 0,
  failedFiles: [],
  importedObservations: 6,
  warnings: [],
};

function selectFile(input: HTMLElement, name = "export.txt") {
  const selected = new File(["content"], name, { type: "text/plain" });
  return userEvent.setup().upload(input, selected);
}

describe("MarketImportDialog", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  test("picking a file previews it, shows the detected scope, then imports and confirms", async () => {
    api.previewMarketExports.mockResolvedValue(preview([file()]));
    api.importMarketExports.mockResolvedValue(importedResult);
    const onImported = vi.fn();
    render(<MarketImportDialog onClose={vi.fn()} onImported={onImported} open />);

    const input = document.querySelector('input[type="file"]') as HTMLInputElement;
    await selectFile(input, "export.txt");

    expect(await screen.findByText("Market export detected")).toBeInTheDocument();
    expect(screen.getByText("The Forge")).toBeInTheDocument();
    expect(screen.getByText("Jita IV - Moon 4 - Caldari Navy Assembly Plant")).toBeInTheDocument();

    await userEvent.setup().click(screen.getByRole("button", { name: "Import" }));

    expect(api.importMarketExports).toHaveBeenCalled();
    expect(await screen.findByText("Market observations imported")).toBeInTheDocument();
    expect(onImported).toHaveBeenCalledTimes(1);
  });

  test("a file with two distinct scopes shows one detected-scope card per scope", async () => {
    api.previewMarketExports.mockResolvedValue(
      preview([
        file(),
        file({
          typeId: 35,
          typeName: "Pyerite",
          locationId: 60_004_588,
          locationName: "Rens VI - Moon 8 - Brutor Tribe Treasury",
          regionId: 10_000_030,
          regionName: "Heimatar",
        }),
      ]),
    );
    render(<MarketImportDialog onClose={vi.fn()} onImported={vi.fn()} open />);

    const input = document.querySelector('input[type="file"]') as HTMLInputElement;
    await selectFile(input);

    const cards = await screen.findAllByText("Market export detected");
    expect(cards).toHaveLength(2);
    expect(screen.getByText("The Forge")).toBeInTheDocument();
    expect(screen.getByText("Heimatar")).toBeInTheDocument();
  });

  test("a failed preview shows an inline error instead of the detected scope", async () => {
    api.previewMarketExports.mockRejectedValue(new Error("boom"));
    render(<MarketImportDialog onClose={vi.fn()} onImported={vi.fn()} open />);

    const input = document.querySelector('input[type="file"]') as HTMLInputElement;
    await selectFile(input);

    expect(await screen.findByText("Market import unavailable")).toBeInTheDocument();
    expect(screen.queryByText("Market export detected")).not.toBeInTheDocument();
  });

  test("an unresolved private structure shows the resolution banner but numeric ids stay authoritative", async () => {
    api.previewMarketExports.mockResolvedValue(
      preview([file({ locationId: 1_049_588_174_021, locationName: "Structure 1049588174021" })]),
    );
    api.resolveMarketLocations.mockResolvedValue({
      configured: true,
      resolved: [],
      unresolvedLocationIds: [1_049_588_174_021],
      eligibleCharacterCount: 1,
      needsReconnection: false,
      warnings: [],
    });
    render(<MarketImportDialog onClose={vi.fn()} onImported={vi.fn()} open />);

    const input = document.querySelector('input[type="file"]') as HTMLInputElement;
    await selectFile(input);
    await screen.findByText("Market export detected");

    await waitFor(() => expect(api.resolveMarketLocations).toHaveBeenCalledWith([1_049_588_174_021]));
    expect(await screen.findByText("Some structure names are private")).toBeInTheDocument();
  });

  test("closing and reopening resets back to the idle drop state", async () => {
    api.previewMarketExports.mockResolvedValue(preview([file()]));
    const { rerender } = render(<MarketImportDialog onClose={vi.fn()} onImported={vi.fn()} open />);
    const input = document.querySelector('input[type="file"]') as HTMLInputElement;
    await selectFile(input);
    await screen.findByText("Market export detected");

    rerender(<MarketImportDialog onClose={vi.fn()} onImported={vi.fn()} open={false} />);
    rerender(<MarketImportDialog onClose={vi.fn()} onImported={vi.fn()} open />);

    expect(await screen.findByText("Drop EVE market exports here")).toBeInTheDocument();
  });
});
