import type { PlannerScenario, PlannerSolution } from "./pi-planner";
import { requestJson } from "./workspace";

/**
 * Mirrors `apps/iskworks-api/src/routes/pi_saved_plans.rs`: PI plans saved as
 * frozen snapshots of a solve, with the colonies ticked as set up in game.
 */

/** The snapshot shape this web app writes and can render. */
export const SAVED_PLAN_SNAPSHOT_VERSION = 1;

/** The solve request with each character's name (and EVE id when prefilled). */
export interface SavedPlanScenario extends PlannerScenario {
  characterNames: string[];
  eveCharacterIds?: (number | null)[];
}

export interface SavedPlanSummary {
  id: string;
  name: string;
  iskPerMonth: string;
  colonies: number;
  /** Colonies ticked as set up. */
  setUp: number;
  homeSystemId: number;
  homeSystemName: string | null;
  priceObservedAt: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface SavedPlan extends SavedPlanSummary {
  snapshotVersion: number;
  scenario: SavedPlanScenario;
  solution: PlannerSolution;
  /** Indexes into `solution.plan.colonies`, ascending. */
  setUpColonies: number[];
}

export interface SavedPlanUpdate {
  name?: string;
  setUpColonies?: number[];
}

const JSON_HEADERS = { "Content-Type": "application/json" };

export function listSavedPlans(): Promise<SavedPlanSummary[]> {
  return requestJson<SavedPlanSummary[]>("/api/planetary/plans");
}

export function getSavedPlan(id: string): Promise<SavedPlan> {
  return requestJson<SavedPlan>(`/api/planetary/plans/${encodeURIComponent(id)}`);
}

export function savePlan(plan: { name: string; scenario: SavedPlanScenario; solution: PlannerSolution }): Promise<SavedPlan> {
  return requestJson<SavedPlan>("/api/planetary/plans", {
    method: "POST",
    headers: JSON_HEADERS,
    body: JSON.stringify(plan),
  });
}

export function updateSavedPlan(id: string, update: SavedPlanUpdate): Promise<SavedPlan> {
  return requestJson<SavedPlan>(`/api/planetary/plans/${encodeURIComponent(id)}`, {
    method: "PATCH",
    headers: JSON_HEADERS,
    body: JSON.stringify(update),
  });
}

export function deleteSavedPlan(id: string): Promise<void> {
  return requestJson<void>(`/api/planetary/plans/${encodeURIComponent(id)}`, { method: "DELETE" });
}
