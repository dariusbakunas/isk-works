import type { BlueprintObservation, BlueprintSelection } from "../../../../api/industry";

/**
 * The observed blueprint the Build editor should use for `requiredRuns`.
 *
 * The current selection always wins while it is still listed: a copy with
 * fewer licensed runs than required is a valid choice, planned as one job
 * per copy (multi-BPC job split). With no current selection, prefer a copy
 * that covers every run in one job, then an original, then any copy.
 */
export function selectAvailableBlueprint(
  observations: BlueprintObservation[],
  requiredRuns: number,
  currentObservationId = "",
): BlueprintObservation | null {
  const current = observations.find((observation) => observation.id === currentObservationId);
  if (current) return current;
  return observations.find(
    (observation) =>
      observation.kind === "copy" && observation.licensedRuns !== null && observation.licensedRuns >= requiredRuns,
  )
    ?? observations.find((observation) => observation.kind === "original")
    ?? observations.find((observation) => observation.kind === "copy")
    ?? null;
}

/**
 * The current observation a persisted `observedAsset` selection should
 * render as "selected".
 *
 * Each ESI blueprint sync mints fresh `blueprint_observations` rows (new
 * ids, `is_current = true`) for the same physical blueprints and marks the
 * previous batch `is_current = false` -- so a saved
 * `blueprintSelection.observationId` dangles after the next sync. The Build
 * worksheet editor already self-heals this for its own controls
 * (`selectAvailableBlueprint`); the read-only inspector adapters must do
 * the same or they show "not selected" for a build whose blueprint is
 * clearly set.
 *
 * Exact id match wins; otherwise fall back to the best current blueprint
 * for `requiredRuns`, matching the editor's behaviour so the inspector and
 * the Build page agree. `null` when the selection isn't
 * `observedAsset` or nothing current fits (the caller then keeps showing
 * the stale id's frozen ME/TE).
 */
export function reconcileObservedSelection(
  observations: BlueprintObservation[],
  selection: BlueprintSelection | null | undefined,
  requiredRuns: number,
): BlueprintObservation | null {
  if (!selection || selection.mode !== "observedAsset") return null;
  const exact = observations.find((observation) => observation.id === selection.observationId);
  if (exact) return exact;
  return selectAvailableBlueprint(observations, requiredRuns, selection.observationId);
}
