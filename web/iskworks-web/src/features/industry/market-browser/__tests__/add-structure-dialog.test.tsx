import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router";
import { beforeEach, describe, expect, test, vi } from "vitest";

import type { KnownStructure } from "../../../../api/industry";
import { AddStructureDialog } from "../add-structure-dialog";

const api = vi.hoisted(() => ({
  searchKnownStructures: vi.fn(),
  resolveKnownStructure: vi.fn(),
  verifyStructureMarketAccess: vi.fn(),
}));

vi.mock("../../../../api/industry", async () => {
  const actual = await vi.importActual<typeof import("../../../../api/industry")>("../../../../api/industry");
  return {
    ...actual,
    searchKnownStructures: api.searchKnownStructures,
    resolveKnownStructure: api.resolveKnownStructure,
    verifyStructureMarketAccess: api.verifyStructureMarketAccess,
  };
});

const staging: KnownStructure = {
  structureId: 1_050_487_654_321,
  structureName: "C-J6MT 1st Tai Mahgoon",
  structureTypeId: 35_827,
  structureTypeName: "Sotiyo",
  solarSystemId: 30_000_505,
  solarSystemName: "C-J6MT",
  securityClass: "nullSec",
};

function renderDialog(props: { onAdded: () => void; onClose: () => void }) {
  return render(
    <MemoryRouter>
      <AddStructureDialog onAdded={props.onAdded} onClose={props.onClose} open />
    </MemoryRouter>,
  );
}

async function pickFromSearch(name = "Tai Mahgoon") {
  const user = userEvent.setup();
  await user.type(screen.getByLabelText("Structure"), name);
  await user.click(await screen.findByText(staging.structureName));
  await user.click(screen.getByRole("button", { name: "Verify market access" }));
}

describe("AddStructureDialog", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    api.searchKnownStructures.mockResolvedValue([staging]);
  });

  test("picking a known structure from search then confirmed access calls onAdded", async () => {
    api.verifyStructureMarketAccess.mockResolvedValue({
      access: "confirmed",
      characterName: "Valka",
    });
    const onAdded = vi.fn();
    renderDialog({ onAdded, onClose: vi.fn() });

    await pickFromSearch();

    expect(api.verifyStructureMarketAccess).toHaveBeenCalledWith(staging.structureId);
    expect(await screen.findByText(/can price this market\./)).toBeInTheDocument();
    expect(onAdded).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("button", { name: "Done" })).toBeInTheDocument();
  });

  test("no eligible character points the user at reconnecting", async () => {
    api.verifyStructureMarketAccess.mockResolvedValue({ access: "noEligibleCharacter" });
    const onAdded = vi.fn();
    renderDialog({ onAdded, onClose: vi.fn() });

    await pickFromSearch();

    expect(await screen.findByText("No character has market access granted")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Open Characters" })).toHaveAttribute("href", "/characters");
    expect(onAdded).not.toHaveBeenCalled();
  });

  test("every eligible character denied shows the docking-rights explanation", async () => {
    api.verifyStructureMarketAccess.mockResolvedValue({ access: "denied" });
    renderDialog({ onAdded: vi.fn(), onClose: vi.fn() });

    await pickFromSearch();

    expect(await screen.findByText("No connected character can dock here")).toBeInTheDocument();
  });

  test("a structure absent from search can still be resolved by pasting its numeric ID", async () => {
    api.searchKnownStructures.mockResolvedValue([]);
    api.resolveKnownStructure.mockResolvedValue({
      configured: true,
      structure: staging,
      needsReconnection: false,
      eligibleCharacterCount: 1,
      warnings: [],
    });
    api.verifyStructureMarketAccess.mockResolvedValue({
      access: "confirmed",
      characterName: "Valka",
    });
    const user = userEvent.setup();
    renderDialog({ onAdded: vi.fn(), onClose: vi.fn() });

    await user.type(screen.getByLabelText("Structure"), "1050487654321");
    await user.click(await screen.findByRole("button", { name: /Resolve structure 1050487654321 via ESI/ }));

    expect(api.resolveKnownStructure).toHaveBeenCalledWith(1_050_487_654_321);
    await screen.findByRole("button", { name: "Verify market access" });
    await user.click(screen.getByRole("button", { name: "Verify market access" }));

    expect(await screen.findByText(/can price this market\./)).toBeInTheDocument();
  });

  test("a structure absent from search offers no resolve action for a plain name", async () => {
    api.searchKnownStructures.mockResolvedValue([]);
    const user = userEvent.setup();
    renderDialog({ onAdded: vi.fn(), onClose: vi.fn() });

    await user.type(screen.getByLabelText("Structure"), "Some Unknown Place");

    expect(await screen.findByText(/No known structures match/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Resolve structure/ })).not.toBeInTheDocument();
    expect(screen.getByText(/Shift-drag the structure/)).toBeInTheDocument();
  });
});
