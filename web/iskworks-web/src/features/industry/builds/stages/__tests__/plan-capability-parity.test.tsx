// Worksheet capability parity. Every editable row-level Build capability is
// reachable from the Plan inspectors, in
// the owner the domain assigns it to: sourcing / scope / price override on
// the demand edge (root row), blueprint / ME / TE / facility on the one
// producer, root configuration in Build settings.

import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type {
  AcquisitionConsumerRef,
  AcquisitionLine,
  ExecutionConsumerRef,
  ExecutionNode,
  ExecutionOccurrence,
  ExecutionPlanProjection,
  PlannerPricingSelection,
  PreviewBuildPlanCommand,
  WorksheetItem,
} from "../../../../../api/industry";
import type { FacilityProfile } from "../../../../../api/industry/facilities";
import type { RowPricingSlice } from "../../../inspector/inspector-model";
import type { BuildWorksheetEditorModel } from "../../use-build-worksheet-editor";
import { rootRowEditing, type RootRowEditing } from "../root-row-editing";
import { StagesInspector, type StagesSelection } from "../stages-inspector";

const updateDescendantProductionConfiguration = vi.fn();
const listBlueprintObservations = vi.fn();
vi.mock("../../../../../api/industry", async (importOriginal) => ({
  ...(await importOriginal<object>()),
  updateDescendantProductionConfiguration: (...args: unknown[]) =>
    updateDescendantProductionConfiguration(...args),
  listBlueprintObservations: (...args: unknown[]) => listBlueprintObservations(...args),
}));

const COMMAND = {
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

const ALL_FACILITIES: FacilityProfile[] = [
  ...FACILITIES,
  { ...FACILITIES[0], id: "rx-a", name: "Reactor A", role: "reaction" } as FacilityProfile,
  { ...FACILITIES[1], id: "rx-b", name: "Reactor B", role: "reaction" } as FacilityProfile,
];

const ROOT = "root-build";
const PSE = 11_000; // Pulse Shield Emitter
const FERROGEL = 16_683;
const TRIT = 34;

function consumerEdge(nodeId: string, buildId: string, quantity: number): ExecutionConsumerRef {
  return {
    nodeId,
    occurrenceId: nodeId,
    quantity,
    buildId,
    dependencyId: `pd:${buildId}:${nodeId}`,
    fulfillmentScope: "missing",
    requiredQuantity: quantity,
    plannedInventoryQuantity: 0,
  };
}

function acqEdge(nodeId: string, buildId: string, quantity: number): AcquisitionConsumerRef {
  return {
    nodeId,
    occurrenceId: nodeId,
    quantity,
    buildId,
    dependencyId: `pd:${buildId}:${nodeId}`,
    fulfillmentScope: "missing",
    requiredQuantity: quantity,
    plannedInventoryQuantity: 0,
    freshCost: "5000.00",
    freshUnitPrice: "50.00",
  };
}

const rootOcc = occurrence({
  id: "build:root",
  nodeId: "root",
  buildId: ROOT,
  isRoot: true,
  stage: 2,
  outputTypeId: 12_005,
  outputTypeName: "Ishtar",
  blueprintOrFormulaName: "Ishtar Blueprint",
  facilityId: "fac-a",
  facilityName: "Facility A",
  effectiveMe: 2,
  effectiveTe: 4,
});
const pseOcc = occurrence({
  id: "build:pse",
  nodeId: "pse",
  buildId: "pse-build",
  outputTypeId: PSE,
  outputTypeName: "Pulse Shield Emitter",
  blueprintOrFormulaTypeId: 11_001,
  blueprintOrFormulaName: "Pulse Shield Emitter Blueprint",
  blueprintSelection: { mode: "manual", kind: "original", materialEfficiency: 10, timeEfficiency: 20, licensedRuns: null, notes: "" },
  facilityId: "fac-a",
  facilityName: "Facility A",
  effectiveMe: 10,
  effectiveTe: 20,
  projectedRuns: 3,
  requirements: [{
    typeId: FERROGEL,
    typeName: "Ferrogel",
    requiredQuantity: 20,
    plannedInventoryQuantity: 0,
    shortageQuantity: 20,
    fulfillmentScope: "missing",
    resolution: "reaction",
    dependencyId: "pd:pse-ferrogel",
    producerBuildId: "ferrogel-build",
    producerNodeId: "ferrogel",
  }],
});
const ferrogelOcc = occurrence({
  id: "build:ferrogel",
  nodeId: "ferrogel",
  buildId: "ferrogel-build",
  activity: "reaction",
  outputTypeId: FERROGEL,
  outputTypeName: "Ferrogel",
  blueprintOrFormulaTypeId: 166_831,
  blueprintOrFormulaName: "Ferrogel Reaction Formula",
  facilityId: null,
  facilityName: null,
});

const rootNode = node({
  id: "root",
  occurrenceIds: [rootOcc.id],
  stage: 2,
  outputTypeId: 12_005,
  outputTypeName: "Ishtar",
  facilityId: "fac-a",
  facilityName: "Facility A",
  effectiveMe: 2,
  effectiveTe: 4,
});
const pseNode = node({
  id: "pse",
  occurrenceIds: [pseOcc.id],
  outputTypeId: PSE,
  outputTypeName: "Pulse Shield Emitter",
  facilityId: "fac-a",
  facilityName: "Facility A",
  effectiveMe: 10,
  effectiveTe: 20,
  consumers: [consumerEdge("root", ROOT, 15)],
  productionMethods: [{ mode: "manufacturing", blueprintTypeId: 11_001 }],
});
// One canonical reaction producer serving two consumers (Muninn's
// Ferrogel): ONE configuration, two sourcing edges. No facility yet.
const ferrogelNode = node({
  id: "ferrogel",
  occurrenceIds: [ferrogelOcc.id],
  activity: "reaction",
  outputTypeId: FERROGEL,
  outputTypeName: "Ferrogel",
  productionDemand: 60,
  consumers: [consumerEdge("root", ROOT, 40), consumerEdge("pse", "pse-build", 20)],
  productionMethods: [{ mode: "reaction", reactionFormulaTypeId: 166_831 }],
});

const tritLine: AcquisitionLine = {
  typeId: TRIT,
  typeName: "Tritanium",
  requiredQuantity: 150,
  plannedInventoryQuantity: 0,
  shortageQuantity: 150,
  availableQuantity: 0,
  sourceStrategy: "buy",
  consumers: [acqEdge("root", ROOT, 100), acqEdge("pse", "pse-build", 50)],
  productionMethods: [],
  freshCost: "7500.00",
  freshUnitPrice: "50.00",
  freshPriceStale: false,
};

const PLAN: ExecutionPlanProjection = {
  rootNodeId: "root",
  stages: [
    { index: 0, nodeIds: ["ferrogel"] },
    { index: 1, nodeIds: ["pse"] },
    { index: 2, nodeIds: ["root"] },
  ],
  nodes: [ferrogelNode, pseNode, rootNode],
  edges: [],
  occurrences: [ferrogelOcc, pseOcc, rootOcc],
  acquisitions: [tritLine],
  unresolved: [],
  complete: false,
  warnings: [],
  generatedAt: "2026-09-24T00:00:00Z",
  logistics: { destinations: [], totalVolumeM3: "0", volumeComplete: true },
};

function slice(typeId: number, role: "material" | "output", mode: RowPricingSlice["mode"] = "default"): RowPricingSlice {
  return {
    kind: "row",
    mode,
    policy: "highestBuy",
    manualUnitPrice: mode === "manual" ? "42.00" : null,
    unitPrice: "50.00",
    allowPolicyOverride: role === "material",
    role,
    typeId,
    readOnly: false,
    summary: mode === "manual" ? "Manual price" : "Jita default",
  };
}

function editingStub(over: Partial<RootRowEditing> = {}): RootRowEditing {
  return {
    pricing: vi.fn((typeId: number, role: "material" | "output") => slice(typeId, role)),
    onPricingChange: vi.fn(),
    scopeOf: vi.fn(() => "missing" as const),
    onScopeChange: vi.fn(),
    ...over,
  };
}

function renderInspector(
  selection: StagesSelection,
  {
    plan = PLAN,
    rootEditing = editingStub(),
    onConfigurationSaved = vi.fn(),
    onOpenBuildSettings = vi.fn(),
  }: {
    plan?: ExecutionPlanProjection;
    rootEditing?: RootRowEditing | null;
    onConfigurationSaved?: () => void;
    onOpenBuildSettings?: () => void;
  } = {},
) {
  return render(
    <StagesInspector
      command={COMMAND}
      facilities={ALL_FACILITIES}
      onClose={vi.fn()}
      onSelect={vi.fn()}
      onConfigurationSaved={onConfigurationSaved}
      onOpenBuildSettings={onOpenBuildSettings}
      plan={plan}
      rootBuildId={ROOT}
      rootEditing={rootEditing}
      selection={selection}
      sourcing={{ pendingByEdge: {}, errorByEdge: {}, changeAll: vi.fn(), change: vi.fn() }}
    />,
  );
}

function section(label: string): HTMLElement {
  const pattern = label === "Production" ? "^Production(?!s| configuration)" : `^${label}`;
  const button = screen.getByRole("button", { name: new RegExp(pattern) });
  const found = button.closest("section");
  if (!found) throw new Error(`no section ${label}`);
  return found;
}

beforeEach(() => {
  updateDescendantProductionConfiguration.mockReset();
  updateDescendantProductionConfiguration.mockResolvedValue([]);
  listBlueprintObservations.mockReset();
  listBlueprintObservations.mockResolvedValue([]);
});

describe("Plan producer inspector -- production configuration (producer-owned)", () => {
  it("shows occurrence-owned direct requirements without replacing producer totals", () => {
    renderInspector({ kind: "production", nodeId: "pse" });
    const requirements = within(section("Requirements"));
    expect(requirements.getByText("Ferrogel")).toBeInTheDocument();
    expect(requirements.getAllByText("20")).toHaveLength(2);
    expect(requirements.getByText(/Total planned 60/)).toBeInTheDocument();
  });

  it("shows the manufacturing blueprint, ME and TE, and edits ME/TE through the descendant-configuration path", async () => {
    const onConfigurationSaved = vi.fn();
    renderInspector({ kind: "production", nodeId: "pse" }, { onConfigurationSaved });
    const config = within(section("Production configuration"));
    expect(config.getByText("Pulse Shield Emitter Blueprint")).toBeInTheDocument();
    expect(config.getByText("ME")).toBeInTheDocument();
    expect(config.getByText("TE")).toBeInTheDocument();

    const me = config.getByLabelText(/Material Efficiency/);
    fireEvent.change(me, { target: { value: "8" } });
    fireEvent.blur(me);
    await waitFor(() => expect(updateDescendantProductionConfiguration).toHaveBeenCalledTimes(1));
    const [rootBuildId, input] = updateDescendantProductionConfiguration.mock.calls[0];
    expect(rootBuildId).toBe(ROOT);
    expect(input.members).toEqual([{ buildId: "pse-build", expectedRevision: 1 }]);
    expect(input.request).toMatchObject({
      kind: "blueprintSelection",
      blueprintSelection: { mode: "manual", materialEfficiency: 8, timeEfficiency: 20 },
    });
    await waitFor(() => expect(onConfigurationSaved).toHaveBeenCalledTimes(1));

    const te = config.getByLabelText(/Time Efficiency/);
    fireEvent.change(te, { target: { value: "16" } });
    fireEvent.blur(te);
    await waitFor(() => expect(updateDescendantProductionConfiguration).toHaveBeenCalledTimes(2));
    expect(updateDescendantProductionConfiguration.mock.calls[1][1].request).toMatchObject({
      blueprintSelection: { timeEfficiency: 16 },
    });
  });

  it("edits the manufacturing facility (manufacturing profiles only) and re-plans", async () => {
    const onConfigurationSaved = vi.fn();
    renderInspector({ kind: "production", nodeId: "pse" }, { onConfigurationSaved });
    const select = within(section("Production configuration")).getByLabelText("Facility") as HTMLSelectElement;
    expect(select.value).toBe("fac-a");
    const options = [...select.options].map((option) => option.textContent);
    expect(options).toEqual(["No facility", "Facility A", "Facility B"]);
    fireEvent.change(select, { target: { value: "fac-b" } });
    await waitFor(() => expect(updateDescendantProductionConfiguration).toHaveBeenCalledTimes(1));
    expect(updateDescendantProductionConfiguration.mock.calls[0][1].request).toEqual({
      kind: "facility",
      facilityProfileId: "fac-b",
    });
    await waitFor(() => expect(onConfigurationSaved).toHaveBeenCalledTimes(1));
    // The inspector stays open after a save, editable again.
    await waitFor(() => expect(screen.queryByText("Saving...")).toBeNull());
    expect(select).not.toBeDisabled();
  });

  it("shows the reaction formula read-only and edits the reaction facility (reaction profiles only)", async () => {
    renderInspector({ kind: "production", nodeId: "ferrogel" });
    const config = within(section("Production configuration"));
    expect(config.getByText("Reaction formula")).toBeInTheDocument();
    expect(config.getByText("Ferrogel Reaction Formula")).toBeInTheDocument();
    // No blueprint/ME/TE editing for a reaction, and no formula chooser.
    expect(config.queryByLabelText(/Material Efficiency/)).toBeNull();
    expect(config.queryByText("ME")).toBeNull();
    const select = config.getByLabelText("Facility") as HTMLSelectElement;
    expect([...select.options].map((option) => option.textContent)).toEqual(["No facility", "Reactor A", "Reactor B"]);
    fireEvent.change(select, { target: { value: "rx-b" } });
    await waitFor(() =>
      expect(updateDescendantProductionConfiguration.mock.calls[0][1]).toMatchObject({
        members: [{ buildId: "ferrogel-build", expectedRevision: 1 }],
        request: { kind: "facility", facilityProfileId: "rx-b" },
      }),
    );
  });

  it("clearly surfaces a missing facility next to the field", () => {
    renderInspector({ kind: "production", nodeId: "ferrogel" });
    const config = within(section("Production configuration"));
    expect(config.getByText(/Facility not selected -- installation cost is incomplete/)).toBeInTheDocument();
  });

  it("a shared producer shows ONE production configuration, separate from per-consumer sourcing", () => {
    renderInspector({ kind: "production", nodeId: "ferrogel" });
    // Two demand edges, each with its own sourcing switch...
    const usedBy = within(section("Used by / Sourcing"));
    expect(usedBy.getAllByRole("radiogroup", { name: /Sourcing for/ })).toHaveLength(2);
    expect(usedBy.getByRole("radiogroup", { name: "Sourcing for Ishtar" })).toBeInTheDocument();
    expect(usedBy.getByRole("radiogroup", { name: "Sourcing for Pulse Shield Emitter" })).toBeInTheDocument();
    // ...no configuration on an edge...
    expect(usedBy.queryByLabelText("Facility")).toBeNull();
    // ...and exactly one configuration for the producer.
    expect(screen.getAllByRole("button", { name: /^Production configuration/ })).toHaveLength(1);
    expect(screen.getAllByLabelText("Facility")).toHaveLength(1);
    expect(within(section("Production configuration")).getByText(/applies to all 2 consumers/)).toBeInTheDocument();
  });

  it("per-consumer sourcing stays separate: one switch changes only its own edge", () => {
    const change = vi.fn();
    render(
      <StagesInspector
        command={COMMAND}
        facilities={ALL_FACILITIES}
        onClose={vi.fn()}
        onSelect={vi.fn()}
        onConfigurationSaved={vi.fn()}
        plan={PLAN}
        rootBuildId={ROOT}
        selection={{ kind: "production", nodeId: "ferrogel" }}
        sourcing={{ pendingByEdge: {}, errorByEdge: {}, changeAll: vi.fn(), change }}
      />,
    );
    fireEvent.click(
      within(screen.getByRole("radiogroup", { name: "Sourcing for Pulse Shield Emitter" })).getByRole("radio", {
        name: "Buy",
      }),
    );
    expect(change).toHaveBeenCalledTimes(1);
    expect(change).toHaveBeenCalledWith("pse-build", FERROGEL, null);
  });

  it("a produced operation carries no purchase-price override (production cost is authoritative)", () => {
    renderInspector({ kind: "production", nodeId: "pse" });
    expect(screen.queryByRole("button", { name: /^Pricing/ })).toBeNull();
    expect(screen.queryByRole("radio", { name: "Manual price" })).toBeNull();
    expect(screen.getByRole("button", { name: /^Economics/ })).toBeInTheDocument();
  });

  it("links to the producer's own Build for its row-level exceptions", () => {
    renderInspector({ kind: "production", nodeId: "pse" });
    expect(screen.getByRole("link", { name: /Open producer Build/ })).toHaveAttribute(
      "href",
      "/builds/root-build/producers/pse-build",
    );
  });

  it("the root still uses Edit build settings; its configuration is read-only here, with output pricing", () => {
    const onOpenBuildSettings = vi.fn();
    const rootEditing = editingStub();
    renderInspector({ kind: "production", nodeId: "root" }, { onOpenBuildSettings, rootEditing });
    fireEvent.click(screen.getByRole("button", { name: "Edit build settings" }));
    expect(onOpenBuildSettings).toHaveBeenCalledTimes(1);
    const config = within(section("Production configuration"));
    expect(config.getByText("Ishtar Blueprint")).toBeInTheDocument();
    expect(config.queryByLabelText("Facility")).toBeNull();
    expect(config.getByText(/Root configuration is edited in Build settings/)).toBeInTheDocument();
    expect(rootEditing.pricing).toHaveBeenCalledWith(12_005, "output");
    expect(within(section("Output pricing")).getByRole("radio", { name: "Manual price" })).toBeInTheDocument();
  });
});

describe("Plan input inspector -- Sourcing / Requirement / Pricing (demand-edge owned)", () => {
  it("has Sourcing, Requirement and Pricing sections in that order", () => {
    renderInspector({ kind: "acquisition", typeId: TRIT });
    const labels = screen
      .getAllByRole("button")
      .map((button) => button.textContent ?? "")
      .filter((text) => /^(Sourcing|Requirement|Pricing)/.test(text))
      .map((text) => text.match(/^(Sourcing|Requirement|Pricing)/)?.[1]);
    expect(labels).toEqual(["Sourcing", "Requirement", "Pricing"]);
    const requirement = within(section("Requirement"));
    expect(requirement.getByText("For Ishtar")).toBeInTheDocument();
    expect(requirement.getByText("For Pulse Shield Emitter")).toBeInTheDocument();
    expect(requirement.getByText("External shortage")).toBeInTheDocument();
    const pricing = within(section("Pricing"));
    expect(pricing.getByText("Unit price")).toBeInTheDocument();
    expect(pricing.getByText("Est. fresh cost")).toBeInTheDocument();
  });

  it("offers a price override for a root Buy input and resets it to the Price Source default", () => {
    const onPricingChange = vi.fn();
    const rootEditing = editingStub({
      pricing: vi.fn((typeId: number, role: "material" | "output") => slice(typeId, role, "manual")),
      onPricingChange,
    });
    renderInspector({ kind: "acquisition", typeId: TRIT }, { rootEditing });
    expect(rootEditing.pricing).toHaveBeenCalledWith(TRIT, "material");
    const pricing = within(section("Pricing"));
    expect(pricing.getByRole("radio", { name: "Manual price" })).toBeChecked();
    fireEvent.click(pricing.getByRole("radio", { name: "Price Source default" }));
    expect(onPricingChange).toHaveBeenCalledWith({
      typeId: TRIT,
      role: "material",
      selection: { kind: "default" },
    } satisfies PlannerPricingSelection);
  });

  it("a nested consumer's price is owned by its own Build -- linked, not overridden from the root", () => {
    renderInspector({ kind: "acquisition", typeId: TRIT });
    const pricing = within(section("Pricing"));
    expect(pricing.getByRole("link", { name: "Pulse Shield Emitter" })).toHaveAttribute(
      "href",
      "/builds/root-build/producers/pse-build",
    );
  });

  it("edits the root edge's fulfillment scope; a nested edge's scope is read-only", () => {
    const onScopeChange = vi.fn();
    renderInspector({ kind: "acquisition", typeId: TRIT }, { rootEditing: editingStub({ onScopeChange }) });
    const scope = screen.getByLabelText("Fulfillment scope for Ishtar");
    fireEvent.change(scope, { target: { value: "full" } });
    expect(onScopeChange).toHaveBeenCalledWith(TRIT, "full");
    expect(screen.queryByLabelText("Fulfillment scope for Pulse Shield Emitter")).toBeNull();
  });
});

describe("rootRowEditing -- the Worksheet's own row model, re-homed", () => {
  function row(typeId: number, role: "material" | "output", kind: "default" | "manual"): WorksheetItem {
    return {
      typeId,
      typeName: `Type ${typeId}`,
      role,
      pricing: {
        selectionKind: kind,
        effectivePolicy: "highestBuy",
        unitPrice: "42.00",
        manualUnitPrice: kind === "manual" ? "42.00" : null,
        missing: false,
        sourceNote: "",
      },
    } as unknown as WorksheetItem;
  }

  function editorWith(state: { prices: Record<number, string>; policies: Record<number, string>; scopes: Record<number, string> }) {
    const updater = <T,>(key: "prices" | "policies" | "scopes") => (next: T | ((current: T) => T)) => {
      const current = state[key] as unknown as T;
      state[key] = (typeof next === "function" ? (next as (c: T) => T)(current) : next) as never;
    };
    return {
      setPrices: updater("prices"),
      setItemPricingPolicies: updater("policies"),
      setFulfillmentScopes: updater("scopes"),
      fulfillmentScopes: state.scopes,
      source: { kind: "market", name: "Jita" },
      estimate: {
        worksheet: {
          groups: [{ key: "m", label: "Materials", items: [row(TRIT, "material", state.prices[TRIT] ? "manual" : "default")] }],
          output: { items: [row(12_005, "output", "default")] },
        },
      },
    } as unknown as BuildWorksheetEditorModel;
  }

  it("reads a root row's pricing and writes a manual price / reset through the Worksheet's own state", () => {
    const state = { prices: {} as Record<number, string>, policies: {}, scopes: {} };
    const editing = rootRowEditing(editorWith(state))!;
    expect(editing.pricing(TRIT, "material")).toMatchObject({ mode: "default", typeId: TRIT, role: "material" });
    expect(editing.pricing(12_005, "output")).toMatchObject({ role: "output", allowPolicyOverride: false });
    expect(editing.pricing(999, "material")).toBeNull();

    editing.onPricingChange({ typeId: TRIT, role: "material", selection: { kind: "manual", unit_price: "42.00" } });
    expect(state.prices).toEqual({ [TRIT]: "42.00" });
    expect(rootRowEditing(editorWith(state))!.pricing(TRIT, "material")).toMatchObject({ mode: "manual" });

    editing.onPricingChange({ typeId: TRIT, role: "material", selection: { kind: "default" } });
    expect(state.prices).toEqual({});
  });

  it("Buy -> Build keeps an override dormant; Build -> Buy restores it", () => {
    // The override is keyed by type in the root's pricing state, which a
    // sourcing change never touches: while the row is produced the backend
    // prices it by production cost (the override is ignored), and when it
    // is bought again the same override applies.
    const state = { prices: { [TRIT]: "42.00" } as Record<number, string>, policies: {}, scopes: {} };
    const editing = rootRowEditing(editorWith(state))!;
    // (A sourcing change writes componentResolutions only -- see
    // usePlanSourcing -- so the pricing state is unchanged by construction.)
    expect(editing.pricing(TRIT, "material")).toMatchObject({ mode: "manual", manualUnitPrice: "42.00" });
  });

  it("writes the root edge's fulfillment scope", () => {
    const state = { prices: {}, policies: {}, scopes: {} as Record<number, string> };
    const editing = rootRowEditing(editorWith(state))!;
    editing.onScopeChange(TRIT, "full");
    expect(state.scopes).toEqual({ [TRIT]: "full" });
    editing.onScopeChange(TRIT, "missing");
    expect(state.scopes).toEqual({});
  });
});
