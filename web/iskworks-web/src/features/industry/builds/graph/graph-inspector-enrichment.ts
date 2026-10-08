// Normalises the persisted `Build` behind a lazily-fetched linked Production
// node into one shape the Graph inspector renders. Manufacturing vs Reaction
// is kept explicit: reactions have no blueprint and no ME/TE, and the shape
// never invents them. (The root Production node is not enriched here -- it is
// built directly from the resident editor by `buildRootInspector`.)

import type { BlueprintKind, Build, FacilityProfile } from "../../../../api/industry";

export interface ProductionEnrichment {
  /** Manufacturing blueprint name; `null` for a reaction or not-yet-known. */
  blueprintName: string | null;
  /** BPO / BPC where the persisted Build records it. */
  blueprintOrigin: BlueprintKind | null;
  /** Reaction formula name; `null` for a manufacturing node. */
  formulaName: string | null;
  /** Authoritative blueprint ME / TE, or `null` when not authoritative --
   * a reaction (no such concept) or an observed-asset selection whose real
   * ME/TE isn't on the Build DTO (see `meFromOwnedBlueprint`). */
  me: number | null;
  te: number | null;
  /** Observed-asset manufacturing selection -- render "From owned blueprint"
   * rather than a fabricated ME/TE, and do not fetch the observation. */
  meFromOwnedBlueprint: boolean;
  /** Resolved facility profile for this node's job kind. */
  facility: FacilityProfile | null;
  /** A facility was configured but its profile isn't in `allFacilities`. */
  facilityUnresolved: boolean;
  // ---- root-only (always `null` / `false` for a linked node) ------------
  /** Adjusted, facility-aware planned duration -- root only. */
  plannedDurationSeconds: number | null;
  /** Installation cost, only when the estimate says it's authoritative. */
  installationCost: string | null;
  /** Material + installation, only when the estimate reports it. */
  totalCost: string | null;
  /** Root estimate not ready -- only preview-derived rows show "Computing…". */
  computing: boolean;
}

const EMPTY_ROOT_ONLY = {
  plannedDurationSeconds: null,
  installationCost: null,
  totalCost: null,
  computing: false,
} as const;

/** Enrichment for a selected *linked* Production node, from its persisted
 * `Build` plus the resident facility collection. `effective` carries the
 * authoritative `effectiveMe` / `effectiveTe` off the graph `ProductionNode`
 * (resolved by the same preview the worksheet uses -- so an `observedAsset`
 * selection has concrete values here even though the `Build` DTO only holds
 * an `observationId`). No extra requests. */
export function linkedProductionEnrichment(
  detail: Build,
  allFacilities: FacilityProfile[],
  effective: { me: number | null; te: number | null },
): ProductionEnrichment {
  const isReaction = detail.recipe.kind === "reaction";
  const input = detail.draftPlanning?.input;
  const selection = input?.blueprintSelection ?? null;
  const manual = selection && selection.mode === "manual" ? selection : null;
  const observedAsset = Boolean(selection && selection.mode === "observedAsset");

  const facilityId =
    (isReaction ? input?.reactionFacility : input?.manufacturingFacility)?.facilityProfileId ??
    null;
  const facility = facilityId
    ? allFacilities.find((profile) => profile.id === facilityId) ?? null
    : null;

  // Authoritative effective values from the DTO win; the blueprint-selection
  // fields remain only as a fallback / provenance for when the backend
  // snapshot couldn't run.
  const me = isReaction ? null : effective.me ?? (manual ? manual.materialEfficiency : null);
  const te = isReaction ? null : effective.te ?? (manual ? manual.timeEfficiency : null);

  return {
    blueprintName:
      detail.recipe.kind === "manufacturing" ? detail.recipe.blueprintName : null,
    blueprintOrigin: isReaction ? null : detail.selectedBlueprintOrigin,
    formulaName:
      detail.recipe.kind === "reaction" ? detail.recipe.reactionFormulaName : null,
    me,
    te,
    // Only fall back to the provenance label when we have no concrete value.
    meFromOwnedBlueprint: observedAsset && !isReaction && me == null,
    facility,
    facilityUnresolved: Boolean(facilityId) && !facility,
    ...EMPTY_ROOT_ONLY,
  };
}

