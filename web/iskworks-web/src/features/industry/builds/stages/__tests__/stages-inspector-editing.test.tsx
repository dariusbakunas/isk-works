import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type {
  ExecutionNode,
  ExecutionOccurrence,
  ExecutionPlanProjection,
  PreviewBuildPlanCommand,
} from "../../../../../api/industry";
import { ApiError } from "../../../../../api/workspace";
import type { BlueprintObservation } from "../../../../../api/industry";
import type { FacilityProfile } from "../../../../../api/industry/facilities";
import { StagesInspector } from "../stages-inspector";

const updateDescendantProductionConfiguration = vi.fn();
const listBlueprintObservations = vi.fn();
vi.mock("../../../../../api/industry", async (importOriginal) => ({
  ...(await importOriginal<object>()),
  updateDescendantProductionConfiguration: (...args: unknown[]) =>
    updateDescendantProductionConfiguration(...args),
  listBlueprintObservations: (...args: unknown[]) => listBlueprintObservations(...args),
}));

const COMMAND: PreviewBuildPlanCommand = {
  recipe: { mode: "manufacturing", blueprintTypeId: 80_000 },
  runs: 1,
  componentResolutions: [],
} as unknown as PreviewBuildPlanCommand;

function occurrence(over: Partial<ExecutionOccurrence> & { id: string; nodeId: string; buildId: string }): ExecutionOccurrence {
  return {
    revision: 1,
    blueprintSelection: null,
    isRoot: false,
    stage: 0,
    activity: "manufacturing",
    outputTypeId: 1,
    outputTypeName: "Item",
    blueprintOrFormulaTypeId: 1,
    blueprintOrFormulaName: "Widget Blueprint",
    facilityId: null,
    facilityName: null,
    effectiveMe: null,
    effectiveTe: null,
    projectedRuns: 1,
    projectedOutput: 1,
    requiredQuantity: 0,
    plannedInventoryQuantity: 0,
    productionDemand: 0,
    retainedSurplusQuantity: 0,
    retainedSurplusCost: null,
    materialComponentCost: null,
    ownInstallationCost: null,
    totalProductionCost: null,
    unitProductionCost: null,
    costComplete: true,
    requirements: [],
    ...over,
  };
}

function node(over: Partial<ExecutionNode> & { id: string; occurrenceIds: string[] }): ExecutionNode {
  return {
    outputTypeId: 1,
    outputTypeName: "Item",
    activity: "manufacturing",
    stage: 0,
    facilityId: null,
    facilityName: null,
    effectiveMe: null,
    effectiveTe: null,
    requiredQuantity: 0,
    plannedInventoryQuantity: 0,
    productionDemand: 0,
    projectedOutput: 0,
    projectedRuns: 0,
    retainedSurplusQuantity: 0,
    retainedSurplusCost: null,
    materialComponentCost: null,
    ownInstallationCost: null,
    totalProductionCost: null,
    costComplete: true,
    consumers: [],
    productionMethods: [],
    unitProductionCost: null,
    availableQuantity: 0,
    ...over,
  };
}

const ROOT_OCCURRENCE = occurrence({
  id: "root:1",
  nodeId: "node-root",
  buildId: "root-build",
  isRoot: true,
  stage: 2,
  facilityId: "fac-a",
  facilityName: "Facility A",
});
const ROOT_NODE = node({
  id: "node-root",
  occurrenceIds: [ROOT_OCCURRENCE.id],
  stage: 2,
  facilityId: "fac-a",
  facilityName: "Facility A",
  outputTypeName: "Root Product",
});

const X_OCCURRENCE = occurrence({
  id: "build:x",
  nodeId: "node-x",
  buildId: "x-build",
  revision: 3,
  facilityId: "fac-a",
  facilityName: "Facility A",
  outputTypeName: "Widget",
});
const X_NODE = node({
  id: "node-x",
  occurrenceIds: [X_OCCURRENCE.id],
  facilityId: "fac-a",
  facilityName: "Facility A",
  outputTypeName: "Widget",
});

const Y1_OCCURRENCE = occurrence({
  id: "build:y1",
  nodeId: "node-y",
  buildId: "y1-build",
  revision: 2,
  facilityId: "fac-a",
  facilityName: "Facility A",
  outputTypeName: "Fernite Carbide",
});
const Y2_OCCURRENCE = occurrence({
  id: "build:y2",
  nodeId: "node-y",
  buildId: "y2-build",
  revision: 5,
  facilityId: "fac-a",
  facilityName: "Facility A",
  outputTypeName: "Fernite Carbide",
});
const Y_NODE = node({
  id: "node-y",
  occurrenceIds: [Y1_OCCURRENCE.id, Y2_OCCURRENCE.id],
  facilityId: "fac-a",
  facilityName: "Facility A",
  outputTypeName: "Fernite Carbide",
  activity: "reaction",
});

// A shared MANUFACTURING operation (Y is a shared Reaction, used by the
// facility tests above) -- for blueprint/ME/TE editing, which only applies
// to manufacturing.
const SEEDED_SELECTION = {
  mode: "manual" as const,
  kind: "original" as const,
  materialEfficiency: 5,
  timeEfficiency: 10,
  licensedRuns: null,
  notes: "",
};
const Z1_OCCURRENCE = occurrence({
  id: "build:z1",
  nodeId: "node-z",
  buildId: "z1-build",
  revision: 3,
  facilityId: "fac-a",
  facilityName: "Facility A",
  outputTypeName: "Composite",
  blueprintOrFormulaTypeId: 900,
  blueprintOrFormulaName: "Widget Blueprint",
  blueprintSelection: SEEDED_SELECTION,
});
const Z2_OCCURRENCE = occurrence({
  id: "build:z2",
  nodeId: "node-z",
  buildId: "z2-build",
  revision: 7,
  facilityId: "fac-a",
  facilityName: "Facility A",
  outputTypeName: "Composite",
  blueprintOrFormulaTypeId: 900,
  blueprintOrFormulaName: "Widget Blueprint",
  blueprintSelection: SEEDED_SELECTION,
});
const Z_NODE = node({
  id: "node-z",
  occurrenceIds: [Z1_OCCURRENCE.id, Z2_OCCURRENCE.id],
  facilityId: "fac-a",
  facilityName: "Facility A",
  outputTypeName: "Composite",
  activity: "manufacturing",
});

const PLAN: ExecutionPlanProjection = {
  rootNodeId: "node-root",
  stages: [
    { index: 0, nodeIds: ["node-x", "node-y", "node-z"] },
    { index: 2, nodeIds: ["node-root"] },
  ],
  nodes: [X_NODE, Y_NODE, Z_NODE, ROOT_NODE],
  edges: [],
  occurrences: [
    X_OCCURRENCE,
    Y1_OCCURRENCE,
    Y2_OCCURRENCE,
    Z1_OCCURRENCE,
    Z2_OCCURRENCE,
    ROOT_OCCURRENCE,
  ],
  acquisitions: [],
  unresolved: [],
  complete: true,
  warnings: [],
  generatedAt: new Date().toISOString(),
  logistics: { destinations: [], totalVolumeM3: "0", volumeComplete: true },
};

const SOURCING = { pendingByEdge: {}, errorByEdge: {}, changeAll: vi.fn(), change: vi.fn() };

const FACILITIES: FacilityProfile[] = [
  {
    id: "fac-a",
    workspaceId: "ws-1",
    name: "Facility A",
    kind: "manual",
    role: "manufacturing",
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
    sccSurchargePercent: "0",
    allianceSurchargePercent: "0",
    fixedSupplementalCost: "0",
    manualSystemCostIndex: null,
    notes: "",
    rigs: [],
    archivedAt: null,
    revision: 1,
    createdAt: "",
    updatedAt: "",
  } as unknown as FacilityProfile,
  {
    id: "fac-b",
    workspaceId: "ws-1",
    name: "Facility B",
    kind: "manual",
    role: "manufacturing",
    structureId: null,
    structureTypeId: null,
    structureTypeName: "",
    solarSystemId: null,
    solarSystemName: "",
    securityClass: "unknown",
    materialReductionPercent: "10",
    timeReductionPercent: "0",
    jobCostReductionPercent: "0",
    facilityTaxPercent: "0",
    sccSurchargePercent: "0",
    allianceSurchargePercent: "0",
    fixedSupplementalCost: "0",
    manualSystemCostIndex: null,
    notes: "",
    rigs: [],
    archivedAt: null,
    revision: 1,
    createdAt: "",
    updatedAt: "",
  } as unknown as FacilityProfile,
];

beforeEach(() => {
  updateDescendantProductionConfiguration.mockReset();
  listBlueprintObservations.mockReset();
  listBlueprintObservations.mockResolvedValue([]);
});
afterEach(() => {
  vi.restoreAllMocks();
});

describe("Stages descendant production configuration editing", () => {
  it("exposes an editable facility control for a non-shared descendant production node", () => {
    render(
      <StagesInspector
        command={COMMAND}
        facilities={FACILITIES}
        onClose={vi.fn()}
        onSelect={vi.fn()}
        onConfigurationSaved={vi.fn()}
        plan={PLAN}
        rootBuildId="root-build"
        selection={{ kind: "production", nodeId: "node-x" }}
        sourcing={SOURCING}
      />,
    );
    const select = screen.getByLabelText("Facility") as HTMLSelectElement;
    expect(select.value).toBe("fac-a");
    expect(select.disabled).toBe(false);
  });

  it("does not duplicate editable root configuration in Stages -- Final Production is read-only", () => {
    render(
      <StagesInspector
        command={COMMAND}
        facilities={FACILITIES}
        onClose={vi.fn()}
        onSelect={vi.fn()}
        onConfigurationSaved={vi.fn()}
        plan={PLAN}
        rootBuildId="root-build"
        selection={{ kind: "production", nodeId: "node-root" }}
        sourcing={SOURCING}
      />,
    );
    expect(screen.queryByLabelText("Facility")).toBeNull();
    expect(screen.getByText("Facility A")).toBeInTheDocument();
    expect(screen.getByText(/Root configuration is edited in Build settings/)).toBeInTheDocument();
  });

  it("calls the canonical mutation for a non-shared edit and refetches on success", async () => {
    updateDescendantProductionConfiguration.mockResolvedValue([]);
    const onConfigurationSaved = vi.fn();
    render(
      <StagesInspector
        command={COMMAND}
        facilities={FACILITIES}
        onClose={vi.fn()}
        onSelect={vi.fn()}
        onConfigurationSaved={onConfigurationSaved}
        plan={PLAN}
        rootBuildId="root-build"
        selection={{ kind: "production", nodeId: "node-x" }}
        sourcing={SOURCING}
      />,
    );
    fireEvent.change(screen.getByLabelText("Facility"), { target: { value: "fac-b" } });

    await waitFor(() => expect(updateDescendantProductionConfiguration).toHaveBeenCalledTimes(1));
    const [, input] = updateDescendantProductionConfiguration.mock.calls[0];
    expect(input.members).toEqual([{ buildId: "x-build", expectedRevision: 3 }]);
    await waitFor(() => expect(onConfigurationSaved).toHaveBeenCalledTimes(1));
  });

  it("edits a non-shared Manufacturing operation's TE in one request for its one member", async () => {
    updateDescendantProductionConfiguration.mockResolvedValue([]);
    const onConfigurationSaved = vi.fn();
    render(
      <StagesInspector
        command={COMMAND}
        facilities={FACILITIES}
        onClose={vi.fn()}
        onSelect={vi.fn()}
        onConfigurationSaved={onConfigurationSaved}
        plan={PLAN}
        rootBuildId="root-build"
        selection={{ kind: "production", nodeId: "node-x" }}
        sourcing={SOURCING}
      />,
    );
    // node-x has no seeded blueprintSelection -- the manual fields must
    // still render, seeded from the unresearched default (ME/TE 0).
    const te = screen.getByLabelText("Time Efficiency (0-20)");
    expect(screen.getByLabelText("Material Efficiency (0-10)")).toHaveValue("0");
    fireEvent.change(te, { target: { value: "14" } });
    fireEvent.blur(te);

    await waitFor(() => expect(updateDescendantProductionConfiguration).toHaveBeenCalledTimes(1));
    const [, input] = updateDescendantProductionConfiguration.mock.calls[0];
    expect(input.members).toEqual([{ buildId: "x-build", expectedRevision: 3 }]);
    expect(input.request).toEqual({
      kind: "blueprintSelection",
      blueprintSelection: {
        mode: "manual",
        kind: "original",
        materialEfficiency: 0,
        timeEfficiency: 14,
        licensedRuns: null,
        notes: "",
      },
    });
    await waitFor(() => expect(onConfigurationSaved).toHaveBeenCalledTimes(1));
  });

  it("never renders runs/output/surplus as editable inputs -- only Facility and Blueprint kind/ME/TE", () => {
    render(
      <StagesInspector
        command={COMMAND}
        facilities={FACILITIES}
        onClose={vi.fn()}
        onSelect={vi.fn()}
        onConfigurationSaved={vi.fn()}
        plan={PLAN}
        rootBuildId="root-build"
        selection={{ kind: "production", nodeId: "node-x" }}
        sourcing={SOURCING}
      />,
    );
    // No numeric spinner control of any kind -- ME/TE are legitimate
    // `type="text"` fields (see `Field`), never a `type="number"` input
    // that could be mistaken for (or accidentally wired to) a derived
    // value like runs.
    expect(screen.queryAllByRole("spinbutton")).toHaveLength(0);
    // Exactly the two legitimate selects: Facility and Blueprint kind.
    // Runs/output/surplus/production-demand have no form control at all.
    expect(screen.getByRole("combobox", { name: "Facility" })).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Blueprint kind" })).toBeInTheDocument();
    expect(screen.getAllByRole("combobox")).toHaveLength(2);
    // ME/TE are legitimately editable text fields...
    expect(screen.getByLabelText("Material Efficiency (0-10)")).toBeInTheDocument();
    expect(screen.getByLabelText("Time Efficiency (0-20)")).toBeInTheDocument();
    // ...but nothing derived ever gets its own input/select.
    for (const forbidden of [
      "Runs",
      "Projected output",
      "Retained surplus",
      "Production demand",
      "Required",
    ]) {
      expect(screen.queryByRole("textbox", { name: forbidden })).toBeNull();
      expect(screen.queryByRole("spinbutton", { name: forbidden })).toBeNull();
      expect(screen.queryByRole("combobox", { name: forbidden })).toBeNull();
    }
  });

  it("shows a curated error and never applies a fake local value on a stale/conflict response", async () => {
    updateDescendantProductionConfiguration.mockRejectedValue(
      new ApiError(409, {
        code: "descendant_operation_membership_stale",
        message: "This production operation has changed since it was loaded.",
      }),
    );
    const onConfigurationSaved = vi.fn();
    render(
      <StagesInspector
        command={COMMAND}
        facilities={FACILITIES}
        onClose={vi.fn()}
        onSelect={vi.fn()}
        onConfigurationSaved={onConfigurationSaved}
        plan={PLAN}
        rootBuildId="root-build"
        selection={{ kind: "production", nodeId: "node-x" }}
        sourcing={SOURCING}
      />,
    );
    const select = screen.getByLabelText("Facility") as HTMLSelectElement;
    fireEvent.change(select, { target: { value: "fac-b" } });

    await waitFor(() =>
      expect(screen.getByText(/changed since it was loaded/)).toBeInTheDocument(),
    );
    expect(onConfigurationSaved).not.toHaveBeenCalled();
    // The select must not have optimistically adopted the rejected value --
    // it still reflects the plan's own last-known facility.
    expect(select.value).toBe("fac-a");
  });
  it("re-seeds the chosen observed blueprint when the plan's observation changes in the same mode", async () => {
    const observation = (id: string, locationName: string) =>
      ({
        id,
        ownerName: "Owner",
        blueprintTypeId: 900,
        blueprintName: "Widget Blueprint",
        kind: "original",
        materialEfficiency: 10,
        timeEfficiency: 20,
        licensedRuns: null,
        locationId: 1,
        locationName,
        observedAt: "2026-10-01T00:00:00Z",
      }) as unknown as BlueprintObservation;
    listBlueprintObservations.mockResolvedValue([
      observation("obs-1", "Hangar One"),
      observation("obs-2", "Hangar Two"),
    ]);
    const planWith = (observationId: string): ExecutionPlanProjection => ({
      ...PLAN,
      occurrences: PLAN.occurrences.map((member) =>
        member.nodeId === "node-z"
          ? { ...member, blueprintSelection: { mode: "observedAsset", observationId } }
          : member,
      ),
    });
    const props = {
      command: COMMAND,
      facilities: FACILITIES,
      onClose: vi.fn(),
      onSelect: vi.fn(),
      onConfigurationSaved: vi.fn(),
      rootBuildId: "root-build",
      selection: { kind: "production", nodeId: "node-z" } as const,
      sourcing: SOURCING,
    };
    const { rerender } = render(<StagesInspector {...props} plan={planWith("obs-1")} />);
    const picker = await screen.findByRole("combobox", { name: "Available blueprint" });
    await waitFor(() => expect(picker).toHaveTextContent("Hangar One"));

    rerender(<StagesInspector {...props} plan={planWith("obs-2")} />);

    await waitFor(() => expect(picker).toHaveTextContent("Hangar Two"));
  });
});
