import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, test, vi } from "vitest";

import type { FacilityProfile } from "../../../../api/industry";

const industryApi = vi.hoisted(() => ({
  updateFacility: vi.fn(),
  getSystemCostIndex: vi.fn(),
}));
vi.mock("../../../../api/industry", async () => {
  const actual = await vi.importActual<typeof import("../../../../api/industry")>("../../../../api/industry");
  return { ...actual, ...industryApi };
});

import { FacilityFormDialog } from "../facility-form-dialog";

const facility: FacilityProfile = {
  id: "facility-1",
  workspaceId: "workspace-1",
  name: "Alliance Tatara",
  kind: "manual",
  role: "reaction",
  structureId: null,
  structureTypeId: null,
  structureTypeName: "",
  solarSystemId: 30000142,
  solarSystemName: "Jita",
  securityClass: "highSec",
  materialReductionPercent: "0",
  timeReductionPercent: "0",
  jobCostReductionPercent: "0",
  facilityTaxPercent: "1",
  sccSurchargePercent: "4",
  allianceSurchargePercent: "0",
  fixedSupplementalCost: "0.0000",
  manualSystemCostIndex: "0.05",
  notes: "",
  rigs: [],
  archivedAt: null,
  revision: 2,
  createdAt: "2026-08-08T00:00:00Z",
  updatedAt: "2026-08-08T00:00:00Z",
};

async function openDialog() {
  const onSaved = vi.fn();
  render(<FacilityFormDialog facility={facility} onClose={vi.fn()} onSaved={onSaved} open />);
  await screen.findByRole("dialog", { name: "Edit Facility" });
  return { onSaved };
}

describe("FacilityFormDialog -- Fixed supplemental cost via MoneyInput", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    industryApi.getSystemCostIndex.mockResolvedValue({ manufacturing: "0.05", reaction: "0.05" });
    industryApi.updateFacility.mockImplementation(async (_id, _rev, input) => ({ ...facility, ...input }));
  });

  test("submits the canonical unformatted fixed cost on explicit Save", async () => {
    await openDialog();

    const field = screen.getByLabelText("Fixed supplemental cost");
    await userEvent.clear(field);
    await userEvent.type(field, "1,000,000");
    await userEvent.click(screen.getByRole("button", { name: "Save Changes" }));

    await waitFor(() => expect(industryApi.updateFacility).toHaveBeenCalled());
    expect(industryApi.updateFacility).toHaveBeenCalledWith(
      "facility-1",
      2,
      expect.objectContaining({ fixedSupplementalCost: "1000000" }),
    );
  });

  test("shows an inline error and never submits invalid fixed-cost text", async () => {
    await openDialog();

    const field = screen.getByLabelText("Fixed supplemental cost");
    await userEvent.clear(field);
    await userEvent.type(field, "1.234567");

    expect(await screen.findByRole("alert")).toHaveTextContent(/4 decimal places/i);

    await userEvent.click(screen.getByRole("button", { name: "Save Changes" }));
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(industryApi.updateFacility).not.toHaveBeenCalled();
  });

  test("treats a cleared fixed-cost field as zero, not an empty string", async () => {
    await openDialog();

    const field = screen.getByLabelText("Fixed supplemental cost");
    await userEvent.clear(field);
    field.blur();
    await userEvent.click(screen.getByRole("button", { name: "Save Changes" }));

    await waitFor(() => expect(industryApi.updateFacility).toHaveBeenCalled());
    const [, , input] = industryApi.updateFacility.mock.calls.at(-1)!;
    expect(input.fixedSupplementalCost).toBe("0");
  });

  test("does not disturb the percentage fields", async () => {
    await openDialog();

    expect(screen.getByLabelText("Facility tax %")).toHaveValue("1");
    expect(screen.getByLabelText("SCC surcharge %")).toHaveValue("4");
    // percentages are still plain text Fields, not MoneyInput
    await userEvent.clear(screen.getByLabelText("Facility tax %"));
    await userEvent.type(screen.getByLabelText("Facility tax %"), "2.5");
    expect(screen.getByLabelText("Facility tax %")).toHaveValue("2.5");
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });
});
