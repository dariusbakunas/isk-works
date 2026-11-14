import type { SavedPlan, SavedPlanSummary } from "../../../../../api/pi-plans";
import { SOLUTION } from "../../planner/__tests__/plan-fixture";

export const SUMMARY: SavedPlanSummary = {
  id: "plan-1",
  name: "Q-3HS5 · Mechanical Parts",
  iskPerMonth: "1420000000",
  colonies: 3,
  setUp: 1,
  homeSystemId: 30_000_797,
  homeSystemName: "Q-3HS5",
  priceObservedAt: "2026-11-01T00:00:00Z",
  createdAt: "2026-11-14T10:00:00Z",
  updatedAt: "2026-11-14T12:00:00Z",
};

/** The Plan tab's fixture solve, saved with one colony set up. */
export const SAVED: SavedPlan = {
  ...SUMMARY,
  snapshotVersion: 1,
  scenario: {
    homeSystemId: 30_000_797,
    radius: 2,
    characters: [
      { interplanetaryConsolidation: 1, commandCenterUpgrades: 4 },
      { interplanetaryConsolidation: 0, commandCenterUpgrades: 4 },
    ],
    characterNames: ["Valka", "Alt"],
    defaultCustomsRate: "0.05",
    resetDays: 3,
    haulDays: 7,
    upliftAtLogin: true,
    buyIntermediates: false,
    maxColonies: null,
    maxSystemsPerCharacter: 1,
    estimateUnscanned: true,
    targetTypeId: null,
  },
  solution: SOLUTION,
  setUpColonies: [0],
};
