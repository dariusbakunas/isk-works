import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { ApiError } from "../../../api/workspace";
import type { CombineSuggestion } from "../combine-suggestions";
import { ticketFixture } from "./fixtures";

const industryApi = vi.hoisted(() => ({ combineTickets: vi.fn() }));

vi.mock("../../../api/industry", async () => {
  const actual = await vi.importActual<typeof import("../../../api/industry")>("../../../api/industry");
  return { ...actual, ...industryApi };
});

import { CombineSuggestionsDialog } from "../combine-suggestions-dialog";

function suggestion(name: string, ids: string[], overrides: Partial<CombineSuggestion> = {}): CombineSuggestion {
  return {
    key: name,
    capturedName: name,
    facility: { id: "facility-1", name: "Tatara" } as never,
    tickets: ids.map((id) =>
      ticketFixture({
        id,
        displayId: `ISK-${id}`,
        kind: "reaction",
        capturedName: name,
      }),
    ),
    quantity: 3000,
    runs: 20,
    epicCount: ids.length,
    estimatedLineTotal: "1000000.0000",
    mixedAssignees: false,
    ...overrides,
  };
}

const groups = [suggestion("Nanotransistors", ["1", "2"]), suggestion("Sylramic Fibers", ["3", "4", "5"])];

describe("CombineSuggestionsDialog", () => {
  beforeEach(() => {
    industryApi.combineTickets.mockReset();
  });

  it("lists every group, checked by default", () => {
    render(<CombineSuggestionsDialog onClose={vi.fn()} open suggestions={groups} />);

    const dialog = screen.getByRole("dialog", { name: "Combine suggestions" });
    expect(within(dialog).getByRole("checkbox", { name: /Nanotransistors/ })).toBeChecked();
    expect(within(dialog).getByRole("checkbox", { name: /Sylramic Fibers/ })).toBeChecked();
    expect(within(dialog).getAllByText(/Tatara/)).toHaveLength(2);
    expect(within(dialog).getByRole("button", { name: "Combine selected (2)" })).toBeEnabled();
  });

  it("combines only the checked groups, one request each", async () => {
    const user = userEvent.setup();
    industryApi.combineTickets.mockResolvedValue(ticketFixture({ id: "combined" }));
    render(<CombineSuggestionsDialog onClose={vi.fn()} open suggestions={groups} />);

    await user.click(screen.getByRole("checkbox", { name: /Nanotransistors/ }));
    await user.click(screen.getByRole("button", { name: "Combine selected (1)" }));

    expect(industryApi.combineTickets).toHaveBeenCalledTimes(1);
    expect(industryApi.combineTickets).toHaveBeenCalledWith(["3", "4", "5"]);
    expect(await screen.findByText("Combined")).toBeInTheDocument();
  });

  it("keeps going past a failed group and reports it", async () => {
    const user = userEvent.setup();
    industryApi.combineTickets
      .mockRejectedValueOnce(
        new ApiError(409, {
          code: "ticket_not_combinable",
          message: "ISK-1 can't be combined: it has linked industry jobs",
        }),
      )
      .mockResolvedValueOnce(ticketFixture({ id: "combined" }));
    const onClose = vi.fn();
    render(<CombineSuggestionsDialog onClose={onClose} open suggestions={groups} />);

    await user.click(screen.getByRole("button", { name: "Combine selected (2)" }));

    expect(await screen.findByText(/it has linked industry jobs/)).toBeInTheDocument();
    expect(screen.getByText("Combined")).toBeInTheDocument();
    expect(industryApi.combineTickets).toHaveBeenCalledTimes(2);

    await user.click(screen.getByRole("button", { name: "Done" }));
    expect(onClose).toHaveBeenCalledWith(true);
  });

  it("closes without changes", async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    render(<CombineSuggestionsDialog onClose={onClose} open suggestions={groups} />);

    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onClose).toHaveBeenCalledWith(false);
  });
});
