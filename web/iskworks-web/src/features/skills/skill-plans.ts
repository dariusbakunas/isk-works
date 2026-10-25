import type { CatalogueSkill, PlanEntry } from "../../api/skills";
import { formatRemainingMs } from "../characters/characters-formatters";

const key = (skillTypeId: number, level: number) => `${skillTypeId}:${level}`;

/** Every (skill, level) that must be trained before `entry`: the skill's
 * lower levels and, transitively, each prerequisite up to its level. */
function needs(entry: PlanEntry, byId: Map<number, CatalogueSkill>, out = new Set<string>(), depth = 0): Set<string> {
  for (let level = 1; level < entry.level; level += 1) out.add(key(entry.skillTypeId, level));
  if (depth > 12) return out;
  for (const required of byId.get(entry.skillTypeId)?.prerequisites ?? []) {
    for (let level = 1; level <= required.level; level += 1) out.add(key(required.skillTypeId, level));
    needs({ skillTypeId: required.skillTypeId, level: required.level }, byId, out, depth + 1);
  }
  return out;
}

/** Indices of the entries after `removedIndex` that need it (directly or
 * through another prerequisite), so removing it means removing them too. */
export function dependentsOf(entries: PlanEntry[], removedIndex: number, skills: CatalogueSkill[]): number[] {
  const byId = new Map(skills.map((skill) => [skill.typeId, skill]));
  const removed = entries[removedIndex];
  if (!removed) return [];
  const target = key(removed.skillTypeId, removed.level);
  const dependents: number[] = [];
  entries.forEach((entry, index) => {
    if (index > removedIndex && needs(entry, byId).has(target)) dependents.push(index);
  });
  return dependents;
}

/** A plan's progress as the client's plan cards read it. */
export function planProgressLabel(progress: {
  percentComplete: number;
  remainingSeconds: number | null;
  estimated: boolean;
}): string {
  if (progress.percentComplete >= 100) return "Completed";
  if (progress.remainingSeconds === null) return "—";
  const text = formatRemainingMs(progress.remainingSeconds * 1000);
  return progress.estimated ? `≈ ${text}` : text;
}

/** What a failed plan request says, for showing inline. Validation errors
 * come back as "validation failed: <reason>"; the reason reads on its own. */
export function errorMessage(reason: unknown, fallback: string): string {
  if (!(reason instanceof Error) || reason.message === "") return fallback;
  return reason.message.replace(/^validation failed:\s*/i, "");
}
