import { json, request } from "./request";
import type { Ticket } from "./tickets";

// The first (and only) kind of Execution Batch: groups compatible,
// already-Ready Acquisition tickets for one shopping trip, without
// merging or destroying the source tickets.
export type AcquisitionRunStatus = "ready" | "inProgress" | "complete";

export interface AcquisitionRun {
  id: string;
  workspaceId: string;
  ownerId: string;
  displayId: string;
  name: string;
  kind: "acquisition";
  status: AcquisitionRunStatus;
  marketRegionId: number | null;
  marketLocationId: number | null;
  priceSourceId: string | null;
  createdAt: string;
  updatedAt: string;
  startedAt: string | null;
  completedAt: string | null;
}

// The Run's real, uncapped per-type acquired total -- may exceed the
// matching tickets' summed `quantity` (over-acquisition is valid). Ticket
// `acquiredQuantity` stays capped to that ticket's own demand; this is the
// actual "Acquired" number to show in the drawer.
export interface AcquisitionRunItem {
  typeId: number;
  acquiredQuantity: number;
}

export interface AcquisitionRunDetail extends AcquisitionRun {
  tickets: Ticket[];
  items: AcquisitionRunItem[];
}

export function createAcquisitionRun(input: { name?: string; ticketIds: string[] }): Promise<AcquisitionRun> {
  return request("/api/acquisition-runs", json("POST", input));
}

export function listAcquisitionRuns(): Promise<AcquisitionRun[]> {
  return request("/api/acquisition-runs");
}

export function getAcquisitionRun(runId: string): Promise<AcquisitionRunDetail> {
  return request(`/api/acquisition-runs/${runId}`);
}

// No preview/confirm step -- an Order's status is fully derived, so
// there's no lock-transition side effect to warn about before starting.
export function startAcquisitionRun(runId: string): Promise<AcquisitionRun> {
  return request(`/api/acquisition-runs/${runId}/start`, { method: "POST" });
}

export interface AcquisitionProgressItem {
  typeId: number;
  acquiredQuantity: number;
}

export function recordAcquisitionProgress(
  runId: string,
  items: AcquisitionProgressItem[],
): Promise<AcquisitionRun> {
  return request(`/api/acquisition-runs/${runId}/items`, json("PATCH", { items }));
}

export function completeAcquisitionRun(runId: string): Promise<AcquisitionRun> {
  return request(`/api/acquisition-runs/${runId}/complete`, { method: "POST" });
}
