import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, test, vi } from "vitest";

import type { FacilityProfile } from "../../../../api/industry";
import { FacilitiesPage } from "../facilities-page";

const industryApi = vi.hoisted(() => ({
  listFacilities: vi.fn(),
  createFacility: vi.fn(),
  updateFacility: vi.fn(),
  deleteFacility: vi.fn(),
  exportFacilities: vi.fn(),
  importFacilities: vi.fn(),
  previewFacilityImport: vi.fn(),
  getSystemCostIndex: vi.fn(),
  searchKnownStructures: vi.fn(),
  searchNpcStations: vi.fn(),
  searchSolarSystems: vi.fn(),
}));

const sdeApi = vi.hoisted(() => ({
  searchStructureRigs: vi.fn(),
  searchReactionRigs: vi.fn(),
  getRigManufacturingModifiers: vi.fn(),
  getRigReactionModifiers: vi.fn(),
  getStructureManufacturingModifiers: vi.fn(),
  searchStructureTypes: vi.fn(),
}));

vi.mock("../../../../api/industry", async () => {
  const actual = await vi.importActual<typeof import("../../../../api/industry")>("../../../../api/industry");
  return { ...actual, ...industryApi };
});

vi.mock("../../../../api/sde", async () => {
  const actual = await vi.importActual<typeof import("../../../../api/sde")>("../../../../api/sde");
  return { ...actual, ...sdeApi };
});

function reactionProfile(overrides: Partial<FacilityProfile> = {}): FacilityProfile {
  return {
    id: "facility-1",
    workspaceId: "workspace-1",
    name: "Athanor",
    kind: "manual",
    role: "reaction",
    structureId: null,
    structureTypeId: null,
    structureTypeName: "",
    solarSystemId: null,
    solarSystemName: "",
    securityClass: "unknown",
    materialReductionPercent: "0",
    timeReductionPercent: "0",
    jobCostReductionPercent: "0",
    facilityTaxPercent: "0",
    sccSurchargePercent: "4",
    allianceSurchargePercent: "0",
    fixedSupplementalCost: "0",
    manualSystemCostIndex: null,
    notes: "",
    rigs: [],
    archivedAt: null,
    revision: 1,
    createdAt: "2026-07-27T10:00:00Z",
    updatedAt: "2026-07-27T10:00:00Z",
    ...overrides,
  };
}

function manufacturingProfile(overrides: Partial<FacilityProfile> = {}): FacilityProfile {
  return reactionProfile({
    id: "facility-mfg",
    name: "Sotiyo",
    kind: "upwellStructure",
    role: "manufacturing",
    structureTypeName: "Sotiyo",
    solarSystemName: "Jita",
    materialReductionPercent: "1",
    timeReductionPercent: "20",
    ...overrides,
  });
}

describe("FacilitiesPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    industryApi.listFacilities.mockResolvedValue([]);
    industryApi.searchSolarSystems.mockResolvedValue([]);
    industryApi.searchNpcStations.mockResolvedValue([]);
    industryApi.searchKnownStructures.mockResolvedValue([]);
    sdeApi.searchStructureTypes.mockResolvedValue([]);
    sdeApi.searchStructureRigs.mockResolvedValue([]);
    sdeApi.searchReactionRigs.mockResolvedValue([]);
  });

  test("switching role to reaction clears captured rigs and resets structure bonus fields", async () => {
    const user = userEvent.setup();
    render(<FacilitiesPage />);

    await user.click(await screen.findByRole("button", { name: "New Facility" }));
    await screen.findByRole("dialog", { name: "New Facility" });

    await user.clear(screen.getByLabelText("Structure material reduction %"));
    await user.type(screen.getByLabelText("Structure material reduction %"), "5");

    await user.click(screen.getByRole("button", { name: "Add rig" }));
    expect(screen.getByText("Rig slot 1")).toBeInTheDocument();

    await user.selectOptions(screen.getByLabelText("Role"), "reaction");

    expect(screen.queryByText("Rig slot 1")).not.toBeInTheDocument();
    expect(screen.getByLabelText("Structure material reduction %")).toHaveValue("0");
  });

  test("deleting a facility requires confirmation and calls permanent deletion", async () => {
    const user = userEvent.setup();
    const profile = reactionProfile({ name: "Alliance Tatara" });
    industryApi.listFacilities.mockResolvedValue([profile]);
    industryApi.deleteFacility.mockResolvedValue(undefined);
    render(<FacilitiesPage />);

    // The row action's label is generic (no facility name) so it is not
    // captured in session replay; the confirm dialog's button is distinct.
    await user.click(await screen.findByRole("button", { name: "Delete facility" }));
    expect(industryApi.deleteFacility).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Delete Facility" }));

    expect(industryApi.deleteFacility).toHaveBeenCalledWith(profile.id, profile.revision);
  });

  test("archived legacy facilities are hidden", async () => {
    industryApi.listFacilities.mockResolvedValue([
      reactionProfile({ id: "active", name: "Active Tatara" }),
      reactionProfile({ id: "archived", name: "Old Tatara", archivedAt: "2026-08-01T00:00:00Z" }),
    ]);
    render(<FacilitiesPage />);

    expect(await screen.findByText("Active Tatara")).toBeInTheDocument();
    expect(screen.queryByText("Old Tatara")).not.toBeInTheDocument();
  });

  test("editing an existing reaction facility preserves its role in the form", async () => {
    industryApi.listFacilities.mockResolvedValue([reactionProfile()]);
    const user = userEvent.setup();
    render(<FacilitiesPage />);

    await user.click(await screen.findByRole("button", { name: "Edit" }));
    await screen.findByRole("dialog", { name: "Edit Facility" });

    expect(screen.getByLabelText("Role")).toHaveValue("reaction");
  });

  test("adding a rig under a reaction-role facility searches and derives bonuses through the reaction endpoints", async () => {
    sdeApi.searchReactionRigs.mockResolvedValue([
      {
        typeId: 46_486,
        typeName: "Standup M-Set Composite Reactor Material Efficiency I",
        groupName: "Structure Composite Reactor Rig M - ME",
        published: true,
      },
    ]);
    sdeApi.getRigReactionModifiers.mockResolvedValue({
      typeId: 46_486,
      materialReductionPercent: "2.2",
      timeReductionPercent: "0",
      compatibleWithStructure: null,
    });

    const user = userEvent.setup();
    render(<FacilitiesPage />);

    await user.click(await screen.findByRole("button", { name: "New Facility" }));
    await screen.findByRole("dialog", { name: "New Facility" });

    await user.selectOptions(screen.getByLabelText("Role"), "reaction");
    await user.click(screen.getByRole("button", { name: "Add rig" }));
    await user.type(screen.getByLabelText("Rig"), "Composite");

    await expect(screen.findByRole("button", {
      name: /Standup M-Set Composite Reactor Material Efficiency I/,
    }, { timeout: 1000 })).resolves.toBeInTheDocument();

    expect(sdeApi.searchReactionRigs).toHaveBeenCalledWith("Composite", null);
    expect(sdeApi.searchStructureRigs).not.toHaveBeenCalled();

    await user.click(screen.getByRole("button", {
      name: /Standup M-Set Composite Reactor Material Efficiency I/,
    }));

    await expect(
      screen.findByDisplayValue("2.2", {}, { timeout: 1000 }),
    ).resolves.toBeInTheDocument();
    expect(sdeApi.getRigReactionModifiers).toHaveBeenCalledWith(46_486, "unknown", null);
    expect(sdeApi.getRigManufacturingModifiers).not.toHaveBeenCalled();
  });

  test("selecting a solar system for a reaction-role facility auto-fills the reaction cost index", async () => {
    industryApi.searchSolarSystems.mockResolvedValue([{
      solarSystemId: 30_000_799,
      solarSystemName: "0-VG7A",
      securityClass: "nullSec",
    }]);
    industryApi.getSystemCostIndex.mockResolvedValue({
      solarSystemId: 30_000_799,
      manufacturing: "0.0014",
      reaction: "0.0099",
      fetchedAt: "2026-08-03T10:00:00Z",
      expiresAt: "2026-08-03T11:00:00Z",
    });

    const user = userEvent.setup();
    render(<FacilitiesPage />);

    await user.click(await screen.findByRole("button", { name: "New Facility" }));
    await screen.findByRole("dialog", { name: "New Facility" });

    await user.selectOptions(screen.getByLabelText("Role"), "reaction");
    await user.type(screen.getByLabelText("Solar system"), "0-VG7A");
    await user.click(await screen.findByRole("button", { name: "0-VG7A" }));

    expect(await screen.findByDisplayValue("0.0099")).toBeInTheDocument();
    expect(screen.queryByDisplayValue("0.0014")).not.toBeInTheDocument();
  });

  test("a manufacturing profile card shows the Manufacturing badge", async () => {
    industryApi.listFacilities.mockResolvedValue([manufacturingProfile()]);
    render(<FacilitiesPage />);

    const card = (await screen.findByRole("heading", { name: "Sotiyo" })).closest("section")!;
    expect(within(card).getByText("Manufacturing")).toBeInTheDocument();
    expect(within(card).queryByText("Reaction")).not.toBeInTheDocument();
  });

  test("a reaction profile card shows the Reaction badge", async () => {
    industryApi.listFacilities.mockResolvedValue([reactionProfile({ name: "Alliance Tatara" })]);
    render(<FacilitiesPage />);

    const card = (await screen.findByRole("heading", { name: "Alliance Tatara" })).closest("section")!;
    expect(within(card).getByText("Reaction")).toBeInTheDocument();
    expect(within(card).queryByText("Manufacturing")).not.toBeInTheDocument();
  });

  test("card shows the effective structure+rig percentage, not the bare structure field", async () => {
    // Structure role bonus 1% + one 4.04% rig -> ~5% effective material.
    industryApi.listFacilities.mockResolvedValue([
      manufacturingProfile({
        materialReductionPercent: "1",
        timeReductionPercent: "0",
        rigs: [{
          slotNumber: 1,
          typeId: 43_920,
          typeName: "Standup M-Set Basic Medium Ship Manufacturing Material Efficiency I",
          materialReductionPercent: "4.04",
          timeReductionPercent: "0",
        }],
      }),
    ]);
    render(<FacilitiesPage />);

    const card = (await screen.findByRole("heading", { name: "Sotiyo" })).closest("section")!;
    expect(within(card).getByText("5%")).toBeInTheDocument();
    expect(within(card).queryByText("1%")).not.toBeInTheDocument();
  });

  test("card preserves meaningful bonus precision like 5.04%", async () => {
    industryApi.listFacilities.mockResolvedValue([
      manufacturingProfile({
        materialReductionPercent: "0",
        timeReductionPercent: "0",
        rigs: [{
          slotNumber: 1,
          typeId: 43_920,
          typeName: "Standup L-Set Efficiency",
          materialReductionPercent: "5.04",
          timeReductionPercent: "50.4",
        }],
      }),
    ]);
    render(<FacilitiesPage />);

    const card = (await screen.findByRole("heading", { name: "Sotiyo" })).closest("section")!;
    expect(within(card).getByText("5.04%")).toBeInTheDocument();
    expect(within(card).getByText("50.4%")).toBeInTheDocument();
  });

  test("Manufacturing filter hides reaction profiles; Reactions filter hides manufacturing profiles; All shows both", async () => {
    industryApi.listFacilities.mockResolvedValue([
      manufacturingProfile({ id: "m1", name: "Sotiyo Prime" }),
      reactionProfile({ id: "r1", name: "Tatara Prime" }),
    ]);
    const user = userEvent.setup();
    render(<FacilitiesPage />);

    expect(await screen.findByRole("heading", { name: "Sotiyo Prime" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Tatara Prime" })).toBeInTheDocument();

    await user.click(screen.getByRole("tab", { name: "Manufacturing" }));
    expect(screen.getByRole("heading", { name: "Sotiyo Prime" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Tatara Prime" })).not.toBeInTheDocument();

    await user.click(screen.getByRole("tab", { name: "Reactions" }));
    expect(screen.queryByRole("heading", { name: "Sotiyo Prime" })).not.toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Tatara Prime" })).toBeInTheDocument();

    await user.click(screen.getByRole("tab", { name: "All" }));
    expect(screen.getByRole("heading", { name: "Sotiyo Prime" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Tatara Prime" })).toBeInTheDocument();
  });

  test("search narrows by name and combines with the activity filter (AND)", async () => {
    industryApi.listFacilities.mockResolvedValue([
      manufacturingProfile({ id: "m1", name: "Azbel North", structureTypeName: "Azbel" }),
      manufacturingProfile({ id: "m2", name: "Sotiyo South", structureTypeName: "Sotiyo" }),
      reactionProfile({ id: "r1", name: "Azbel Reactor" }),
    ]);
    const user = userEvent.setup();
    render(<FacilitiesPage />);

    await screen.findByRole("heading", { name: "Azbel North" });
    await user.type(screen.getByRole("searchbox", { name: "Search facilities" }), "azbel");

    expect(screen.getByRole("heading", { name: "Azbel North" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Azbel Reactor" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Sotiyo South" })).not.toBeInTheDocument();

    await user.click(screen.getByRole("tab", { name: "Manufacturing" }));
    expect(screen.getByRole("heading", { name: "Azbel North" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Azbel Reactor" })).not.toBeInTheDocument();
  });

  test("filtered-empty state is distinct from the no-facilities state and offers a reset", async () => {
    industryApi.listFacilities.mockResolvedValue([manufacturingProfile({ name: "Sotiyo Prime" })]);
    const user = userEvent.setup();
    render(<FacilitiesPage />);

    await user.click(await screen.findByRole("tab", { name: "Reactions" }));

    expect(screen.getByText("No reaction facilities match these filters.")).toBeInTheDocument();
    expect(screen.queryByText("No Facility Profiles")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Reset filters" }));
    expect(screen.getByRole("heading", { name: "Sotiyo Prime" })).toBeInTheDocument();
  });

  test("the no-facilities empty state has no filter bar", async () => {
    industryApi.listFacilities.mockResolvedValue([]);
    render(<FacilitiesPage />);

    expect(await screen.findByText("No Facility Profiles")).toBeInTheDocument();
    expect(screen.queryByRole("tab", { name: "Manufacturing" })).not.toBeInTheDocument();
  });
});
