import { render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";

import type { FacilityProfile } from "../../../../api/industry";
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
