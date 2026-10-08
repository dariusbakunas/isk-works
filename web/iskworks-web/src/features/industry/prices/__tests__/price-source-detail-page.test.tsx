import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes } from "react-router";
import { beforeEach, describe, expect, test, vi } from "vitest";

import type { PriceSource } from "../../../../api/industry";

const industryApi = vi.hoisted(() => ({
  getPriceSource: vi.fn(),
  upsertPriceItem: vi.fn(),
}));
vi.mock("../../../../api/industry", async () => {
  const actual = await vi.importActual<typeof import("../../../../api/industry")>("../../../../api/industry");
  return { ...actual, ...industryApi };
});

const sdeApi = vi.hoisted(() => ({ searchTypes: vi.fn() }));
vi.mock("../../../../api/sde", async () => {
  const actual = await vi.importActual<typeof import("../../../../api/sde")>("../../../../api/sde");
  return { ...actual, ...sdeApi };
});

import { PriceSourceDetailPage } from "../price-source-detail-page";

function priceSource(): PriceSource {
  return {
    id: "src-1",
    workspaceId: "ws-1",
    name: "Conservative fallback",
    description: "",
    kind: "manual",
    revision: 3,
    itemCount: 0,
    recentBuildCount: 0,
    items: [],
    createdAt: "2026-08-01T00:00:00Z",
    updatedAt: "2026-08-01T00:00:00Z",
  };
}

function renderPage() {
  return render(
    <MemoryRouter initialEntries={["/prices/src-1"]}>
      <Routes>
        <Route element={<PriceSourceDetailPage />} path="/prices/:priceSourceId" />
      </Routes>
    </MemoryRouter>,
  );
}

describe("PriceSourceDetailPage -- Unit price (ISK) via MoneyInput", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    industryApi.getPriceSource.mockResolvedValue(priceSource());
    industryApi.upsertPriceItem.mockImplementation(async () => priceSource());
    sdeApi.searchTypes.mockResolvedValue([]);
  });

  async function fillTypeId() {
    await screen.findByText("Add a price");
    await userEvent.type(screen.getByLabelText("EVE type ID"), "34");
    await userEvent.type(screen.getByLabelText("Item name"), "Tritanium");
  }

  test("accepts comma grouping and submits only the canonical string on explicit Save", async () => {
    renderPage();
    await fillTypeId();

    await userEvent.type(screen.getByLabelText("Unit price (ISK)"), "1,000,000");
    await userEvent.click(screen.getByRole("button", { name: /Save price/ }));

    await waitFor(() => expect(industryApi.upsertPriceItem).toHaveBeenCalled());
    expect(industryApi.upsertPriceItem).toHaveBeenCalledWith(
      "src-1",
      34,
      expect.objectContaining({ price: "1000000" }),
    );
  });

  test("does not persist a valid value until the user presses Save", async () => {
    renderPage();
    await fillTypeId();

    await userEvent.type(screen.getByLabelText("Unit price (ISK)"), "1,234.56");
    await new Promise((resolve) => setTimeout(resolve, 350));

    expect(industryApi.upsertPriceItem).not.toHaveBeenCalled();
  });

  test("shows an inline error and blocks Save while the price is invalid", async () => {
    renderPage();
    await fillTypeId();

    await userEvent.type(screen.getByLabelText("Unit price (ISK)"), "1.234567");

    expect(await screen.findByRole("alert")).toHaveTextContent(/4 decimal places/i);
    expect(screen.getByRole("button", { name: /Save price/ })).toBeDisabled();

    await userEvent.click(screen.getByRole("button", { name: /Save price/ }));
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(industryApi.upsertPriceItem).not.toHaveBeenCalled();
  });

  test("blocks Save while the price is empty", async () => {
    renderPage();
    await fillTypeId();

    expect(screen.getByRole("button", { name: /Save price/ })).toBeDisabled();
  });
});
