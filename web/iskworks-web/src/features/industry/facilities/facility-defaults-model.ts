import type {
  FacilityDefaultAssignment,
  FacilityDefaultTarget,
  FacilityDefaultsView,
  FacilityRole,
  FacilityTargetBonus,
} from "../../../api/industry";

export function targetKey(role: FacilityRole, filterId: number | null): string {
  return `${role}:${filterId ?? "fallback"}`;
}

export function bonusFor(
  view: FacilityDefaultsView,
  facilityProfileId: string,
  role: FacilityRole,
  filterId: number | null,
): FacilityTargetBonus | undefined {
  return view.bonuses.find(
    (bonus) =>
      bonus.facilityProfileId === facilityProfileId && bonus.role === role && bonus.filterId === filterId,
  );
}

/** The role's "Everything else" row. */
export function fallbackTarget(view: FacilityDefaultsView, role: FacilityRole): FacilityDefaultTarget | undefined {
  return view.targets.find((target) => target.role === role && target.filterId === null);
}

/**
 * What a new Build making something in this target gets: the target's own
 * default, else the role fallback's facility with its bonus for this target.
 */
export function effectiveChoice(
  view: FacilityDefaultsView,
  target: FacilityDefaultTarget,
): { facilityName: string; materialBonusPercent: string; timeBonusPercent: string; viaFallback: boolean } | null {
  if (target.default) return { ...target.default, viaFallback: false };
  if (target.filterId === null || target.status === "unknownTarget") return null;
  const fallback = fallbackTarget(view, target.role)?.default;
  if (!fallback) return null;
  const bonus = bonusFor(view, fallback.facilityProfileId, target.role, target.filterId);
  return {
    facilityName: fallback.facilityName,
    materialBonusPercent: bonus?.materialBonusPercent ?? fallback.materialBonusPercent,
    timeBonusPercent: bonus?.timeBonusPercent ?? fallback.timeBonusPercent,
    viaFallback: true,
  };
}

/** Every pending suggestion, as the assignments that would accept it. */
export function suggestionAssignments(view: FacilityDefaultsView): FacilityDefaultAssignment[] {
  return view.targets
    .filter((target) => target.suggestion)
    .map((target) => ({
      role: target.role,
      filterId: target.filterId,
      facilityProfileId: target.suggestion!.facilityProfileId,
    }));
}

export interface FacilityDefaultRoles {
  /** Target names this facility is the default for, in view order. */
  targets: string[];
  /** Targets whose rigs no longer cover the target. */
  coverageLost: string[];
}

/** Per facility id, the targets it is the default for. */
export function defaultsByFacility(view: FacilityDefaultsView | null): Map<string, FacilityDefaultRoles> {
  const byFacility = new Map<string, FacilityDefaultRoles>();
  for (const target of view?.targets ?? []) {
    if (!target.default) continue;
    const entry = byFacility.get(target.default.facilityProfileId) ?? { targets: [], coverageLost: [] };
    entry.targets.push(target.name);
    if (target.status === "coverageLost") entry.coverageLost.push(target.name);
    byFacility.set(target.default.facilityProfileId, entry);
  }
  return byFacility;
}
