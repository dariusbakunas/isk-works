import { jsonRequest } from "./json-request";

/** Mirrors `iskworks_sde::SkillAttribute`. */
export type SkillAttribute = "charisma" | "intelligence" | "memory" | "perception" | "willpower";

/** Mirrors `iskworks_core::skills::SkillState`. */
export type SkillState = "max" | "training" | "queued" | "canTrain" | "canInject" | "locked" | "omegaOnly";

export interface RequiredSkill {
  skillTypeId: number;
  level: number;
}

export interface SkillProgress {
  trainedLevel: number;
  /** Below `trainedLevel` when an Alpha clone caps the skill. */
  activeLevel: number;
  skillpoints: number;
  state: SkillState;
  queuedToLevel: number | null;
  /** To finish the next level; `null` at V, not injected, or no rate. */
  secondsToNextLevel: number | null;
  estimated: boolean;
}

export interface CatalogueSkill {
  typeId: number;
  name: string;
  groupId: number;
  groupName: string;
  rank: number;
  primaryAttribute: SkillAttribute;
  secondaryAttribute: SkillAttribute;
  omegaOnly: boolean;
  /** In the client's prerequisite order. */
  prerequisites: RequiredSkill[];
  /** Cumulative SP for levels I..V. */
  spPerLevel: [number, number, number, number, number];
  /** `null` without a character, or without its skills data. */
  progress: SkillProgress | null;
}

export interface CatalogueGroup {
  groupId: number;
  name: string;
  skillCount: number;
  maxSp: number;
  injectedCount: number | null;
  trainedSp: number | null;
}

export type RateBasis =
  | { kind: "attributes" }
  | { kind: "observedQueue"; spPerMinute: number }
  | { kind: "unknown" };

export interface TrainingRate {
  basis: RateBasis;
  clone: "alpha" | "omega";
  /** Training faster than the attributes explain (an accelerator). */
  accelerated: boolean;
}

export interface CharacterAttributes {
  charisma: number;
  intelligence: number;
  memory: number;
  perception: number;
  willpower: number;
}

export interface SkillCatalogueCharacter {
  connectionId: string;
  name: string;
  skillsScopeMissing: boolean;
  observedAt: string | null;
  totalSp: number | null;
  unallocatedSp: number | null;
  alphaDetected: boolean;
  attributes: CharacterAttributes | null;
  rate: TrainingRate | null;
}

export interface SkillCatalogue {
  /** False when the server's SDE import has no skill data yet. */
  skillDataAvailable: boolean;
  character: SkillCatalogueCharacter | null;
  /** In catalogue order (by group name). */
  groups: CatalogueGroup[];
  /** Grouped as `groups`, by name within a group. */
  skills: CatalogueSkill[];
}

const request = jsonRequest({
  unavailable: "ISK Works API is unavailable.",
  failed: "Skills request failed.",
});

export function getSkillCatalogue(characterId: string | null, signal?: AbortSignal): Promise<SkillCatalogue> {
  const query = characterId ? `?${new URLSearchParams({ characterId })}` : "";
  return request(`/api/skills/catalogue${query}`, { signal });
}
