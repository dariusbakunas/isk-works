import { render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";

import type { FacilityProfile } from "../../../../api/industry";
import * as sdeApi from "../../../../api/sde";
import { FacilityFormDialog } from "../facility-form-dialog";

vi.mock("../../../../api/sde", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../../../api/sde")>()),
  getRigManufacturingModifiers: vi.fn(),
}));

const facility: FacilityProfile = {
  id: "facility-1",
  workspaceId: "workspace-1",
  name: "Alliance Tatara",
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
  facilityTaxPercent: "1",
  sccSurchargePercent: "4",
  allianceSurchargePercent: "0",
  fixedSupplementalCost: "0",
  manualSystemCostIndex: null,
  notes: "",
  rigs: [],
  archivedAt: null,
  revision: 2,
  createdAt: "2026-08-08T00:00:00Z",
  updatedAt: "2026-08-08T00:00:00Z",
};

test("hydrates edit state behind the facility form boundary", async () => {
  render(
    <FacilityFormDialog
      facility={facility}
      onClose={vi.fn()}
      onSaved={vi.fn()}
      open
    />,
  );

  expect(await screen.findByRole("dialog", { name: "Edit Facility" })).toBeInTheDocument();
  expect(screen.getByLabelText("Name")).toHaveValue("Alliance Tatara");
  expect(screen.getByLabelText("Role")).toHaveValue("reaction");
  expect(screen.getByRole("button", { name: "Save Changes" })).toBeInTheDocument();
});

test("a lab rig on a manufacturing profile captures its invention bonuses", async () => {
  vi.mocked(sdeApi.getRigManufacturingModifiers).mockResolvedValue({
    typeId: 43_878,
    materialReductionPercent: "0",
    timeReductionPercent: "0",
    compatibleWithStructure: true,
    appliesTo: { material: null, time: null },
    science: {
      inventionCostReductionPercent: "25.2",
      inventionTimeReductionPercent: "0",
      copyingCostReductionPercent: "0",
      copyingTimeReductionPercent: "0",
    },
  });
  render(
    <FacilityFormDialog
      facility={{
        ...facility,
        role: "manufacturing",
        securityClass: "nullSec",
        rigs: [{
          slotNumber: 1,
          typeId: 43_878,
          typeName: "Standup M-Set Invention Cost Optimization II",
          materialReductionPercent: "0",
          timeReductionPercent: "0",
        }],
      }}
      onClose={vi.fn()}
      onSaved={vi.fn()}
      open
    />,
  );

  expect(await screen.findByDisplayValue("25.2")).toBe(screen.getByLabelText("Invention cost reduction %"));
  expect(screen.getByLabelText("Copying time reduction %")).toHaveValue("0");
  expect(
    screen.getByText("Affects invention jobs: the system cost index part of the fee and the job time."),
  ).toBeInTheDocument();
  // A lab rig has no manufacturing bonus, so it claims no manufacturing jobs.
  expect(screen.queryByText(/Affects all manufacturing jobs/)).not.toBeInTheDocument();
});

test("a reaction profile's rigs have no invention or copying fields", async () => {
  render(
    <FacilityFormDialog
      facility={{
        ...facility,
        rigs: [{ slotNumber: 1, typeId: 0, typeName: "", materialReductionPercent: "0", timeReductionPercent: "0" }],
      }}
      onClose={vi.fn()}
      onSaved={vi.fn()}
      open
    />,
  );

  expect(await screen.findByText("Rig slot 1")).toBeInTheDocument();
  expect(screen.queryByLabelText("Invention cost reduction %")).not.toBeInTheDocument();
});
