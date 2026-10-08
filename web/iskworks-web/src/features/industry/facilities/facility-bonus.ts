import type { FacilityProfile } from "../../../api/industry";

export type FacilityBonusKind = "material" | "time";

type BonusRelevantProfile = Pick<
  FacilityProfile,
  "role" | "materialReductionPercent" | "timeReductionPercent" | "rigs"
>;

function percentOf(
  source: { materialReductionPercent: string; timeReductionPercent: string },
  kind: FacilityBonusKind,
): number {
  const raw = kind === "material" ? source.materialReductionPercent : source.timeReductionPercent;
  const value = Number(raw);
  return Number.isFinite(value) ? value : 0;
}

/**
 * The effective facility reduction (as a plain percentage, e.g. `5.04` for
 * 5.04%) for one activity dimension, stacking the structure role bonus and
 * every captured rig's bonus multiplicatively -- the exact combination the
 * API's facility preview applies
 * (`crates/iskworks-core/src/facility/preview.rs::reduction_factor`):
 *
 *   effective = (1 - (1 - structure/100) * ∏(1 - rig_i/100)) * 100
 *
 * Reaction profiles ignore the structure bonus fields entirely -- EVE gives
 * refineries no base reaction discount, so only their rigs stack (see
 * `preview_reaction_facility` and its "ignores structure bonus fields" test).
 *
 * Deliberately product-agnostic: the Facilities list has no blueprint or
 * reaction formula in hand, so the per-Build preview's rig *applicability*
 * filtering can't run here. This number therefore matches what the facility
 * editor exposes -- every captured rig folded in -- and the per-Build
 * worksheet stays the place where product-specific applicability narrows it.
 */
export function effectiveFacilityReductionPercent(
  profile: BonusRelevantProfile,
  kind: FacilityBonusKind,
): number {
  const structurePercent = profile.role === "reaction" ? 0 : percentOf(profile, kind);
  const factor = profile.rigs.reduce(
    (running, rig) => running * (1 - percentOf(rig, kind) / 100),
    1 - structurePercent / 100,
  );
  return (1 - factor) * 100;
}

/**
 * A percentage string with the `%` sign and no misleading trailing zeros,
 * keeping up to two decimals of real precision (`5` -> `"5%"`,
 * `5.04` -> `"5.04%"`, `50.4` -> `"50.4%"`). Sub-0.005 values that are not
 * exactly zero still render as `"0%"`, so callers that want to hide "no
 * bonus" rows should gate on the numeric value, not this string.
 */
export function formatBonusPercent(value: number): string {
  if (!Number.isFinite(value)) return "0%";
  const rounded = Math.round(value * 100) / 100;
  return `${rounded.toLocaleString("en-US", { maximumFractionDigits: 2 })}%`;
}
