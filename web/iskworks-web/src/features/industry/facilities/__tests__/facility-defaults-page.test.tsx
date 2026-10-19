import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router";
import { beforeEach, describe, expect, test, vi } from "vitest";

import type { FacilityDefaultsView, FacilityProfile } from "../../../../api/industry";
import { FacilityDefaultsPage } from "../facility-defaults-page";

const industryApi = vi.hoisted(() => ({
  getFacilityDefaults: vi.fn(),
  assignFacilityDefaults: vi.fn(),
  listFacilities: vi.fn(),
}));

vi.mock("../../../../api/industry", async () => {
  const actual = await vi.importActual<typeof import("../../../../api/industry")>("../../../../api/industry");
  return { ...actual, ...industryApi };
});

function profile(id: string, name: string, role: FacilityProfile["role"]): FacilityProfile {
  return {
    id,
    workspaceId: "workspace-1",
    name,
    kind: "manual",
    role,
    structureId: null,
    structureTypeId: null,
    structureTypeName: "",
    solarSystemId: null,
    solarSystemName: "",
    securityClass: "highSec",
    materialReductionPercent: "1",
    timeReductionPercent: "15",
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
    createdAt: "2026-10-19T10:00:00Z",
    updatedAt: "2026-10-19T10:00:00Z",
  };
}

const raitaru = { facilityProfileId: "raitaru", facilityName: "Raitaru", materialBonusPercent: "1", timeBonusPercent: "15" };
const azbel = { facilityProfileId: "azbel", facilityName: "Azbel", materialBonusPercent: "2.98", timeBonusPercent: "32" };

function view(overrides: Partial<FacilityDefaultsView> = {}): FacilityDefaultsView {
  return {
    targets: [
      { role: "manufacturing", filterId: 3, name: "Ships", default: null, suggestion: null, status: "fallback" },
      { role: "manufacturing", filterId: 5, name: "Small T1 Ships", default: null, suggestion: azbel, status: "fallback" },
      { role: "manufacturing", filterId: 14, name: "Components", default: raitaru, suggestion: null, status: "coverageLost" },
      { role: "manufacturing", filterId: null, name: "Everything else", default: raitaru, suggestion: null, status: "set" },
      { role: "reaction", filterId: 16, name: "Hybrid Reactions", default: null, suggestion: null, status: "none" },
      { role: "reaction", filterId: null, name: "Everything else", default: null, suggestion: null, status: "none" },
    ],
    bonuses: [
      { facilityProfileId: "raitaru", role: "manufacturing", filterId: 3, materialBonusPercent: "1", timeBonusPercent: "15", covered: false },
      { facilityProfileId: "azbel", role: "manufacturing", filterId: 5, materialBonusPercent: "2.98", timeBonusPercent: "32", covered: true },
      { facilityProfileId: "raitaru", role: "manufacturing", filterId: 5, materialBonusPercent: "1", timeBonusPercent: "15", covered: false },
    ],
    ...overrides,
  };
}

function renderPage() {
  return render(
    <MemoryRouter initialEntries={["/facilities/defaults"]}>
      <FacilityDefaultsPage />
    </MemoryRouter>,
  );
}

function row(name: string) {
  return screen.getAllByRole("row").find((candidate) => within(candidate).queryByText(name, { exact: true }))!;
}

describe("FacilityDefaultsPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    industryApi.getFacilityDefaults.mockResolvedValue(view());
    industryApi.assignFacilityDefaults.mockResolvedValue(view());
    industryApi.listFacilities.mockResolvedValue([
      profile("raitaru", "Raitaru", "manufacturing"),
      profile("azbel", "Azbel", "manufacturing"),
      profile("tatara", "Tatara", "reaction"),
    ]);
  });

  test("lists targets per role with status, effective bonus and suggestions", async () => {
    renderPage();

    expect(await screen.findByText("Small T1 Ships")).toBeInTheDocument();
    expect(screen.getByText("Manufacturing")).toBeInTheDocument();
    expect(screen.getByText("Reactions")).toBeInTheDocument();
    const small = row("Small T1 Ships");
    expect(within(small).getByText("Uses fallback")).toBeInTheDocument();
    expect(within(small).getByText(/Suggested:/)).toHaveTextContent("Suggested: Azbel (2.98% / 32%)");
    // No own default: the fallback's facility and its bonus for this target.
    expect(within(small).getByText(/Via fallback/)).toHaveTextContent("Via fallback: Raitaru");
    expect(within(small).getByText("15%")).toBeInTheDocument();
    expect(within(row("Components")).getByText("Rigs no longer cover")).toBeInTheDocument();
    expect(within(row("Hybrid Reactions")).getByText("No default")).toBeInTheDocument();
  });

  test("facility options show each facility's bonus for that target and only the row's role", async () => {
    renderPage();

    const select = await screen.findByRole("combobox", { name: "Default facility for Small T1 Ships" });
    const options = within(select).getAllByRole("option").map((option) => option.textContent);
    expect(options).toEqual(["— Use fallback", "Raitaru (1% / 15%)", "Azbel (2.98% / 32%)"]);
    const reactionOptions = within(
      screen.getByRole("combobox", { name: "Default facility for Everything else (reactions)" }),
    ).getAllByRole("option");
    expect(reactionOptions.map((option) => option.textContent)).toEqual(["— No default", "Tatara"]);
  });

  test("changing a row's facility or clearing it sends one assignment", async () => {
    const user = userEvent.setup();
    renderPage();

    const select = await screen.findByRole("combobox", { name: "Default facility for Ships" });
    await user.selectOptions(select, "azbel");
    expect(industryApi.assignFacilityDefaults).toHaveBeenLastCalledWith([
      { role: "manufacturing", filterId: 3, facilityProfileId: "azbel" },
    ]);

    await user.selectOptions(
      screen.getByRole("combobox", { name: "Default facility for Everything else (manufacturing)" }),
      "",
    );
    expect(industryApi.assignFacilityDefaults).toHaveBeenLastCalledWith([
      { role: "manufacturing", filterId: null, facilityProfileId: null },
    ]);
  });

  test("Accept takes one suggestion", async () => {
    const user = userEvent.setup();
    industryApi.getFacilityDefaults.mockResolvedValue(
      view({
        targets: [
          { role: "manufacturing", filterId: 5, name: "Small T1 Ships", default: null, suggestion: azbel, status: "none" },
          { role: "manufacturing", filterId: null, name: "Everything else", default: null, suggestion: raitaru, status: "none" },
        ],
      }),
    );
    renderPage();

    await user.click(await screen.findByRole("button", { name: "Accept suggestion for Small T1 Ships" }));
    expect(industryApi.assignFacilityDefaults).toHaveBeenLastCalledWith([
      { role: "manufacturing", filterId: 5, facilityProfileId: "azbel" },
    ]);

  });

  test("Accept all lists every pending suggestion", async () => {
    const user = userEvent.setup();
    industryApi.getFacilityDefaults.mockResolvedValue(
      view({
        targets: [
          { role: "manufacturing", filterId: 5, name: "Small T1 Ships", default: null, suggestion: azbel, status: "none" },
          { role: "manufacturing", filterId: null, name: "Everything else", default: null, suggestion: raitaru, status: "none" },
        ],
      }),
    );
    renderPage();

    await user.click(await screen.findByRole("button", { name: "Accept all suggestions (2)" }));
    expect(industryApi.assignFacilityDefaults).toHaveBeenCalledWith([
      { role: "manufacturing", filterId: 5, facilityProfileId: "azbel" },
      { role: "manufacturing", filterId: null, facilityProfileId: "raitaru" },
    ]);
  });

  test("a failed save shows the error and keeps the page", async () => {
    const user = userEvent.setup();
    industryApi.assignFacilityDefaults.mockRejectedValue(new Error("default facility has a different role"));
    renderPage();

    await user.selectOptions(await screen.findByRole("combobox", { name: "Default facility for Ships" }), "azbel");

    expect(await screen.findByText("Facility defaults unavailable")).toBeInTheDocument();
    expect(screen.getByText("Small T1 Ships")).toBeInTheDocument();
  });
});
