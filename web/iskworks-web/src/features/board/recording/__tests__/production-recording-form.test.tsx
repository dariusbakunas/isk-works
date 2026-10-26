import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { TaskExecutionSnapshot, TicketSummary } from "../../../../api/industry";
import { ProductionRecordingForm } from "../production-recording-form";

const api = vi.hoisted(() => ({
  previewTicketProduction: vi.fn(),
  recordTicketProduction: vi.fn(),
  recordTicketAcquisition: vi.fn(),
  startTicket: vi.fn(),
  completeTicket: vi.fn(),
  updateTicketStatus: vi.fn(),
}));

vi.mock("../../../../api/industry", async () => {
  const actual =
    await vi.importActual<typeof import("../../../../api/industry")>("../../../../api/industry");
  return { ...actual, ...api };
});

function snapshot(overrides: Partial<TaskExecutionSnapshot> = {}): TaskExecutionSnapshot {
  return {
    runs: 100,
    blueprint: null,
    facility: {
      id: "facility-1",
      workspaceId: "workspace-1",
      name: "Sotiyo — Assembly",
      kind: "upwellStructure",
      role: "manufacturing",
      structureId: 1,
      structureTypeId: 35827,
      structureTypeName: "Sotiyo",
      solarSystemId: 30000142,
      solarSystemName: "Jita",
      securityClass: "highSec",
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
      createdAt: "2026-09-01T00:00:00Z",
      updatedAt: "2026-09-01T00:00:00Z",
    },
    durationSeconds: 7200,
    installationCost: {
      complete: true,
      estimatedItemValue: "5000000.0000",
      systemCostIndex: "0.05",
      unmodifiedSystemIndexCost: "1000.0000",
      jobCostReductionPercent: "0",
      systemIndexCost: "1000.0000",
      facilityTax: null,
      sccSurcharge: null,
      allianceSurcharge: null,
      fixedSupplementalCost: "0",
      total: "1000.0000",
      warnings: [],
      formulaVersion: "test",
    },
    materialValue: "40000.0000",
    ...overrides,
  };
}

function ticket(overrides: Partial<TicketSummary> = {}): TicketSummary {
  return {
    id: "ticket-1",
    workspaceId: "workspace-1",
    ownerId: "owner-1",
    displayId: "ISK-3000",
    kind: "manufacturing",
    typeId: 20185,
    capturedName: "Crystalline Carbonide Armor Plate",
    quantity: 500,
    orderId: null,
    notes: "",
    assigneeCharacterId: null,
    sourceBuildId: "build-1",
    status: "inProgress",
    estimatedUnitCost: "100.0000",
    estimatedLineTotal: "50000.0000",
    actualUnitCost: null,
    actualLineTotal: null,
    marketRegionId: null,
    marketLocationId: null,
    priceSourceId: null,
    acquisitionRunId: null,
    acquiredQuantity: null,
    executionSnapshot: snapshot(),
    createdAt: "2026-09-01T00:00:00Z",
    updatedAt: "2026-09-01T00:00:00Z",
    completedAt: null,
    archivedAt: null,
    blockedBy: [],
    prerequisites: [
      {
        id: "prereq-1",
        ticketId: "ticket-1",
        typeId: 34,
        capturedName: "Tritanium",
        kind: "buy",
        sourceBuildId: null,
        requiredQuantity: 1000,
        fulfillmentScope: "full",
        reusedQuantity: 0,
        freshQuantity: 1000,
        estimatedUnitCost: "5.0000",
        estimatedLineTotal: "5000.0000",
        reusedLineTotal: null,
      },
    ],
    recording: {
      state: "notRecorded",
      requestedQuantity: 100,
      recordedQuantity: 0,
      remainingQuantity: 100,
      surplusQuantity: 0,
    },
    ...overrides,
  };
}

function balance(typeId: number, quantity: number, total: string, average: string | null) {
  return {
    key: { workspaceId: "workspace-1", ownerId: "owner-1", typeId },
    typeName: "",
    quantity,
    totalHistoricalCost: total,
    averageUnitCost: average,
    revision: 1,
    lastActivityAt: null,
  };
}

const PREVIEW = {
  lines: [
    {
      typeId: 20185,
      typeName: "Crystalline Carbonide Armor Plate",
      effect: "added",
      current: balance(20185, 0, "0", null),
      resulting: balance(20185, 500, "50000.0000", "100.0000"),
    },
    {
      typeId: 34,
      typeName: "Tritanium",
      effect: "consumed",
      current: balance(34, 2_000, "10000.0000", "5.0000"),
      resulting: balance(34, 1_000, "5000.0000", "5.0000"),
    },
  ],
};

async function previewThenRecord(user = userEvent) {
  await user.click(screen.getByRole("button", { name: "Preview" }));
  await user.click(await screen.findByRole("button", { name: "Record" }));
}

const runsInput = () => screen.getByRole("spinbutton", { name: "Runs completed" });
const outputInput = () =>
  screen.getByRole("spinbutton", { name: /Output quantity for/ });
const materialInput = () =>
  screen.getByRole("spinbutton", { name: "Consumed quantity for Tritanium" });
const installInput = () => screen.getByRole("textbox", { name: "Installation cost (ISK)" });

describe("ProductionRecordingForm — snapshot-based defaults", () => {
  beforeEach(() => vi.clearAllMocks());

  it("shows Record production and defaults runs to the remaining runs", () => {
    render(<ProductionRecordingForm onCancel={() => {}} onRecorded={() => {}} ticket={ticket()} />);
    expect(screen.getByRole("form", { name: "Record production" })).toBeInTheDocument();
    expect(runsInput()).toHaveValue(100);
  });

  it("prefills output, materials and installation cost from the frozen snapshot", () => {
    render(<ProductionRecordingForm onCancel={() => {}} onRecorded={() => {}} ticket={ticket()} />);
    // 500 planned output / 100 planned runs -> 100 runs = 500.
    expect(outputInput()).toHaveValue(500);
    // 1000 planned material / 100 planned runs -> 100 runs = 1000.
    expect(materialInput()).toHaveValue(1000);
    // 1000 ISK planned install / 100 planned runs -> 100 runs = 1,000.
    expect(installInput()).toHaveValue("1,000");
  });

  it("prefills the installation cost from the linked ESI jobs' actual cost", async () => {
    const linkedJob = (jobId: number, runs: number, cost: string | null) => ({
      jobId,
      activity: "manufacturing" as const,
      activityId: 1,
      status: "delivered",
      blueprintTypeId: 1000,
      blueprintName: null,
      productTypeId: 20185,
      productName: null,
      runs,
      facilityId: 1,
      facilityName: null,
      solarSystemName: null,
      startDate: "2026-10-13T00:00:00Z",
      endDate: "2026-10-13T10:00:00Z",
      cost,
    });
    render(
      <ProductionRecordingForm
        jobs={[linkedJob(1, 60, "1234.5600"), linkedJob(2, 40, "823.0400")]}
        onCancel={() => {}}
        onRecorded={() => {}}
        ticket={ticket()}
      />,
    );

    // 2,057.60 ISK over 100 runs, recording all 100.
    expect(installInput()).toHaveValue("2,057.6");
    expect(screen.getByTestId("install-cost-source")).toHaveTextContent("From 2 ESI jobs");

    // Recording 40 runs takes 40 runs' share.
    await userEvent.clear(runsInput());
    await userEvent.type(runsInput(), "40");
    expect(installInput()).toHaveValue("823.04");
  });

  it("falls back to the planned estimate unless every linked job reports its cost", () => {
    render(
      <ProductionRecordingForm
        jobs={[
          {
            jobId: 1,
            activity: "manufacturing",
            activityId: 1,
            status: "active",
            blueprintTypeId: 1000,
            blueprintName: null,
            productTypeId: 20185,
            productName: null,
            runs: 100,
            facilityId: 1,
            facilityName: null,
            solarSystemName: null,
            startDate: "2026-10-13T00:00:00Z",
            endDate: "2026-10-13T10:00:00Z",
            cost: null,
          },
        ]}
        onCancel={() => {}}
        onRecorded={() => {}}
        ticket={ticket()}
      />,
    );
    expect(installInput()).toHaveValue("1,000");
    expect(screen.getByTestId("install-cost-source")).toHaveTextContent("Planned estimate");
  });

  it("rescales untouched defaults when runs change (output / material / installation)", async () => {
    render(<ProductionRecordingForm onCancel={() => {}} onRecorded={() => {}} ticket={ticket()} />);

    await userEvent.clear(runsInput());
    await userEvent.type(runsInput(), "40");

    expect(outputInput()).toHaveValue(200); // 5/run * 40
    expect(materialInput()).toHaveValue(400); // 10/run * 40
    expect(installInput()).toHaveValue("400"); // 10 ISK/run * 40
  });

  it("keeps a hand-edited material when runs change again", async () => {
    render(<ProductionRecordingForm onCancel={() => {}} onRecorded={() => {}} ticket={ticket()} />);

    await userEvent.clear(materialInput());
    await userEvent.type(materialInput(), "777");
    await userEvent.clear(runsInput());
    await userEvent.type(runsInput(), "50");

    expect(materialInput()).toHaveValue(777); // user value survives
    expect(outputInput()).toHaveValue(250); // untouched, rescaled
  });

  it("shows the frozen facility context read-only", () => {
    render(<ProductionRecordingForm onCancel={() => {}} onRecorded={() => {}} ticket={ticket()} />);
    expect(screen.getByText("Sotiyo — Assembly")).toBeInTheDocument();
    expect(screen.getByText("Jita")).toBeInTheDocument();
  });
});

describe("ProductionRecordingForm — submission", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    api.previewTicketProduction.mockResolvedValue(PREVIEW);
  });

  it("previews the output and consumed materials before recording", async () => {
    render(<ProductionRecordingForm onCancel={() => {}} onRecorded={() => {}} ticket={ticket()} />);

    await userEvent.click(screen.getByRole("button", { name: "Preview" }));

    expect(await screen.findByText("Inventory after recording")).toBeInTheDocument();
    expect(screen.getByText("Added")).toBeInTheDocument();
    expect(screen.getByText("Consumed")).toBeInTheDocument();
    expect(screen.getByText("No existing stock")).toBeInTheDocument();
    expect(api.previewTicketProduction.mock.calls[0][1]).toMatchObject({ runsCompleted: 100 });
    expect(api.recordTicketProduction).not.toHaveBeenCalled();
  });

  it("records via record-production only, with the edited actuals, and no workflow call", async () => {
    api.recordTicketProduction.mockResolvedValue({});
    const onRecorded = vi.fn();
    render(
      <ProductionRecordingForm onCancel={() => {}} onRecorded={onRecorded} ticket={ticket()} />,
    );

    await userEvent.clear(runsInput());
    await userEvent.type(runsInput(), "40");
    await userEvent.clear(materialInput());
    await userEvent.type(materialInput(), "410");
    await previewThenRecord();

    await waitFor(() => expect(onRecorded).toHaveBeenCalled());
    expect(api.recordTicketProduction).toHaveBeenCalledTimes(1);
    expect(api.recordTicketProduction.mock.calls[0][0]).toBe("ticket-1");
    expect(api.recordTicketProduction.mock.calls[0][1]).toMatchObject({
      runsCompleted: 40,
      output: { typeId: 20185, quantity: 200 },
      inputs: [{ typeId: 34, quantity: 410 }],
      installationCost: "400",
      idempotencyKey: expect.any(String),
    });
    expect(api.completeTicket).not.toHaveBeenCalled();
    expect(api.startTicket).not.toHaveBeenCalled();
    expect(api.updateTicketStatus).not.toHaveBeenCalled();
  });

  it("accepts a zero installation cost", async () => {
    api.recordTicketProduction.mockResolvedValue({});
    render(<ProductionRecordingForm onCancel={() => {}} onRecorded={() => {}} ticket={ticket()} />);

    await userEvent.clear(installInput());
    await userEvent.type(installInput(), "0");
    await userEvent.tab();
    await previewThenRecord();

    await waitFor(() => expect(api.recordTicketProduction).toHaveBeenCalled());
    expect(api.recordTicketProduction.mock.calls[0][1].installationCost).toBe("0");
  });

  it("preserves the form and its values when the backend reports insufficient inventory", async () => {
    api.previewTicketProduction.mockRejectedValue(
      new Error("not enough of a material is in stock to record this consumption"),
    );
    render(<ProductionRecordingForm onCancel={() => {}} onRecorded={() => {}} ticket={ticket()} />);

    await userEvent.clear(runsInput());
    await userEvent.type(runsInput(), "40");
    await userEvent.click(screen.getByRole("button", { name: "Preview" }));

    expect(
      await screen.findByText("not enough of a material is in stock to record this consumption"),
    ).toBeInTheDocument();
    expect(runsInput()).toHaveValue(40);
    expect(outputInput()).toHaveValue(200);
    expect(screen.getByRole("button", { name: "Preview" })).toBeEnabled();
    expect(api.recordTicketProduction).not.toHaveBeenCalled();
  });

  it("drives a Reaction ticket through the same record-production path", async () => {
    api.recordTicketProduction.mockResolvedValue({});
    render(
      <ProductionRecordingForm
        onCancel={() => {}}
        onRecorded={() => {}}
        ticket={ticket({ kind: "reaction", capturedName: "Caesarium Cadmide" })}
      />,
    );

    expect(screen.getByRole("form", { name: "Record reaction" })).toBeInTheDocument();
    await previewThenRecord();
    await waitFor(() => expect(api.recordTicketProduction).toHaveBeenCalledTimes(1));
  });
});

describe("ProductionRecordingForm — legacy ticket without a frozen snapshot", () => {
  beforeEach(() => vi.clearAllMocks());

  it("renders manual-entry mode: an explainer, blank runs/output, materials from prerequisites", () => {
    render(
      <ProductionRecordingForm
        onCancel={() => {}}
        onRecorded={() => {}}
        ticket={ticket({
          executionSnapshot: null,
          recording: {
            state: "notRecorded",
            requestedQuantity: 0,
            recordedQuantity: 0,
            remainingQuantity: 0,
            surplusQuantity: 0,
          },
        })}
      />,
    );

    expect(screen.getByText(/predates frozen production planning/)).toBeInTheDocument();
    expect(runsInput()).toHaveValue(null);
    expect(outputInput()).toHaveValue(null);
    // The frozen prerequisite quantity is still the best available default.
    expect(materialInput()).toHaveValue(1000);
    expect(installInput()).toHaveValue("0");
  });
});

describe("ProductionRecordingForm when other Epics reserved the stock", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    api.recordTicketProduction.mockResolvedValue({});
  });

  it("names the holders and records after taking only the missing amount", async () => {
    const { ApiError } = await import("../../../../api/workspace");
    api.previewTicketProduction
      .mockRejectedValueOnce(
        new ApiError(409, {
          code: "insufficient_available",
          message: "Other Epics have reserved stock this recording needs.",
          shortages: [
            {
              typeId: 34,
              typeName: "Tritanium",
              needed: 1000,
              own: 600,
              free: 100,
              holders: [
                { orderId: "epic-b", displayName: "Manufacture Sacrilege", quantity: 250 },
                { orderId: "epic-c", displayName: "Manufacture Ishkur", quantity: 400 },
              ],
            },
          ],
        } as never),
      )
      .mockResolvedValueOnce(PREVIEW);
    const onRecorded = vi.fn();
    const user = userEvent.setup();
    render(<ProductionRecordingForm onCancel={() => {}} onRecorded={onRecorded} ticket={ticket()} />);

    await user.click(screen.getByRole("button", { name: "Preview" }));

    expect(await screen.findByText("Reserved by other Epics")).toBeInTheDocument();
    expect(
      screen.getByText(/Reserved by Manufacture Sacrilege \(250\), Reserved by Manufacture Ishkur \(400\)/),
    ).toBeInTheDocument();
    expect(onRecorded).not.toHaveBeenCalled();

    // 1,000 needed - 600 own - 100 free = 300: all 250 from the first
    // holder, the other 50 from the second.
    await user.click(
      screen.getByRole("button", {
        name: "Take 250 Tritanium from Manufacture Sacrilege, 50 Tritanium from Manufacture Ishkur and preview",
      }),
    );
    await user.click(await screen.findByRole("button", { name: "Record" }));

    await waitFor(() => expect(onRecorded).toHaveBeenCalled());
    const takes = [
      { orderId: "epic-b", typeId: 34 },
      { orderId: "epic-c", typeId: 34 },
    ];
    const [first, second] = api.previewTicketProduction.mock.calls;
    expect(first[1].takeFrom).toBeUndefined();
    expect(second[1].takeFrom).toEqual(takes);
    // Recording sends exactly what was previewed.
    expect(api.recordTicketProduction).toHaveBeenCalledTimes(1);
    expect(api.recordTicketProduction.mock.calls[0][1].takeFrom).toEqual(takes);
  });

  it("can back out without taking anything", async () => {
    const { ApiError } = await import("../../../../api/workspace");
    api.previewTicketProduction.mockRejectedValueOnce(
      new ApiError(409, {
        code: "insufficient_available",
        message: "x",
        shortages: [
          {
            typeId: 34,
            typeName: "Tritanium",
            needed: 1000,
            own: 0,
            free: 0,
            holders: [{ orderId: "epic-b", displayName: "Manufacture Sacrilege", quantity: 1000 }],
          },
        ],
      } as never),
    );
    const user = userEvent.setup();
    render(<ProductionRecordingForm onCancel={() => {}} onRecorded={() => {}} ticket={ticket()} />);

    await user.click(screen.getByRole("button", { name: "Preview" }));
    await user.click(await screen.findByRole("button", { name: "Keep their reservations" }));

    expect(screen.queryByText("Reserved by other Epics")).not.toBeInTheDocument();
    expect(api.recordTicketProduction).not.toHaveBeenCalled();
  });
});

describe("ProductionRecordingForm — combined ticket", () => {
  beforeEach(() => vi.clearAllMocks());

  // A combined ticket carries one copy of each member's prerequisites,
  // tagged with its Epic: the same material appears once per Epic.
  function combinedTicket(): TicketSummary {
    const base = ticket();
    const tritanium = base.prerequisites[0];
    return ticket({
      orderId: null,
      prerequisites: [
        { ...tritanium, id: "copy-a", requiredQuantity: 400, freshQuantity: 400, orderId: "epic-a" },
        { ...tritanium, id: "copy-b", requiredQuantity: 600, freshQuantity: 600, orderId: "epic-b" },
      ],
    });
  }

  it("shows one input per material, summed across Epics, and records one line", async () => {
    api.recordTicketProduction.mockResolvedValue({});
    api.previewTicketProduction.mockResolvedValue(PREVIEW);
    const onRecorded = vi.fn();
    render(<ProductionRecordingForm onCancel={() => {}} onRecorded={onRecorded} ticket={combinedTicket()} />);

    expect(screen.getAllByRole("spinbutton", { name: "Consumed quantity for Tritanium" })).toHaveLength(1);
    expect(materialInput()).toHaveValue(1000);

    await previewThenRecord();

    await waitFor(() => expect(onRecorded).toHaveBeenCalled());
    expect(api.recordTicketProduction.mock.calls[0][1].inputs).toEqual([{ typeId: 34, quantity: 1000 }]);
  });
});
