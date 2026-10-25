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

// --- Skill plans -------------------------------------------------------------

export interface PlanEntry {
  skillTypeId: number;
  level: number;
}

export interface SkillPlanProgress {
  /** By SP, 0-100. */
  percentComplete: number;
  remainingSeconds: number | null;
  missingSkillbooks: number;
  blocked: boolean;
  estimated: boolean;
}

export interface SkillPlanSummary {
  id: string;
  name: string;
  description: string;
  entryCount: number;
  totalSp: number;
  updatedAt: string;
  /** For the requested character; `null` without one or without its data. */
  progress: SkillPlanProgress | null;
}

export interface SkillPlanEntryView extends PlanEntry {
  name: string;
  groupName: string;
  rank: number;
  primaryAttribute: SkillAttribute;
  secondaryAttribute: SkillAttribute;
  omegaOnly: boolean;
  /** SP this level takes from the previous one. */
  sp: number;
}

export type PlanEntryStatus = "done" | "partial" | "remaining" | "blocked";

export interface PlanEntryProgress extends PlanEntry {
  status: PlanEntryStatus;
  spTotal: number;
  spRemaining: number;
  seconds: number | null;
  estimated: boolean;
}

export interface PlanEvaluation {
  entries: PlanEntryProgress[];
  totalSp: number;
  remainingSp: number;
  percentComplete: number;
  remainingSeconds: number | null;
  /** Skill type ids the character hasn't injected, in plan order. */
  missingSkillbooks: number[];
  blocked: boolean;
  estimated: boolean;
}

export interface CharacterPlanProgress {
  connectionId: string;
  name: string;
  evaluation: PlanEvaluation | null;
}

export interface SkillPlanDetail {
  plan: { id: string; name: string; description: string; createdAt: string; updatedAt: string };
  entries: SkillPlanEntryView[];
  /** Closest to finishing first. */
  characters: CharacterPlanProgress[];
  /** Prerequisites and lower levels the last write added. */
  inserted: PlanEntry[];
}

export interface SkillPlanBody {
  name: string;
  description: string;
  entries: PlanEntry[];
}

export interface UnrecognizedLine {
  line: number;
  text: string;
  reason: "unknownSkill" | "invalidLevel" | "missingLevel";
}

export interface PlanTextPreview {
  entries: SkillPlanEntryView[];
  inserted: PlanEntry[];
  unrecognized: UnrecognizedLine[];
  /** Why the text can't be imported as it is. */
  error: string | null;
}

export interface ImportedSkillPlan extends SkillPlanDetail {
  unrecognized: UnrecognizedLine[];
}

const json = (body: unknown): RequestInit => ({
  body: JSON.stringify(body),
  headers: { "Content-Type": "application/json" },
});

export function listSkillPlans(characterId: string | null, signal?: AbortSignal): Promise<SkillPlanSummary[]> {
  const query = characterId ? `?${new URLSearchParams({ characterId })}` : "";
  return request(`/api/skill-plans${query}`, { signal });
}

export function getSkillPlan(id: string, signal?: AbortSignal): Promise<SkillPlanDetail> {
  return request(`/api/skill-plans/${encodeURIComponent(id)}`, { signal });
}

export function createSkillPlan(body: SkillPlanBody): Promise<SkillPlanDetail> {
  return request("/api/skill-plans", { method: "POST", ...json(body) });
}

export function updateSkillPlan(id: string, body: SkillPlanBody): Promise<SkillPlanDetail> {
  return request(`/api/skill-plans/${encodeURIComponent(id)}`, { method: "PUT", ...json(body) });
}

export function deleteSkillPlan(id: string): Promise<void> {
  return request(`/api/skill-plans/${encodeURIComponent(id)}`, { method: "DELETE" });
}

export function previewPlanText(text: string, signal?: AbortSignal): Promise<PlanTextPreview> {
  return request("/api/skill-plans/import/preview", { method: "POST", signal, ...json({ text }) });
}

export function importPlanText(body: { name: string; description: string; text: string }): Promise<ImportedSkillPlan> {
  return request("/api/skill-plans/import", { method: "POST", ...json(body) });
}

/** The plan as the client's plan text, ready to paste into the game. */
export async function exportPlanText(id: string): Promise<string> {
  const baseUrl = (import.meta.env.VITE_API_BASE_URL ?? "").replace(/\/$/, "");
  const response = await fetch(`${baseUrl}/api/skill-plans/${encodeURIComponent(id)}/export`, {
    credentials: "include",
  });
  if (!response.ok) throw new Error("Could not export the plan.");
  return response.text();
}
