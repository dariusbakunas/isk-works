import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";

import type { FacilityInput } from "../../../../api/industry";
import { ImportFacilitiesModal } from "../import-facilities-modal";

const api = vi.hoisted(() => ({
  previewFacilityImport: vi.fn(),
  importFacilities: vi.fn(),
}));

vi.mock("../../../../api/industry", async () => {
  const actual = await vi.importActual<typeof import("../../../../api/industry")>("../../../../api/industry");
  return { ...actual, ...api };
});

const facility: FacilityInput = {
  name: "Alliance Sotiyo",
  kind: "upwellStructure",
  role: "manufacturing",
  structureId: 1_050_474_463_169,
  structureTypeId: 35_827,
  structureTypeName: "Sotiyo",
  solarSystemId: 30_002_099,
  solarSystemName: "C-J6MT",
  securityClass: "nullSec",
  materialReductionPercent: "1",
  timeReductionPercent: "30",
  jobCostReductionPercent: "5",
  facilityTaxPercent: "1",
  sccSurchargePercent: "4",
  allianceSurchargePercent: "0",
  fixedSupplementalCost: "0",
  manualSystemCostIndex: null,
  notes: "",
  rigs: [],
};

test("duplicate facilities default to skip and can be explicitly replaced", async () => {
  const user = userEvent.setup();
  api.previewFacilityImport.mockResolvedValue({
    items: [{
      index: 0,
      name: facility.name,
      classification: "duplicate",
      existingId: "facility-1",
      existingName: "Existing Sotiyo",
      existingRevision: 3,
      matchBasis: "eveLocation",
      message: null,
    }],
  });
  api.importFacilities.mockResolvedValue({ results: [{ name: facility.name, status: "replaced", message: null }] });
  render(<ImportFacilitiesModal onClose={vi.fn()} onImported={vi.fn()} />);

  await user.upload(
    screen.getByLabelText("Export file"),
    new File([JSON.stringify({ items: [facility] })], "facilities.json", { type: "application/json" }),
  );

  const importButton = await screen.findByRole("button", { name: "Import 0 facilities" });
  expect(importButton).toBeDisabled();
  await user.click(screen.getByRole("button", { name: "Replace" }));
  await user.click(screen.getByRole("button", { name: "Import 1 facility" }));

  await waitFor(() => expect(api.importFacilities).toHaveBeenCalledWith([{
    action: "replace",
    item: facility,
    existingId: "facility-1",
    expectedRevision: 3,
  }]));
});
