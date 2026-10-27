import { vi } from "vitest";

import type { CharacterRosterEntry } from "../../../api/characters";
import type {
  AttributeView,
  CatalogueItem,
  CatalogueView,
  FitDetail,
  FitItemView,
  FitPilot,
  FitStats,
  FitStatsView,
  FitSummary,
  FitTextPreview,
  FittingsMeta,
  ItemStats,
} from "../../../api/fittings";

export const MIRA = "00000000-0000-0000-0000-000000000001";
export const AURA = "00000000-0000-0000-0000-000000000002";
export const FRESH = "00000000-0000-0000-0000-000000000003";
export const SCOUT = "00000000-0000-0000-0000-000000000004";
export const FIT_ID = "11111111-1111-1111-1111-111111111111";

export function meta(available = true): FittingsMeta {
  return {
    available,
    engine: available ? { dataVersion: "13.3650286.0", major: 13, buildNumber: 3650286 } : null,
    sde: { buildNumber: "3482594" },
    damageProfiles: [
      { id: "uniform", name: "Uniform", em: 25, thermal: 25, kinetic: 25, explosive: 25 },
      { id: "em", name: "EM", em: 1, thermal: 0, kinetic: 0, explosive: 0 },
      { id: "guristas", name: "Guristas", em: 0, thermal: 1828, kinetic: 7413, explosive: 0 },
    ],
  };
}

const roster = (connectionId: string, eveCharacterId: number, characterName: string) =>
  ({ connectionId, eveCharacterId, characterName }) as CharacterRosterEntry;

export function characters(): CharacterRosterEntry[] {
  return [
    roster(MIRA, 90000001, "Mira"),
    roster(AURA, 90000002, "Aura"),
    roster(FRESH, 90000003, "Fresh Char"),
    roster(SCOUT, 90000004, "Scout"),
  ];
}

export function fits(): FitSummary[] {
  const fit = (id: string, name: string, ship: [number, string, string], statuses: Array<FitSummary["pilots"][number]["status"]>): FitSummary => ({
    id,
    name,
    ship: { typeId: ship[0], name: ship[1], groupName: ship[2] },
    itemCount: 13,
    updatedAt: "2026-10-26T00:00:00Z",
    pilots: [MIRA, AURA, FRESH, SCOUT].map((connectionId, index) => ({
      connectionId,
      name: ["Mira", "Aura", "Fresh Char", "Scout"][index],
      status: statuses[index],
    })),
  });
  return [
    fit(FIT_ID, "Null Transport", [12731, "Bustard", "Deep Space Transport"], ["canUseAll", "canFlyHull", "cannot", null]),
    fit("22222222-2222-2222-2222-222222222222", "Lowsec Brick", [12731, "Bustard", "Deep Space Transport"], ["cannot", "canFlyHull", "cannot", null]),
    fit("33333333-3333-3333-3333-333333333333", "L4 Cruise", [641, "Megathron", "Battleship"], ["cannot", "cannot", "cannot", null]),
  ];
}

export function item(overrides: Partial<FitItemView> = {}): FitItemView {
  return {
    typeId: 2048,
    slotKind: "low",
    slotIndex: 0,
    quantity: 1,
    state: "online",
    chargeTypeId: null,
    name: "Damage Control II",
    groupName: "Damage Control",
    chargeName: null,
    ...overrides,
  };
}

export function fitDetail(): FitDetail {
  return {
    id: FIT_ID,
    name: "Null Transport",
    description: "",
    ship: { typeId: 12731, name: "Bustard", groupName: "Deep Space Transport" },
    items: [
      item(),
      item({ typeId: 5975, slotKind: "med", slotIndex: 0, state: "active", name: "50MN Y-T8 Compact Microwarpdrive", groupName: "Propulsion Module" }),
      item({ typeId: 12058, slotKind: "med", slotIndex: 1, state: "active", name: "10MN Afterburner II", groupName: "Propulsion Module" }),
      item({ typeId: 2873, slotKind: "high", slotIndex: 0, state: "active", chargeTypeId: 185, chargeName: "EMP S", name: "200mm AutoCannon II", groupName: "Projectile Weapon" }),
      item({ typeId: 2456, slotKind: "drone_bay", slotIndex: null, quantity: 5, state: "active", name: "Hobgoblin II", groupName: "Combat Drone" }),
      item({ typeId: 28668, slotKind: "cargo", slotIndex: null, quantity: 100, state: "offline", name: "Nanite Repair Paste", groupName: "Nanite Repair Paste" }),
    ],
    createdAt: "2026-10-26T00:00:00Z",
    updatedAt: "2026-10-26T00:00:00Z",
  };
}

const usage = (used: number, total: number) => ({ used, total });

export function stats(overrides: Partial<FitStats> = {}): FitStats {
  return {
    resources: {
      cpu: usage(357, 362.5),
      powergrid: usage(216.5, 250),
      calibration: usage(150, 400),
      turretHardpoints: usage(1, 2),
      launcherHardpoints: usage(0, 2),
      highSlots: usage(1, 2),
      medSlots: usage(2, 3),
      lowSlots: usage(1, 2),
      rigSlots: usage(0, 2),
      droneBandwidth: usage(25, 25),
      droneBay: usage(50, 50),
    },
    capacitor: { capacity: 2718.75, rechargeSeconds: 543.75, peakDelta: -10.4, state: { kind: "depletes", seconds: 190 } },
    offense: { weaponDps: 116.9, weaponDpsWithReload: 111.4, volley: 412, droneDps: 99, totalDps: 215.9 },
    defense: {
      ehp: 90790,
      shield: { hp: 9900, ehp: 73766, resists: { em: 0.817, thermal: 0.929, kinetic: 0.894, explosive: 0.823 } },
      armor: { hp: 2875, ehp: 7074, resists: { em: 0.575, thermal: 0.883, kinetic: 0.681, explosive: 0.235 } },
      hull: { hp: 4000, ehp: 9950, resists: { em: 0.598, thermal: 0.598, kinetic: 0.598, explosive: 0.598 } },
      shieldPassiveRegen: 196.7,
      shieldBoost: 0,
      armorRepair: 0,
      hullRepair: 0,
    },
    targeting: { maxTargets: 3, lockRange: 22424.6, scanResolution: 61.4, sensorStrength: 24, signatureRadius: 1094.7 },
    navigation: { maxVelocity: 655.3, mass: 25000000, inertia: 0.5687, alignSeconds: 19.71, warpSpeed: 3.5 },
    drones: { active: 5, maxActive: 5, controlRange: 60000 },
    ...overrides,
  };
}

function itemStats(overrides: Partial<ItemStats> = {}): ItemStats {
  return {
    reachedState: "online",
    maxState: "online",
    cpu: 30,
    powergrid: 1,
    calibration: 0,
    capacitorPerSecond: 0,
    optimal: 0,
    falloff: 0,
    dps: 0,
    headline: null,
    ...overrides,
  };
}

export function statsView(overrides: Partial<FitStatsView> = {}): FitStatsView {
  return {
    stats: stats(),
    items: [
      itemStats(),
      itemStats({ reachedState: "active", maxState: "overload", cpu: 41, powergrid: 125, capacitorPerSecond: 21.4, headline: { kind: "velocity", value: 505 } }),
      // Requested active, held online by the propulsion group limit.
      itemStats({ reachedState: "online", maxState: "overload", cpu: 32, powergrid: 50, headline: { kind: "velocity", value: 135 } }),
      itemStats({ reachedState: "active", maxState: "overload", cpu: 9, powergrid: 4, optimal: 1200, falloff: 8000, dps: 116.9, capacitorPerSecond: 0, headline: { kind: "dps", value: 116.9 } }),
      itemStats({ reachedState: "active", maxState: "active", cpu: 0, powergrid: 0, dps: 99, headline: { kind: "dps", value: 99 } }),
      null,
    ],
    violations: [{ target: { kind: "item", index: 3 }, rule: "resource", detail: { resource: "cpu", used: 380, available: 362.5 } }],
    unknownItems: [5],
    flyability: null,
    ...overrides,
  };
}

export function pilots(): FitPilot[] {
  const headline = (ehp: number) => ({ ehp, dps: 215.9, capacitor: { kind: "depletes" as const, seconds: 190 }, maxVelocity: 655.3, alignSeconds: 19.71 });
  return [
    {
      connectionId: MIRA,
      name: "Mira",
      status: "canUseAll",
      flyability: { status: "canUseAll", missing: [], remainingSeconds: 0, missingSkillbooks: [], omegaRequired: false, estimated: false, blockedItems: [] },
      stats: headline(90790),
    },
    {
      connectionId: AURA,
      name: "Aura",
      status: "canFlyHull",
      flyability: {
        status: "canFlyHull",
        missing: [{ skillTypeId: 3425, name: "Shield Upgrades", activeLevel: 2, trainedLevel: 2, requiredLevel: 4, seconds: 534_240, omegaRequired: false }],
        remainingSeconds: 534_240,
        missingSkillbooks: [],
        omegaRequired: false,
        estimated: false,
        blockedItems: [3],
      },
      stats: headline(78496),
    },
    {
      connectionId: FRESH,
      name: "Fresh Char",
      status: "cannot",
      flyability: {
        status: "cannot",
        missing: [
          { skillTypeId: 3342, name: "Caldari Hauler", activeLevel: 0, trainedLevel: 0, requiredLevel: 5, seconds: 1_024_001, omegaRequired: false },
          { skillTypeId: 19719, name: "Transport Ships", activeLevel: 0, trainedLevel: 0, requiredLevel: 1, seconds: null, omegaRequired: true },
        ],
        remainingSeconds: 1_024_001,
        missingSkillbooks: [3342, 19719],
        omegaRequired: true,
        estimated: false,
        blockedItems: [],
      },
      stats: headline(63593),
    },
    { connectionId: SCOUT, name: "Scout", status: null, flyability: null, stats: null },
  ];
}

export function preview(): FitTextPreview {
  return {
    fit: {
      name: "Null Transport",
      ship: { typeId: 12731, name: "Bustard", groupName: "Deep Space Transport" },
      items: fitDetail().items,
    },
    unknownLines: [],
    error: null,
  };
}

export function explanation(): AttributeView {
  return {
    base: 290,
    value: 362.5,
    sources: [
      { from: { kind: "skill", typeId: 3426 }, name: "CPU Management", operator: "post_percent", value: 25, penalty: null, applied: true },
      { from: { kind: "item", index: 0 }, name: "Co-Processor II", operator: "post_percent", value: 10, penalty: 0.869, applied: true },
    ],
  };
}

export interface StubOptions {
  meta?: FittingsMeta;
  fits?: FitSummary[];
  preview?: FitTextPreview;
  statsView?: FitStatsView;
  /** What a draft calculation answers; by default 1,000 more EHP than saved. */
  calculateView?: FitStatsView;
  catalogue?: unknown;
}

/** The saved fit's stats with 1,000 more EHP: what an edit calculates. */
export function editedStatsView(): FitStatsView {
  const edited = stats();
  edited.defense.ehp += 1000;
  return statsView({ stats: edited });
}

const catalogueItem = (typeId: number, name: string, overrides: Partial<CatalogueItem> = {}): CatalogueItem => ({
  typeId,
  name,
  kind: "module",
  slot: "low",
  cpu: 30,
  powergrid: 1,
  calibration: 0,
  hardpoint: null,
  rigSize: null,
  chargeSize: null,
  marketPath: ["Ship Equipment", "Hull & Armor", "Damage Controls"],
  canUse: null,
  ...overrides,
});

/** What the module browser's catalogue searches answer, by kind. */
export function catalogueFor(url: string): CatalogueView {
  const params = new URL(url, "http://localhost").searchParams;
  const pilot = params.get("pilot");
  const canUse = (value: boolean) => (pilot && pilot !== "allV" && pilot !== "allZero" ? value : null);
  switch (params.get("kind")) {
    case "charges":
      return {
        entries: [
          catalogueItem(186, "Phased Plasma S", { kind: "charge", slot: null, marketPath: ["Ammunition", "Projectile"] }),
          catalogueItem(185, "EMP S", { kind: "charge", slot: null, marketPath: ["Ammunition", "Projectile"] }),
        ],
        truncated: false,
      };
    case "drones":
      return {
        entries: [catalogueItem(2456, "Hobgoblin II", { kind: "drone", slot: null, marketPath: ["Drones", "Combat Drones"] })],
        truncated: false,
      };
    default:
      return {
        entries: [
          catalogueItem(2048, "Damage Control II", { canUse: canUse(true) }),
          catalogueItem(11640, "Warp Core Stabilizer II", { canUse: canUse(false) }),
          catalogueItem(2873, "200mm AutoCannon II", {
            slot: "high",
            hardpoint: "turret",
            marketPath: ["Ship Equipment", "Turrets", "Projectile"],
            canUse: canUse(true),
          }),
        ],
        truncated: false,
      };
  }
}

/** Answers every Fittings request from the fixtures, recording the calls. */
export function stubFetch(options: StubOptions = {}) {
  return vi.spyOn(globalThis, "fetch").mockImplementation(async (input, init) => {
    const url = String(input);
    const method = init?.method ?? "GET";
    const json = (body: unknown, status = 200) =>
      new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });
    if (url.endsWith("/api/fittings/meta")) return json(options.meta ?? meta());
    if (url.includes("/api/fittings/calculate")) return json(options.calculateView ?? editedStatsView());
    if (url.includes("/api/fittings/catalogue")) return json(options.catalogue ?? catalogueFor(url));
    if (url.includes("/api/types/search")) {
      return json([{ typeId: 34, typeName: "Tritanium", groupName: "Mineral", published: true }]);
    }
    if (url.includes(`/api/fittings/`) && method === "PUT") {
      const body = JSON.parse(String(init?.body ?? "{}"));
      return json({ ...fitDetail(), name: body.name });
    }
    if (url.endsWith("/api/characters")) return json(characters());
    if (url.endsWith("/api/fittings") && method === "GET") return json(options.fits ?? fits());
    if (url.endsWith("/api/fittings") && method === "POST") return json({ ...fitDetail(), id: "copy-id" }, 201);
    if (url.includes("/api/fittings/parse")) return json(options.preview ?? preview());
    if (url.includes("/api/fittings/import")) return json({ ...fitDetail(), id: "new-fit-id" }, 201);
    if (url.includes("/stats")) return json(options.statsView ?? statsView());
    if (url.includes("/pilots")) return json(pilots());
    if (url.includes("/explain")) return json(explanation());
    if (url.includes("/skill-plan")) return json({ id: "plan-1", name: "Fit: Null Transport", description: "", entries: [] }, 201);
    if (url.includes("/export")) return new Response("[Bustard, Null Transport]\n", { status: 200 });
    if (url.includes(`/api/fittings/`) && method === "DELETE") return new Response(null, { status: 204 });
    if (url.includes(`/api/fittings/`)) return json(fitDetail());
    return new Response("not stubbed", { status: 404 });
  });
}
