import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { CharacterRosterEntry } from "../../../api/characters";
import type { Build, OrderSummary, Ticket, TicketPlanPreview } from "../../../api/industry";

const industryApi = vi.hoisted(() => ({
  createTicket: vi.fn(),
  previewTicketPlan: vi.fn(),
}));

vi.mock("../../../api/industry", async () => {
  const actual = await vi.importActual<typeof import("../../../api/industry")>("../../../api/industry");
  return { ...actual, ...industryApi };
});

const sdeApi = vi.hoisted(() => ({
  searchTypes: vi.fn(),
}));

vi.mock("../../../api/sde", async () => {
  const actual = await vi.importActual<typeof import("../../../api/sde")>("../../../api/sde");
  return { ...actual, ...sdeApi };
});

import { TicketEditor } from "../ticket-editor";

function orderFixture(overrides: Partial<OrderSummary> = {}): OrderSummary {
  return {
    id: "order-1",
    workspaceId: "workspace-1",
    ownerId: "owner-1",
    sourceBuildId: "build-1",
    sourceBuildRevision: 1,
    displayName: "Weekend Ishtar Production",
    runs: 1,
    recipeFingerprint: "fp",
    priceSnapshotId: "snapshot-1",
    estimatedMaterialCost: "1000.0000",
    expectedRevenue: null,
    estimatedMargin: null,
    missingPriceCount: 0,
    createdAt: "2026-08-21T00:00:00Z",
    updatedAt: "2026-08-21T00:00:00Z",
    startedAt: null,
    completedAt: null,
    canceledAt: null,
    archivedAt: null,
    status: "blocked",
    rollup: { satisfied: 0, needsAction: 1, inProgress: 0, total: 1 },
    ...overrides,
  };
}

function ticketFixture(overrides: Partial<Ticket> = {}): Ticket {
  return {
    id: "new-ticket",
    workspaceId: "workspace-1",
    ownerId: "owner-1",
    displayId: "ISK-3000",
    kind: "generic",
    typeId: null,
    capturedName: "Move blueprints to C-J6MT",
    quantity: null,
    orderId: null,
    notes: "",
    assigneeCharacterId: null,
    sourceBuildId: null,
    status: "todo",
    estimatedUnitCost: null,
    estimatedLineTotal: null,
    actualUnitCost: null,
    actualLineTotal: null,
    marketRegionId: null,
    marketLocationId: null,
    priceSourceId: null,
    acquisitionRunId: null,
    acquiredQuantity: null,
    executionSnapshot: null,
    createdAt: "2026-09-05T00:00:00Z",
    updatedAt: "2026-09-05T00:00:00Z",
    archivedAt: null,
    ...overrides,
  };
}

function buildFixture(overrides: Partial<Build> = {}): Build {
  return {
    id: "build-manufacturing-1",
    workspaceId: "workspace-1",
    ownerId: "owner-1",
    name: "Rifter",
    recipe: {
      kind: "manufacturing",
      sourceSdeDatasetId: "dataset-1",
      sourceSdeVersion: "test",
      blueprintTypeId: 6830,
      blueprintName: "Rifter Blueprint",
      durationSecondsPerRun: 600,
      materials: [{ typeId: 34, typeName: "Tritanium", quantityPerRun: 1000, sortOrder: 0 }],
      products: [{ typeId: 5876, typeName: "Rifter", quantityPerRun: 1, sortOrder: 0 }],
      fingerprint: "fp",
    },
    runs: 1,
    notes: "",
    revision: 1,
    createdAt: "2026-09-05T00:00:00Z",
    updatedAt: "2026-09-05T00:00:00Z",
    draftPlanning: null,
    recipeCurrency: "current",
    activeSdeVersion: null,
    productCategoryName: null,
    productGroupName: null,
    selectedBlueprintOrigin: null,
    hasOwnedBlueprint: false,
    ...overrides,
  };
}

function reactionBuildFixture(overrides: Partial<Build> = {}): Build {
  return buildFixture({
    id: "build-reaction-1",
    name: "Hull Section",
    recipe: {
      kind: "reaction",
      sourceSdeDatasetId: "dataset-1",
      sourceSdeVersion: "test",
      reactionFormulaTypeId: 90_003,
      reactionFormulaName: "Hull Section Reaction Formula",
      durationSecondsPerRun: 50,
      materials: [{ typeId: 35, typeName: "Pyerite", quantityPerRun: 50, sortOrder: 0 }],
      products: [{ typeId: 90_001, typeName: "Hull Section", quantityPerRun: 1, sortOrder: 0 }],
      fingerprint: "fp-reaction",
    },
    ...overrides,
  });
}

function previewFixture(overrides: Partial<TicketPlanPreview> = {}): TicketPlanPreview {
  return {
    kind: "manufacturing",
    buildId: "build-manufacturing-1",
    runs: 1,
    typeId: 5876,
    capturedName: "Rifter",
    quantity: 1,
    executionSnapshot: {
      runs: 1,
      blueprint: null,
      facility: null,
      durationSeconds: 600,
      installationCost: null,
      materialValue: "5000.0000",
    },
    prerequisites: [
      {
        typeId: 34,
        capturedName: "Tritanium",
        kind: "buy",
        requiredQuantity: 1000,
        estimatedUnitCost: null,
        estimatedLineTotal: null,
      },
    ],
    ...overrides,
  };
}

const characters: CharacterRosterEntry[] = [
  {
    connectionId: "char-1",
    eveCharacterId: 1001,
    characterName: "Alt One",
    corporationId: null,
    corporationName: null,
    securityStatus: null,
    solarSystemId: null,
    solarSystemName: null,
    walletBalance: null,
    totalSp: null,
    unallocatedSp: null,
    trainingQueue: [],
    trainingObservedAt: null,
    trainingQueueScopeMissing: false,
    manufacturingActiveJobs: null,
    manufacturingMaxJobs: null,
    reactionActiveJobs: null,
    reactionMaxJobs: null,
    researchActiveJobs: null,
    researchMaxJobs: null,
    connectionStatus: "connected",
    health: "healthy",
    lastSyncedAt: null,
  },
];

describe("TicketEditor", () => {
  beforeEach(() => vi.clearAllMocks());

  it("defaults to Generic, hides execution-specific fields, and creates a standalone ticket", async () => {
    industryApi.createTicket.mockResolvedValue(ticketFixture());
    const onCreated = vi.fn();
    render(
      <TicketEditor builds={[]} characters={[]} onClose={() => {}} onCreated={onCreated} orders={[]} />,
    );

    expect(screen.queryByLabelText("Search item")).not.toBeInTheDocument();
    expect(screen.queryByRole("spinbutton")).not.toBeInTheDocument();

    await userEvent.type(screen.getByPlaceholderText("e.g. Move blueprints to C-J6MT"), "Move blueprints to C-J6MT");
    await userEvent.click(screen.getByRole("button", { name: "Create ticket" }));

    await waitFor(() =>
      expect(industryApi.createTicket).toHaveBeenCalledWith({
        kind: "generic",
        capturedName: "Move blueprints to C-J6MT",
        notes: undefined,
        orderId: undefined,
        assigneeCharacterId: undefined,
      }),
    );
    expect(onCreated).toHaveBeenCalledWith(ticketFixture());
  });

  it("shows Item and Quantity fields once Acquisition is selected (no separate Title -- the item is the title), and requires an item", async () => {
    render(<TicketEditor builds={[]} characters={[]} onClose={() => {}} onCreated={() => {}} orders={[]} />);

    await userEvent.selectOptions(screen.getByRole("combobox", { name: "Type" }), "acquisition");

    expect(screen.getByLabelText("Search item")).toBeInTheDocument();
    expect(screen.getByRole("spinbutton")).toBeInTheDocument();
    expect(screen.queryByLabelText("Title")).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Create ticket" }));

    expect(await screen.findByText("Select an item to acquire.")).toBeInTheDocument();
  });

  it("creates an Acquisition ticket titled from the selected item -- explicit recording validates captured name against the SDE, never a free-form title", async () => {
    sdeApi.searchTypes.mockResolvedValue([{ typeId: 34, typeName: "Tritanium", groupName: null, published: true }]);
    industryApi.createTicket.mockResolvedValue(
      ticketFixture({ kind: "acquisition", typeId: 34, quantity: 10_000_000, capturedName: "Tritanium" }),
    );
    render(<TicketEditor builds={[]} characters={[]} onClose={() => {}} onCreated={() => {}} orders={[]} />);

    await userEvent.selectOptions(screen.getByRole("combobox", { name: "Type" }), "acquisition");
    await userEvent.type(screen.getByLabelText("Search item"), "trit");
    await userEvent.click(await screen.findByText("Tritanium"));
    await userEvent.type(screen.getByRole("spinbutton"), "10000000");
    await userEvent.click(screen.getByRole("button", { name: "Create ticket" }));

    await waitFor(() =>
      expect(industryApi.createTicket).toHaveBeenCalledWith(
        expect.objectContaining({
          kind: "acquisition",
          capturedName: "Tritanium",
          typeId: 34,
          quantity: 10_000_000,
        }),
      ),
    );
  });

  it("lists Board's already-loaded Epics and connected characters with no extra fetch", () => {
    render(
      <TicketEditor builds={[]}
        characters={characters}
        initialEpicId="order-1"
        onClose={() => {}}
        onCreated={() => {}}
        orders={[orderFixture()]}
      />,
    );

    expect(screen.getByLabelText("Epic")).toHaveValue("order-1");
    expect(screen.getByText("Weekend Ishtar Production")).toBeInTheDocument();
    expect(screen.getByLabelText("Assignee")).toHaveValue("");
    expect(screen.getByText("Alt One")).toBeInTheDocument();
  });

  it("keeps the Epic editable even when prefilled, and can be cleared before creating", async () => {
    industryApi.createTicket.mockResolvedValue(ticketFixture());
    render(
      <TicketEditor builds={[]} characters={[]} initialEpicId="order-1" onClose={() => {}} onCreated={() => {}} orders={[orderFixture()]} />,
    );

    await userEvent.selectOptions(screen.getByLabelText("Epic"), "");
    await userEvent.type(screen.getByPlaceholderText("e.g. Move blueprints to C-J6MT"), "Standalone work");
    await userEvent.click(screen.getByRole("button", { name: "Create ticket" }));

    await waitFor(() =>
      expect(industryApi.createTicket).toHaveBeenCalledWith(expect.objectContaining({ orderId: undefined })),
    );
  });

  it("rejects a blank title", async () => {
    render(<TicketEditor builds={[]} characters={[]} onClose={() => {}} onCreated={() => {}} orders={[]} />);

    await userEvent.click(screen.getByRole("button", { name: "Create ticket" }));

    expect(await screen.findByText("A title is required.")).toBeInTheDocument();
    expect(industryApi.createTicket).not.toHaveBeenCalled();
  });

  it("shows the Build/Runs fields (no item selector, no Title) for Manufacturing, defaults Runs to the Build's own, and creates a Build-backed ticket", async () => {
    industryApi.previewTicketPlan.mockResolvedValue(
      previewFixture({
        executionSnapshot: {
          runs: 1,
          blueprint: {
            id: "blueprint-snapshot-1",
            buildId: "build-manufacturing-1",
            sourceMode: "legacyMigration",
            blueprintTypeId: 6830,
            blueprintName: "Rifter Blueprint",
            kind: "original",
            materialEfficiency: 0,
            timeEfficiency: 0,
            licensedRuns: null,
            requestedRuns: 1,
            sourceObservationId: null,
            sourceEveItemId: null,
            sourceOwnerId: null,
            sourceOwnerName: null,
            sourceLocationId: null,
            sourceLocationName: null,
            observedAt: null,
            importedAt: null,
            manualNotes: null,
            plannedDurationSeconds: 600,
            formulaVersion: "legacy-blueprint-snapshot-v1",
            capturedAt: "2026-09-05T00:00:00Z",
          },
          facility: null,
          durationSeconds: 600,
          installationCost: null,
          materialValue: "5000.0000",
        },
      }),
    );
    industryApi.createTicket.mockResolvedValue(
      ticketFixture({ kind: "manufacturing", typeId: 5876, quantity: 1, capturedName: "Rifter", sourceBuildId: "build-manufacturing-1" }),
    );
    render(
      <TicketEditor builds={[buildFixture()]} characters={[]} onClose={() => {}} onCreated={() => {}} orders={[]} />,
    );

    await userEvent.selectOptions(screen.getByRole("combobox", { name: "Type" }), "manufacturing");

    expect(screen.queryByLabelText("Search item")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Title")).not.toBeInTheDocument();
    expect(screen.getByLabelText("Build")).toBeInTheDocument();

    await userEvent.selectOptions(screen.getByLabelText("Build"), "build-manufacturing-1");
    // Runs defaults to the selected Build's own runs.
    expect(screen.getByLabelText("Runs")).toHaveValue(1);

    await waitFor(() => expect(industryApi.previewTicketPlan).toHaveBeenCalledWith("build-manufacturing-1", 1));
    expect(await screen.findByText("Plan to freeze")).toBeInTheDocument();
    expect(screen.getByText("Rifter Blueprint")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Create ticket" }));

    await waitFor(() =>
      expect(industryApi.createTicket).toHaveBeenCalledWith({
        kind: "manufacturing",
        buildId: "build-manufacturing-1",
        runs: 1,
        notes: undefined,
        orderId: undefined,
        assigneeCharacterId: undefined,
      }),
    );
  });

  it("supports Reaction through the same canonical editor, filtering the Build picker to Reaction Builds only", async () => {
    industryApi.previewTicketPlan.mockResolvedValue(
      previewFixture({ kind: "reaction", buildId: "build-reaction-1", typeId: 90_001, capturedName: "Hull Section" }),
    );
    industryApi.createTicket.mockResolvedValue(ticketFixture({ kind: "reaction" }));
    render(
      <TicketEditor
        builds={[buildFixture(), reactionBuildFixture()]}
        characters={[]}
        onClose={() => {}}
        onCreated={() => {}}
        orders={[]}
      />,
    );

    await userEvent.selectOptions(screen.getByRole("combobox", { name: "Type" }), "reaction");

    // Only the Reaction Build is offered -- the Manufacturing one is not.
    expect(screen.getByRole("option", { name: "Hull Section" })).toBeInTheDocument();
    expect(screen.queryByRole("option", { name: "Rifter" })).not.toBeInTheDocument();

    await userEvent.selectOptions(screen.getByLabelText("Build"), "build-reaction-1");
    await waitFor(() => expect(industryApi.previewTicketPlan).toHaveBeenCalledWith("build-reaction-1", 1));
    expect(await screen.findByText("Plan to freeze")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Create ticket" }));
    await waitFor(() =>
      expect(industryApi.createTicket).toHaveBeenCalledWith(
        expect.objectContaining({ kind: "reaction", buildId: "build-reaction-1" }),
      ),
    );
  });

  it("filters the Manufacturing Build picker to exclude Reaction Builds", async () => {
    render(
      <TicketEditor
        builds={[buildFixture(), reactionBuildFixture()]}
        characters={[]}
        onClose={() => {}}
        onCreated={() => {}}
        orders={[]}
      />,
    );

    await userEvent.selectOptions(screen.getByRole("combobox", { name: "Type" }), "manufacturing");

    expect(screen.getByRole("option", { name: "Rifter" })).toBeInTheDocument();
    expect(screen.queryByRole("option", { name: "Hull Section" })).not.toBeInTheDocument();
  });

  it("refreshes the plan preview when Runs changes, via the server calculation -- never a client-side recalculation", async () => {
    industryApi.previewTicketPlan.mockResolvedValue(previewFixture());
    render(
      <TicketEditor builds={[buildFixture()]} characters={[]} onClose={() => {}} onCreated={() => {}} orders={[]} />,
    );

    await userEvent.selectOptions(screen.getByRole("combobox", { name: "Type" }), "manufacturing");
    await userEvent.selectOptions(screen.getByLabelText("Build"), "build-manufacturing-1");
    await waitFor(() => expect(industryApi.previewTicketPlan).toHaveBeenCalledWith("build-manufacturing-1", 1));

    industryApi.previewTicketPlan.mockResolvedValue(previewFixture({ runs: 5, quantity: 5 }));
    const runsInput = screen.getByLabelText("Runs");
    await userEvent.clear(runsInput);
    await userEvent.type(runsInput, "5");

    await waitFor(() => expect(industryApi.previewTicketPlan).toHaveBeenCalledWith("build-manufacturing-1", 5));
  });

  it("never renders ME/TE/facility/material editable controls for Manufacturing/Reaction -- the Build owns those, not this editor", async () => {
    industryApi.previewTicketPlan.mockResolvedValue(previewFixture());
    render(
      <TicketEditor builds={[buildFixture()]} characters={[]} onClose={() => {}} onCreated={() => {}} orders={[]} />,
    );

    await userEvent.selectOptions(screen.getByRole("combobox", { name: "Type" }), "manufacturing");
    await userEvent.selectOptions(screen.getByLabelText("Build"), "build-manufacturing-1");
    await screen.findByText("Plan to freeze");

    expect(screen.queryByLabelText(/material efficiency/i)).not.toBeInTheDocument();
    expect(screen.queryByLabelText(/^ME$/i)).not.toBeInTheDocument();
    expect(screen.queryByLabelText(/facility/i)).not.toBeInTheDocument();
    // Only Build/Runs/Epic/Assignee/Notes are editable controls (plus Type).
    expect(screen.getAllByRole("combobox").map((el) => el.getAttribute("id"))).toEqual([
      "ticket-editor-kind",
      "ticket-editor-build",
      "ticket-editor-epic",
      "ticket-editor-assignee",
    ]);
  });

  it("persists Epic and Assignee selections for a Manufacturing ticket the same as any other kind", async () => {
    industryApi.previewTicketPlan.mockResolvedValue(previewFixture());
    industryApi.createTicket.mockResolvedValue(ticketFixture({ kind: "manufacturing" }));
    render(
      <TicketEditor
        builds={[buildFixture()]}
        characters={characters}
        onClose={() => {}}
        onCreated={() => {}}
        orders={[orderFixture()]}
      />,
    );

    await userEvent.selectOptions(screen.getByRole("combobox", { name: "Type" }), "manufacturing");
    await userEvent.selectOptions(screen.getByLabelText("Build"), "build-manufacturing-1");
    await waitFor(() => expect(industryApi.previewTicketPlan).toHaveBeenCalled());
    await userEvent.selectOptions(screen.getByLabelText("Epic"), "order-1");
    await userEvent.selectOptions(screen.getByLabelText("Assignee"), "char-1");
    await userEvent.click(screen.getByRole("button", { name: "Create ticket" }));

    await waitFor(() =>
      expect(industryApi.createTicket).toHaveBeenCalledWith(
        expect.objectContaining({ orderId: "order-1", assigneeCharacterId: "char-1" }),
      ),
    );
  });

  it("requires a Build to be selected before creating a Manufacturing ticket", async () => {
    render(
      <TicketEditor builds={[buildFixture()]} characters={[]} onClose={() => {}} onCreated={() => {}} orders={[]} />,
    );

    await userEvent.selectOptions(screen.getByRole("combobox", { name: "Type" }), "manufacturing");
    await userEvent.click(screen.getByRole("button", { name: "Create ticket" }));

    expect(await screen.findByText("Select a Build.")).toBeInTheDocument();
    expect(industryApi.createTicket).not.toHaveBeenCalled();
  });

  it("keeps an Epic Inspector's prefilled Epic when the kind is switched to Manufacturing -- same editor, same prop, no special wiring", async () => {
    industryApi.previewTicketPlan.mockResolvedValue(previewFixture());
    industryApi.createTicket.mockResolvedValue(ticketFixture({ kind: "manufacturing" }));
    render(
      <TicketEditor
        builds={[buildFixture()]}
        characters={[]}
        initialEpicId="order-1"
        onClose={() => {}}
        onCreated={() => {}}
        orders={[orderFixture()]}
      />,
    );

    await userEvent.selectOptions(screen.getByRole("combobox", { name: "Type" }), "manufacturing");
    expect(screen.getByLabelText("Epic")).toHaveValue("order-1");

    await userEvent.selectOptions(screen.getByLabelText("Build"), "build-manufacturing-1");
    await waitFor(() => expect(industryApi.previewTicketPlan).toHaveBeenCalled());
    await userEvent.click(screen.getByRole("button", { name: "Create ticket" }));

    await waitFor(() =>
      expect(industryApi.createTicket).toHaveBeenCalledWith(expect.objectContaining({ orderId: "order-1" })),
    );
  });
});
