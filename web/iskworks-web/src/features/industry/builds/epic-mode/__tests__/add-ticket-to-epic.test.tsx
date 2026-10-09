import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { OrderSummary } from "../../../../../api/industry";
import { AddTicketToEpicButton } from "../add-ticket-to-epic";

vi.mock("../../../../../api/industry", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../../../../api/industry")>()),
  listOrders: vi.fn(async () => [
    { id: "epic-1", displayName: "Manufacture Muninn", status: "blocked" } as unknown as OrderSummary,
  ]),
  listBuilds: vi.fn(async () => []),
}));
vi.mock("../../../../../api/characters", () => ({ listCharacters: vi.fn(async () => []) }));

describe("AddTicketToEpicButton", () => {
  it("opens the ticket editor with the Epic already chosen", async () => {
    const user = userEvent.setup();
    render(<AddTicketToEpicButton epicId="epic-1" onCreated={vi.fn()} />);

    await user.click(screen.getByRole("button", { name: "Add ticket to Epic" }));

    expect(await screen.findByLabelText("Epic")).toHaveValue("epic-1");
  });
});
