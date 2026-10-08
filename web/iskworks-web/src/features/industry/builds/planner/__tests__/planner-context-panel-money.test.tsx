import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router";
import { beforeEach, describe, expect, test, vi } from "vitest";

import type { ProductionWorksheet as Worksheet, WorksheetItem } from "../../../../../api/industry";
import { PlannerContextPanel } from "../planner-context-panel";

const sdeApi = vi.hoisted(() => ({ recipeForProduct: vi.fn() }));
vi.mock("../../../../../api/sde", async () => {
  const actual = await vi.importActual<typeof import("../../../../../api/sde")>("../../../../../api/sde");
  return { ...actual, ...sdeApi };
});

const industryApi = vi.hoisted(() => ({
  listBlueprintObservations: vi.fn(),
}));
vi.mock("../../../../../api/industry", async () => {
  const actual = await vi.importActual<typeof import("../../../../../api/industry")>("../../../../../api/industry");
  return { ...actual, ...industryApi };
});

function outputItem(overrides: Partial<WorksheetItem> = {}): WorksheetItem {
  return {
    typeId: 23913,
    typeName: "Thanatos",
    role: "output",
    requiredQuantity: 1,
    availableQuantity: 0,
    coveredQuantity: 0,
    missingQuantity: 0,
    coveragePercentage: "0.00",
    projectedInventoryCost: null,
    pricing: {
      selectionKind: "default",
      effectivePolicy: "lowestSell",
      unitPrice: null,
      manualUnitPrice: null,
      missing: true,
      sourceNote: "",
    },
    lineTotal: null,
    contributions: [],
    isBuildResolved: false,
    installationCost: null,
    ...overrides,
  };
}

const worksheet: Worksheet = {
  groups: [],
  output: { key: "output", label: "Output", items: [outputItem()] },
  summary: {
    materialCost: "0.0000",
    installationCost: null,
    totalCost: null,
    expectedRevenue: null,
    estimatedMargin: null,
    pricingComplete: false,
    quantityCoverageComplete: true,
    costCoverageComplete: true,
    warnings: [],
  },
};

function renderPanel(item: WorksheetItem, onPricingChange = vi.fn()) {
  render(
    <MemoryRouter>
      <PlannerContextPanel onPricingChange={onPricingChange} selectedItem={item} worksheet={worksheet} />
    </MemoryRouter>,
  );
  return { onPricingChange };
}

async function enterManualMode() {
  await userEvent.click(screen.getByRole("radio", { name: "Manual price" }));
  return screen.getByLabelText("Unit price");
}

describe("PlannerContextPanel -- manual price presentation boundary", () => {
  beforeEach(() => vi.clearAllMocks());

  test("typing invalid text never publishes to onPricingChange and keeps the inspector mounted", async () => {
    const { onPricingChange } = renderPanel(outputItem());
    const field = await enterManualMode();

    await userEvent.type(field, "1.234567");
    await new Promise((resolve) => setTimeout(resolve, 400));

    expect(onPricingChange).not.toHaveBeenCalled();
    expect(screen.getByRole("complementary", { name: "Thanatos" })).toBeInTheDocument();
    expect(await screen.findByRole("alert")).toHaveTextContent(/4 decimal places/i);
  });

  test("malformed comma grouping is rejected locally on blur without publishing", async () => {
    const { onPricingChange } = renderPanel(outputItem());
    const field = await enterManualMode();

    await userEvent.type(field, "1,00");
    field.blur();

    expect(await screen.findByRole("alert")).toHaveTextContent(/commas only every 3 digits/i);
    expect(onPricingChange).not.toHaveBeenCalled();
  });

  test("'1,000,000' commits the canonical unformatted string", async () => {
    const { onPricingChange } = renderPanel(outputItem());
    const field = await enterManualMode();

    await userEvent.type(field, "1,000,000");

    await waitFor(() =>
      expect(onPricingChange).toHaveBeenLastCalledWith({
        typeId: 23913,
        role: "output",
        selection: { kind: "manual", unit_price: "1000000" },
      }),
    );
  });

  test("'1,234.56' commits as '1234.56'", async () => {
    const { onPricingChange } = renderPanel(outputItem());
    const field = await enterManualMode();

    await userEvent.type(field, "1,234.56");

    await waitFor(() =>
      expect(onPricingChange).toHaveBeenLastCalledWith(
        expect.objectContaining({ selection: { kind: "manual", unit_price: "1234.56" } }),
      ),
    );
  });

  test("four fractional digits commit; five are rejected locally", async () => {
    const { onPricingChange } = renderPanel(outputItem());
    const field = await enterManualMode();

    await userEvent.type(field, "1234.5678");
    await waitFor(() =>
      expect(onPricingChange).toHaveBeenLastCalledWith(
        expect.objectContaining({ selection: { kind: "manual", unit_price: "1234.5678" } }),
      ),
    );

    onPricingChange.mockClear();
    await userEvent.type(field, "9");
    await new Promise((resolve) => setTimeout(resolve, 400));
    expect(onPricingChange).not.toHaveBeenCalled();
    expect(await screen.findByRole("alert")).toHaveTextContent(/4 decimal places/i);
  });

  test("negative values are rejected locally", async () => {
    const { onPricingChange } = renderPanel(outputItem());
    const field = await enterManualMode();

    await userEvent.type(field, "-1");
    await new Promise((resolve) => setTimeout(resolve, 400));

    expect(onPricingChange).not.toHaveBeenCalled();
    expect(await screen.findByRole("alert")).toBeInTheDocument();
  });

  test("incomplete typing states do not commit", async () => {
    const { onPricingChange } = renderPanel(outputItem());
    const field = await enterManualMode();

    await userEvent.type(field, "1,");
    await new Promise((resolve) => setTimeout(resolve, 400));
    expect(onPricingChange).not.toHaveBeenCalled();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();

    await userEvent.type(field, "000.");
    await new Promise((resolve) => setTimeout(resolve, 400));
    expect(onPricingChange).not.toHaveBeenCalled();
  });

  test("correcting invalid input clears the error and commits", async () => {
    const { onPricingChange } = renderPanel(outputItem());
    const field = await enterManualMode();

    await userEvent.type(field, "1.234567");
    expect(await screen.findByRole("alert")).toBeInTheDocument();

    await userEvent.clear(field);
    await userEvent.type(field, "1000");
    await waitFor(() =>
      expect(onPricingChange).toHaveBeenLastCalledWith(
        expect.objectContaining({ selection: { kind: "manual", unit_price: "1000" } }),
      ),
    );
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  test("emptying an existing manual price removes the override without submitting an empty string", async () => {
    const item = outputItem({
      pricing: {
        selectionKind: "manual",
        effectivePolicy: "lowestSell",
        unitPrice: "2460.0000",
        manualUnitPrice: "2460.0000",
        missing: false,
        sourceNote: "",
      },
    });
    const { onPricingChange } = renderPanel(item);

    const field = screen.getByLabelText("Unit price");
    await userEvent.clear(field);
    field.blur();

    await waitFor(() => expect(onPricingChange).toHaveBeenCalled());
    for (const [arg] of onPricingChange.mock.calls) {
      expect(arg.selection).not.toEqual({ kind: "manual", unit_price: "" });
    }
    expect(onPricingChange).toHaveBeenLastCalledWith(
      expect.objectContaining({ typeId: 23913, role: "output", selection: { kind: "default" } }),
    );
  });
});
