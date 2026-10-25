import type { CatalogueSkill, SkillProgress, SkillState } from "../../api/skills";
import { formatRemainingMs, skillLevelNumeral } from "../characters/characters-formatters";

// The client's own catalogue filters. "My skills" is everything injected;
// "Can train now" is injected and below V (or queued/training); "Have
// prerequisites for" is not injected but injectable.
export type SkillFilter = "all" | "my" | "canTrain" | "havePrerequisites";

export const SKILL_FILTERS: Array<{ id: SkillFilter; label: string }> = [
  { id: "all", label: "All skills" },
  { id: "my", label: "My skills" },
  { id: "canTrain", label: "Can train now" },
  { id: "havePrerequisites", label: "Have prerequisites for" },
];

const FILTER_STATES: Record<Exclude<SkillFilter, "all">, ReadonlySet<SkillState>> = {
  my: new Set(["max", "training", "queued", "canTrain"]),
  canTrain: new Set(["training", "queued", "canTrain"]),
  havePrerequisites: new Set(["canInject"]),
};

export function filterSkills(
  skills: CatalogueSkill[],
  { filter, groupId, search }: { filter: SkillFilter; groupId: number | null; search: string },
): CatalogueSkill[] {
  const needle = search.trim().toLowerCase();
  return skills.filter(
    (skill) =>
      (groupId === null || skill.groupId === groupId) &&
      (needle === "" || skill.name.toLowerCase().includes(needle)) &&
      (filter === "all" || (skill.progress !== null && FILTER_STATES[filter].has(skill.progress.state))),
  );
}

export interface PrerequisiteNode {
  skillTypeId: number;
  name: string;
  level: number;
  /** The character's active level reaches it (an Alpha's cap counts). */
  met: boolean;
  children: PrerequisiteNode[];
}

/** The skill's prerequisites, each with its own, as the inspector's tree.
 * Depth is capped as a guard; real SDE chains are a handful deep. */
export function prerequisiteTree(skill: CatalogueSkill, skills: CatalogueSkill[], depth = 0): PrerequisiteNode[] {
  if (depth > 8) return [];
  const byId = new Map(skills.map((candidate) => [candidate.typeId, candidate]));
  return skill.prerequisites.map((required) => {
    const prerequisite = byId.get(required.skillTypeId);
    return {
      skillTypeId: required.skillTypeId,
      name: prerequisite?.name ?? `Skill ${required.skillTypeId}`,
      level: required.level,
      met: (prerequisite?.progress?.activeLevel ?? 0) >= required.level,
      children: prerequisite ? prerequisiteTree(prerequisite, skills, depth + 1) : [],
    };
  });
}

export interface RequiredForEntry {
  skillTypeId: number;
  name: string;
  level: number;
}

/** For each skill, the catalogue skills that list it as a prerequisite, in
 * catalogue order. */
export function requiredForIndex(skills: CatalogueSkill[]): Map<number, RequiredForEntry[]> {
  const index = new Map<number, RequiredForEntry[]>();
  for (const skill of skills) {
    for (const required of skill.prerequisites) {
      const entries = index.get(required.skillTypeId) ?? [];
      entries.push({ skillTypeId: skill.typeId, name: skill.name, level: required.level });
      index.set(required.skillTypeId, entries);
    }
  }
  return index;
}

const STATE_LABELS: Record<SkillState, string> = {
  max: "Max",
  training: "Training",
  queued: "Queued",
  canTrain: "Can train",
  canInject: "Can inject",
  locked: "Locked",
  omegaOnly: "Omega only",
};

export function stateLabel(progress: SkillProgress): string {
  if (progress.state === "queued" && progress.queuedToLevel !== null) {
    return `Queued → ${skillLevelNumeral(progress.queuedToLevel)}`;
  }
  return STATE_LABELS[progress.state];
}

/** Time to the next level: "Max" at V, "—" when there's nothing to time
 * (not injected, or no training rate), "≈" when it's an estimate. */
export function timeLabel(progress: SkillProgress): string {
  if (progress.state === "max") return "Max";
  if (progress.secondsToNextLevel === null) return "—";
  const text = formatRemainingMs(progress.secondsToNextLevel * 1000);
  return progress.estimated ? `≈ ${text}` : text;
}

export const ATTRIBUTE_ABBREVIATIONS = {
  charisma: "CHA",
  intelligence: "INT",
  memory: "MEM",
  perception: "PER",
  willpower: "WIL",
} as const;
