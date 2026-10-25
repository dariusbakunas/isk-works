import type { CatalogueSkill, SkillCatalogue, SkillProgress } from "../../../api/skills";

export const AURA = "11111111-1111-1111-1111-111111111111";
export const DRONES = 3436;
export const DRONE_INTERFACING = 3442;
export const SCOUT_DRONES = 3437;
export const EW_DRONES = 23566;
export const LEADERSHIP = 3348;

export function progress(overrides: Partial<SkillProgress> = {}): SkillProgress {
  return {
    trainedLevel: 0,
    activeLevel: 0,
    skillpoints: 0,
    state: "canInject",
    queuedToLevel: null,
    secondsToNextLevel: null,
    estimated: false,
    ...overrides,
  };
}

export function skill(overrides: Partial<CatalogueSkill> & Pick<CatalogueSkill, "typeId" | "name">): CatalogueSkill {
  return {
    groupId: 273,
    groupName: "Drones",
    rank: 1,
    primaryAttribute: "memory",
    secondaryAttribute: "perception",
    omegaOnly: false,
    prerequisites: [],
    spPerLevel: [250, 1415, 8000, 45255, 256000],
    progress: null,
    ...overrides,
  };
}

/** Aura: Drones V, Drone Interfacing IV training to V, Scout Drones not
 * injected (can inject), EW Drones locked, Leadership II. */
export function catalogue(overrides: Partial<SkillCatalogue> = {}): SkillCatalogue {
  return {
    skillDataAvailable: true,
    character: {
      connectionId: AURA,
      name: "Aura Valex",
      skillsScopeMissing: false,
      observedAt: "2026-10-25T12:00:00Z",
      totalSp: 112_822_873,
      unallocatedSp: 25_000,
      alphaDetected: false,
      attributes: { charisma: 19, intelligence: 20, memory: 27, perception: 27, willpower: 17 },
      rate: { basis: { kind: "attributes" }, clone: "omega", accelerated: false },
    },
    groups: [
      { groupId: 273, name: "Drones", skillCount: 4, maxSp: 2_816_000, injectedCount: 2, trainedSp: 556_000 },
      { groupId: 258, name: "Leadership", skillCount: 1, maxSp: 256_000, injectedCount: 1, trainedSp: 1_915 },
    ],
    skills: [
      skill({
        typeId: DRONE_INTERFACING,
        name: "Drone Interfacing",
        rank: 5,
        spPerLevel: [1250, 7072, 40000, 226275, 1280000],
        prerequisites: [{ skillTypeId: DRONES, level: 5 }],
        progress: progress({ trainedLevel: 4, activeLevel: 4, skillpoints: 300_000, state: "training", queuedToLevel: 5, secondsToNextLevel: 1_200_000 }),
      }),
      skill({
        typeId: DRONES,
        name: "Drones",
        progress: progress({ trainedLevel: 5, activeLevel: 5, skillpoints: 256_000, state: "max" }),
      }),
      skill({
        typeId: EW_DRONES,
        name: "EW Drone Interfacing",
        rank: 5,
        spPerLevel: [1250, 7072, 40000, 226275, 1280000],
        omegaOnly: true,
        prerequisites: [{ skillTypeId: DRONE_INTERFACING, level: 5 }],
        progress: progress({ state: "locked" }),
      }),
      skill({
        typeId: SCOUT_DRONES,
        name: "Scout Drone Operation",
        prerequisites: [{ skillTypeId: DRONES, level: 1 }],
        progress: progress({ state: "canInject" }),
      }),
      skill({
        typeId: LEADERSHIP,
        name: "Leadership",
        groupId: 258,
        groupName: "Leadership",
        primaryAttribute: "charisma",
        secondaryAttribute: "willpower",
        progress: progress({ trainedLevel: 2, activeLevel: 2, skillpoints: 1_915, state: "canTrain", secondsToNextLevel: 13_277 }),
      }),
    ],
    ...overrides,
  };
}

import type { CharacterDetail } from "../../../api/characters";

export const MIRA = "22222222-2222-2222-2222-222222222222";

export function characterDetail(overrides: Partial<CharacterDetail> = {}): CharacterDetail {
  return {
    connectionId: AURA,
    eveCharacterId: 2_119_000_001,
    characterName: "Aura Valex",
    corporationId: null,
    corporationName: null,
    securityStatus: null,
    solarSystemId: null,
    solarSystemName: null,
    walletBalance: null,
    totalSp: 112_822_873,
    unallocatedSp: 25_000,
    trainingQueue: [],
    trainingObservedAt: "2026-10-25T11:55:00Z",
    trainingQueueScopeMissing: false,
    calendarScopeMissing: false,
    marketOrdersScopeMissing: false,
    manufacturingActiveJobs: null,
    manufacturingMaxJobs: null,
    reactionActiveJobs: null,
    reactionMaxJobs: null,
    researchActiveJobs: null,
    researchMaxJobs: null,
    connectionStatus: "connected",
    health: "healthy",
    lastSyncedAt: "2026-10-25T11:55:00Z",
    industryJobs: [],
    sources: [
      {
        sourceKind: "skills",
        refreshState: "current",
        observedAt: "2026-10-25T11:55:00Z",
        nextRefreshAt: null,
        lastError: null,
      },
    ],
    ...overrides,
  };
}

export function roster() {
  const { sources: _sources, industryJobs: _jobs, ...aura } = characterDetail();
  return [aura, { ...aura, connectionId: MIRA, eveCharacterId: 2_119_000_002, characterName: "Mira Caldari", totalSp: 6_200_000 }];
}
