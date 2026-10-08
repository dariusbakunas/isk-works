// Fixtures shared verbatim by several board tests. Suites whose defaults
// differ keep their own local fixtures.
import type { AcquisitionRun, TicketSummary } from "../../../api/industry";

export function makeDataTransfer() {
  const store = new Map<string, string>();
  return {
    setData: (format: string, data: string) => store.set(format, data),
    getData: (format: string) => store.get(format) ?? "",
    effectAllowed: "",
  };
}

export function runFixture(overrides: Partial<AcquisitionRun> = {}): AcquisitionRun {
  return {
    id: "run-1",
    workspaceId: "workspace-1",
    ownerId: "owner-1",
    displayId: "ACQ-0042",
    name: "Tuesday Jita Run",
    kind: "acquisition",
    status: "ready",
    marketRegionId: null,
    marketLocationId: null,
    priceSourceId: "source-1",
    createdAt: "2026-08-11T00:00:00Z",
    updatedAt: "2026-08-11T00:00:00Z",
    startedAt: null,
    completedAt: null,
    ...overrides,
  };
}

export function ticketFixture(overrides: Partial<TicketSummary> = {}): TicketSummary {
  return {
    id: "ticket-1",
    workspaceId: "workspace-1",
    ownerId: "owner-1",
    displayId: "ISK-2000",
    kind: "acquisition",
    typeId: 34,
    capturedName: "Tritanium",
    quantity: 100,
    orderId: null,
    notes: "",
    assigneeCharacterId: null,
    sourceBuildId: null,
    status: "todo",
    estimatedUnitCost: "5.0000",
    estimatedLineTotal: "500.0000",
    actualUnitCost: null,
    actualLineTotal: null,
    marketRegionId: null,
    marketLocationId: null,
    priceSourceId: null,
    acquisitionRunId: null,
    acquiredQuantity: null,
    executionSnapshot: null,
    createdAt: "2026-08-22T00:00:00Z",
    updatedAt: "2026-08-22T00:00:00Z",
    archivedAt: null,
    blockedBy: [],
    prerequisites: [],
    recording: null,
    ...overrides,
  };
}
