import { vi } from "vitest";

import type { CalendarMilestone } from "../../../api/calendar";
import type { CharacterRosterEntry } from "../../../api/characters";
import type { ActiveJobs } from "../../../api/jobs";
import type { MarketOrdersOverview } from "../../../api/market-orders";
import type { PlanetaryOverview } from "../../../api/planetary";
import type { InventoryRunway } from "../../../api/inventory-analytics";
import { activeJobs } from "../../jobs/__tests__/fixtures";

export const NOW = new Date("2026-11-17T12:00:00Z");
export const hoursFromNow = (count: number) => new Date(NOW.getTime() + count * 3_600_000).toISOString();

export function rosterEntry(overrides: Partial<CharacterRosterEntry> = {}): CharacterRosterEntry {
  return {
    connectionId: "11111111-1111-1111-1111-111111111111",
    eveCharacterId: 2_119_000_001,
    characterName: "Kira Vantor",
    corporationId: 98_000_001,
    corporationName: "Perimeter Industrial Holdings",
    securityStatus: "5.00000",
    solarSystemId: 30_000_142,
    solarSystemName: "Jita",
    walletBalance: "4820000000.0000",
    totalSp: 94_300_000,
    unallocatedSp: null,
    trainingQueue: [
      {
        skillId: 3327,
        skillName: "Caldari Industrial",
        finishedLevel: 5,
        queuePosition: 0,
        startDate: hoursFromNow(-10),
        finishDate: hoursFromNow(200),
        trainingStartSp: 300_000,
        levelStartSp: 256_000,
        levelEndSp: 1_280_000,
        currentTrainedLevel: 4,
      },
    ],
    trainingObservedAt: hoursFromNow(-0.5),
    trainingQueueScopeMissing: false,
    calendarScopeMissing: false,
    marketOrdersScopeMissing: false,
    manufacturingActiveJobs: 1,
    manufacturingMaxJobs: 12,
    reactionActiveJobs: 1,
    reactionMaxJobs: 10,
    researchActiveJobs: 0,
    researchMaxJobs: 20,
    connectionStatus: "connected",
    health: "healthy",
    lastSyncedAt: hoursFromNow(-0.1),
    ...overrides,
  };
}

export function marketOverview(overrides: Partial<MarketOrdersOverview> = {}): MarketOrdersOverview {
  return {
    generatedAt: hoursFromNow(-0.2),
    characters: [],
    stargatesMissing: false,
    summary: {
      outbid: 0,
      tied: 0,
      best: 4,
      unknown: 0,
      likelyFilled: 0,
      activeSellValue: "4820000000.0000",
      buyEscrow: "612000000.0000",
      oldestBookAt: hoursFromNow(-0.25),
    },
    locations: [],
    ...overrides,
  };
}

export function planetaryOverview(overrides: Partial<PlanetaryOverview> = {}): PlanetaryOverview {
  return {
    priceObservedAt: hoursFromNow(-1),
    summary: {
      iskPerMonth: "412000000.0000",
      planetCount: 6,
      characterCount: 1,
      nextAction: { characterName: "Kira Vantor", planetName: "Barren VII-4", at: hoursFromNow(30) },
      alerts: { expired: 0, storageFull: 0, starved: 0 },
    },
    characters: [],
    ...overrides,
  };
}

export function runway(overrides: Partial<InventoryRunway> = {}): InventoryRunway {
  return {
    kpis: {
      shortfallValue: "0",
      materialsShort: 0,
      materialsTotal: 12,
      epicsBlocked: 0,
      epicsTotal: 2,
      demandCoveredPercent: 100,
    },
    epics: [],
    materials: [],
    missingPriceCount: 0,
    ...overrides,
  };
}

export function skillMilestone(overrides: Partial<Extract<CalendarMilestone, { kind: "skill" }>> = {}): CalendarMilestone {
  return {
    kind: "skill",
    id: "skill-1",
    connectionId: "11111111-1111-1111-1111-111111111111",
    eveCharacterId: 2_119_000_001,
    characterName: "Kira Vantor",
    title: "Caldari Industrial V finishes",
    occursAt: hoursFromNow(5),
    skillTypeId: 3327,
    skillName: "Caldari Industrial",
    targetLevel: 5,
    queuePosition: 0,
    nextSkillTypeId: null,
    nextSkillName: null,
    ...overrides,
  };
}

export function customMilestone(overrides: Partial<Extract<CalendarMilestone, { kind: "custom" }>> = {}): CalendarMilestone {
  return {
    kind: "custom",
    id: "custom-1",
    entryId: "entry-1",
    title: "Buy fuel blocks",
    occursAt: hoursFromNow(22),
    timeZone: "UTC",
    description: "",
    connectionId: null,
    eveCharacterId: null,
    characterName: null,
    ...overrides,
  };
}

export interface OverviewPayloads {
  characters: CharacterRosterEntry[];
  jobs: ActiveJobs;
  market: MarketOrdersOverview;
  planetary: PlanetaryOverview;
  runway: InventoryRunway;
  calendar: CalendarMilestone[];
}

/** One URL fragment per Overview source, in the order the page asks. */
export const sourceUrls = ["/api/characters", "/api/jobs", "/api/market-orders", "/api/planetary", "/api/inventory/runway", "/api/calendar"];

function respond(body: unknown) {
  return { ok: true, status: 200, json: async () => body };
}

/** Stubs `fetch` by URL; a payload set to an `Error` fails that source. */
export function fetchByUrl(overrides: Partial<Record<keyof OverviewPayloads, unknown>> = {}) {
  const payloads: Record<keyof OverviewPayloads, unknown> = {
    characters: [rosterEntry()],
    jobs: activeJobs(),
    market: marketOverview(),
    planetary: planetaryOverview(),
    runway: runway(),
    calendar: [skillMilestone()],
    ...overrides,
  };
  const keyFor = (url: string): keyof OverviewPayloads | null => {
    if (url.includes("/api/characters")) return "characters";
    if (url.includes("/api/jobs")) return "jobs";
    if (url.includes("/api/market-orders")) return "market";
    if (url.includes("/api/planetary")) return "planetary";
    if (url.includes("/api/inventory/runway")) return "runway";
    if (url.includes("/api/calendar")) return "calendar";
    return null;
  };
  const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
    const url = String(input);
    const key = keyFor(url);
    if (key === null) throw new Error(`unexpected fetch: ${url}`);
    const body = payloads[key];
    if (body instanceof Error) {
      return { ok: false, status: 500, json: async () => ({ error: { code: "internal", message: body.message } }) };
    }
    return respond(body);
  });
  vi.stubGlobal("fetch", fetchMock);
  return fetchMock;
}
