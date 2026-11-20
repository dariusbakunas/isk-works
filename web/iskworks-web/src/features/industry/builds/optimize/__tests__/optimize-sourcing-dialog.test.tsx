import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { SourcingChange, SourcingProposal } from "../../../../../api/industry";

const industryApi = vi.hoisted(() => ({
  previewSourcingOptimization: vi.fn(),
  applySourcingOptimization: vi.fn(),
}));

vi.mock("../../../../../api/industry", async () => {
  const actual = await vi.importActual<typeof import("../../../../../api/industry")>(
    "../../../../../api/industry",
  );
  return { ...actual, ...industryApi };
});

import { OptimizeSourcingDialog } from "../optimize-sourcing-dialog";

function change(overrides: Partial<SourcingChange>): SourcingChange {
  return {
    typeId: 1,
    typeName: "Item",
    quantity: 100,
    from: "buy",
    to: { kind: "produce", recipe: { mode: "reaction", reactionFormulaTypeId: 9 } },
    buyUnit: "5.0000",
    produceUnit: "1.1000",
    saving: "390.00",
    depth: 1,
    ...overrides,
  };
}

const X = change({ typeId: 700, typeName: "Ferrogel" });
const Y = change({
  typeId: 800,
  typeName: "Hexite",
  from: null,
  depth: 2,
  buyUnit: "3.0000",
  produceUnit: "1.0500",
  saving: "195.00",
});
const BUY = change({
  typeId: 900,
  typeName: "Wreathe",
  from: "produce",
  to: { kind: "buy" },
  buyUnit: "1276000.0000",
  produceUnit: "1576825.9784",
  saving: "601651.96",
});

function proposal(overrides: Partial<SourcingProposal> = {}): SourcingProposal {
  return {
    fingerprint: "fp-1",
    expectedRevenue: "10000.0000",
    currentCost: "505.0000",
    proposedCost: "115.0000",
    currentMargin: "9495.0000",
    proposedMargin: "9885.0000",
    delta: "390.0000",
    changes: [X, Y, BUY],
    warnings: [],
    notice: null,
    ...overrides,
  };
}

beforeEach(() => {
  industryApi.previewSourcingOptimization.mockReset();
  industryApi.applySourcingOptimization.mockReset();
});

function renderDialog(onClose = vi.fn()) {
  render(<OptimizeSourcingDialog buildId="root-1" onClose={onClose} open />);
  return onClose;
}

describe("OptimizeSourcingDialog", () => {
  it("previews with the default threshold and lists changes by level", async () => {
    industryApi.previewSourcingOptimization.mockResolvedValue(proposal());
    renderDialog();

    const table = await screen.findByRole("table", { name: "Proposed sourcing changes" });
    expect(industryApi.previewSourcingOptimization).toHaveBeenCalledWith("root-1", {
      minSavingPercent: "2",
      includeWithoutBlueprint: false,
      exclude: [],
    });
    const ferrogel = within(table).getByText("Ferrogel").closest("tr")!;
    expect(within(ferrogel).getByText("React")).toBeInTheDocument();
    expect(within(ferrogel).getByText("Buy")).toBeInTheDocument();
    // Money cells are compact with the exact value on hover.
    expect(within(ferrogel).getByTitle(/390/)).toBeInTheDocument();
    const hexite = within(table).getByText("Hexite").closest("tr")!;
    expect(within(hexite).getByText("New")).toBeInTheDocument();
    const wreathe = within(table).getByText("Wreathe").closest("tr")!;
    expect(within(wreathe).getByText("Build")).toBeInTheDocument();
    expect(within(table).getByText("Level 2")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Apply selected (3)" })).toBeEnabled();
  });

  it("re-previews with an unticked row excluded and keeps it listed", async () => {
    industryApi.previewSourcingOptimization
      .mockResolvedValue(proposal())
      .mockResolvedValueOnce(proposal())
      .mockResolvedValueOnce(proposal({ changes: [X, Y] }));
    renderDialog();

    await userEvent.click(await screen.findByRole("checkbox", { name: "Apply Wreathe" }));

    await waitFor(() =>
      expect(industryApi.previewSourcingOptimization).toHaveBeenLastCalledWith("root-1", {
        minSavingPercent: "2",
        includeWithoutBlueprint: false,
        exclude: [900],
      }),
    );
    expect(await screen.findByRole("button", { name: "Apply selected (2)" })).toBeEnabled();
    const wreathe = screen.getByRole("checkbox", { name: "Apply Wreathe" });
    expect(wreathe).not.toBeChecked();

    await userEvent.click(wreathe);
    await waitFor(() =>
      expect(industryApi.previewSourcingOptimization).toHaveBeenLastCalledWith(
        "root-1",
        expect.objectContaining({ exclude: [] }),
      ),
    );
  });

  it("applies the proposal and closes as changed", async () => {
    industryApi.previewSourcingOptimization.mockResolvedValue(proposal());
    industryApi.applySourcingOptimization.mockResolvedValue({ applied: [700, 800, 900], failed: [], rootRevision: 4 });
    const onClose = renderDialog();

    await userEvent.click(await screen.findByRole("button", { name: "Apply selected (3)" }));

    expect(industryApi.applySourcingOptimization).toHaveBeenCalledWith(
      "root-1",
      expect.objectContaining({ fingerprint: "fp-1", changes: [X, Y, BUY] }),
    );
    await waitFor(() => expect(onClose).toHaveBeenCalledWith(true));
  });

  it("lists failed writes and closes as changed when some applied", async () => {
    industryApi.previewSourcingOptimization.mockResolvedValue(proposal());
    industryApi.applySourcingOptimization.mockResolvedValue({
      applied: [700],
      failed: [{ typeId: 800, buildId: "b-2", error: "Revision conflict" }],
      rootRevision: 4,
    });
    const onClose = renderDialog();

    await userEvent.click(await screen.findByRole("button", { name: "Apply selected (3)" }));

    expect(await screen.findByText("Some changes were not applied")).toBeInTheDocument();
    expect(screen.getByText(/Hexite:\s*Revision conflict/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Re-run" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(onClose).toHaveBeenCalledWith(true);
  });

  it("says when sourcing is already optimal", async () => {
    industryApi.previewSourcingOptimization.mockResolvedValue(
      proposal({ changes: [], proposedCost: "505.0000", proposedMargin: "9495.0000", delta: "0" }),
    );
    renderDialog();

    expect(await screen.findByText("Sourcing is already optimal at this threshold.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Apply selected (0)" })).toBeDisabled();
  });

  it("shows warnings and rejects an invalid threshold", async () => {
    industryApi.previewSourcingOptimization.mockResolvedValue(
      proposal({ warnings: [{ typeId: 11, typeName: "Capital Armor Plates", kind: "no_blueprint" }] }),
    );
    renderDialog();

    expect(
      await screen.findByText("Capital Armor Plates: no synced blueprint, so it stays bought"),
    ).toBeInTheDocument();
    const input = screen.getByRole("textbox", { name: "Minimum saving percent" });
    await userEvent.clear(input);
    await userEvent.type(input, "120");
    expect(screen.getByText("Enter a percentage from 0 to below 100.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Apply selected (3)" })).toBeDisabled();
  });
});
